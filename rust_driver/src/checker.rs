use std::{
    cell::{RefCell, RefMut},
    collections::{BTreeMap, HashMap},
    ops::Bound,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use crate::{
    buf::{PacketBuf, RDMA_ACK_BUFFER_SLOT_SIZE},
    device::{
        ToCardCtrlRbDesc, ToCardCtrlRbDescCommon, ToCardCtrlRbDescUpdateErrPsnRecoverPoint,
        ToHostWorkRbDescAck, ToHostWorkRbDescAethCode, ToHostWorkRbDescRead,
        ToHostWorkRbDescWriteOrReadResp, ToHostWorkRbDescWriteType,
    },
    op_ctx::OpCtx,
    qp::QpContext,
    responser::{make_ack, make_nack, make_read_resp},
    retry::RetryMap,
    types::{Msn, Pmtu, Psn, Qpn, PSN_MAX_WINDOW_SIZE},
    utils::calculate_packet_cnt,
    CtrlDescriptorSender, ThreadSafeHashmap, WorkDescriptorSender,
};

use flume::{Receiver, TryRecvError};

use log::{error, info};
use parking_lot::RwLock;

const MAX_MSN_WINDOW_PER_QP: usize = 16;

#[derive(Debug)]
pub(crate) struct PacketChecker {
    thread: Option<std::thread::JoinHandle<()>>,
    stop_flag: Arc<AtomicBool>,
}

pub(crate) struct PacketCheckerContext {
    pub(crate) desc_poller_channel: Receiver<PacketCheckEvent>,
    pub(crate) recv_ctx_map: RecvContextMap,
    pub(crate) qp_table: ThreadSafeHashmap<Qpn, QpContext>,
    pub(crate) user_op_ctx_map: ThreadSafeHashmap<(Qpn, Msn), OpCtx<()>>,
    pub(crate) ctrl_desc_sender: Arc<dyn CtrlDescriptorSender>,
    pub(crate) work_desc_sender: Arc<dyn WorkDescriptorSender>,
    pub(crate) ack_buffers: PacketBuf<RDMA_ACK_BUFFER_SLOT_SIZE>,
    pub(crate) retry_map: RetryMap,
}

impl PacketChecker {
    pub(crate) fn new(mut context: PacketCheckerContext) -> Self {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let thread_stop_flag = Arc::clone(&stop_flag);
        let thread = std::thread::spawn(move || {
            working_thread(&mut context, &thread_stop_flag);
            log::info!("exit checker");
        });
        Self {
            thread: Some(thread),
            stop_flag,
        }
    }
}

impl Drop for PacketChecker {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            if let Err(e) = thread.join() {
                panic!("{}", format!("PacketChecker thread join failed: {e:?}"));
            }
            info!("PacketChecker thread is normally stopped");
        }
    }
}

fn working_thread(ctx: &mut PacketCheckerContext, stop_flag: &AtomicBool) {
    while !stop_flag.load(Ordering::Relaxed) {
        let result = ctx.desc_poller_channel.try_recv();
        match result {
            Err(TryRecvError::Disconnected) => {
                error!("PacketChecker is stopped due to pipe brocken");
                return;
            }
            Err(TryRecvError::Empty) => {}
            Ok(event) => {
                ctx.handle_check_event(event);
            }
        }
    }
}

impl PacketCheckerContext {
    pub(crate) fn handle_check_event(&self, event: PacketCheckEvent) {
        match event {
            PacketCheckEvent::Write(event) => {
                let qpn = event.common.dqpn;
                let expected_psn = event.common.expected_psn;
                let psn = event.psn;
                let enter_error = expected_psn != psn;
                let (mut is_normal, pmtu) = if let Some(qp) = self.qp_table.read().get(&qpn) {
                    (qp.status.load(Ordering::Acquire).is_normal(), qp.pmtu)
                } else {
                    return;
                };
                if is_normal && enter_error {
                    // ensure only enter error status once
                    self.enter_qp_error_status(qpn, pmtu, expected_psn, psn);
                    is_normal = false;
                }
                // log::info!(
                //     "psn={},expect={}",
                //     event.psn.get(),
                //     event.common.expected_psn.get()
                // );
                if is_normal {
                    self.handle_qp_normal(&event);
                } else {
                    self.handle_qp_ooo(&event, pmtu);
                }
            }
            PacketCheckEvent::ReadReq(event) => {
                // convert read req directly
                self.recv_ctx_map.set_recent_msn_status(
                    event.common.dqpn,
                    event.common.msn,
                    RecentQpMsnStatus::Finished,
                );
                if let Ok(desc) = make_read_resp(&self.qp_table, &event) {
                    if let Err(e) = self.work_desc_sender.send_work_desc(desc) {
                        error!("Send read resp failed {:?}", e);
                    }
                }
            }
            PacketCheckEvent::Ack(event) => {
                log::info!("{:?}", event);
                let code = event.code;
                let qpn = event.common.dqpn;
                let msn = event.msn;
                match code {
                    ToHostWorkRbDescAethCode::Ack => {
                        wakeup_user_op_ctx(&self.user_op_ctx_map, qpn, msn);
                    }
                    ToHostWorkRbDescAethCode::Nak => {
                        if let Ok(Some(desc)) = self.retry_map.get_descritpor(
                            (qpn, msn),
                            Some((event.psn.get(), event.common.expected_psn.get())),
                        ) {
                            if let Err(e) = self.work_desc_sender.send_work_desc(desc) {
                                error!("Failed to send retry {:?}", e);
                            }
                        }
                        log::info!("receive nak");
                    }
                    ToHostWorkRbDescAethCode::Rnr | ToHostWorkRbDescAethCode::Rsvd => {
                        // reserved
                    }
                }
            }
        }
    }

    fn handle_qp_normal(&self, event: &ToHostWorkRbDescWriteOrReadResp) {
        let qpn = event.common.dqpn;
        let msn = event.common.msn;

        match event.write_type {
            ToHostWorkRbDescWriteType::First => {
                let status = self.recv_ctx_map.query_recent_msn_status(qpn, msn);
                if matches!(status, RecentQpMsnStatus::NoExist) {
                    let ctx = RecvContext::from(event);
                    self.recv_ctx_map.insert_ctx(qpn, msn, ctx);
                }
            }
            ToHostWorkRbDescWriteType::Last => {
                self.recv_ctx_map.remove_ctx(qpn, msn);
                if !event.is_read_resp {
                    if !event.can_auto_ack {
                        self.send_ack(qpn, msn, event.psn);
                    }
                    let _ignore = self.retry_map.cancel((qpn, msn));
                } else {
                    wakeup_user_op_ctx(&self.user_op_ctx_map, qpn, msn);
                }
            }
            ToHostWorkRbDescWriteType::Only => {
                self.recv_ctx_map
                    .set_recent_msn_status(qpn, msn, RecentQpMsnStatus::Finished);
                #[allow(clippy::else_if_without_else)]
                if event.is_read_resp {
                    wakeup_user_op_ctx(&self.user_op_ctx_map, qpn, msn);
                } else if !event.can_auto_ack {
                    self.send_ack(qpn, msn, event.psn);
                }
            }
            ToHostWorkRbDescWriteType::Middle => {}
        };
    }

    fn send_ack(&self, qpn: Qpn, msn: Msn, psn: Psn) {
        let slot = self.ack_buffers.recycle_buf();
        if let Ok(desc) = make_ack(slot, &self.qp_table, qpn, msn, psn) {
            if let Err(e) = self.work_desc_sender.send_work_desc(desc) {
                error!("Send ack failed {:?}", e);
            }
        } else {
            error!("send ack failed");
        }
    }

    fn send_nack(&self, qpn: Qpn, msn: Msn, start_psn: Psn, end_psn: Psn) {
        let slot = self.ack_buffers.recycle_buf();
        if let Ok(desc) = make_nack(slot, &self.qp_table, qpn, msn, start_psn, end_psn) {
            if let Err(e) = self.work_desc_sender.send_work_desc(desc) {
                error!("Send nack failed {:?}", e);
            }
        } else {
            error!("create nack failed");
        }
    }

    // handle qp that out-of-order
    #[allow(clippy::unwrap_used)]
    fn handle_qp_ooo(&self, event: &ToHostWorkRbDescWriteOrReadResp, pmtu: Pmtu) {
        let qpn = event.common.dqpn;
        let msn = event.common.msn;
        let mut need_check_completed_or_try_recover = false;
        // the qp contect must exist, for previously we have created inside enter_qp_error_status
        let largest_psn_recved = self
            .recv_ctx_map
            .get_per_qp_ctx_mut(qpn)
            .unwrap() // qp_ctx should have created when it enters in error status
            .largest_psn_recved();
        let recved_psn = event.psn;
        match event.write_type {
            ToHostWorkRbDescWriteType::First => {
                let status = self.recv_ctx_map.query_recent_msn_status(qpn, msn);
                if matches!(status, RecentQpMsnStatus::NoExist) {
                    let ctx = RecvContext::new_with_recvmap(event, pmtu);
                    self.recv_ctx_map.insert_ctx(qpn, msn, ctx);
                }
            }
            ToHostWorkRbDescWriteType::Middle | ToHostWorkRbDescWriteType::Last => {
                if let Some(mut ctx) = self.recv_ctx_map.get_ctx_mut(qpn, msn) {
                    let expected_psn = event.common.expected_psn;
                    let range = get_continous_range(largest_psn_recved, expected_psn);
                    let recv_map = ctx.recv_map.as_mut().unwrap();
                    if let Some(continous_range) = range {
                        recv_map.insert(continous_range);
                    }
                    recv_map.insert((recved_psn, recved_psn));
                    if let Some((start_psn, end_psn)) = recv_map.get_recent_gap() {
                        self.send_nack(qpn, msn, start_psn, end_psn)
                    }
                    need_check_completed_or_try_recover = true;
                }
                // otherwise, we ignore this packet,but we should record its psn
            }
            ToHostWorkRbDescWriteType::Only => {
                self.recv_ctx_map
                    .set_recent_msn_status(qpn, msn, RecentQpMsnStatus::Finished);
                #[allow(clippy::else_if_without_else)]
                if event.is_read_resp {
                    wakeup_user_op_ctx(&self.user_op_ctx_map, qpn, msn);
                } else if !event.can_auto_ack {
                    self.send_ack(qpn, msn, event.psn);
                }
            }
        };
        // the qp contect must exist, for previously we have created inside enter_qp_error_status
        self.recv_ctx_map
            .get_per_qp_ctx_mut(qpn)
            .unwrap() // qp_ctx should have created when it enters in error status
            .update_largest_psn_recved(recved_psn);
        if need_check_completed_or_try_recover {
            self.check_completed_and_try_recover(qpn, msn);
        }
    }

    /// Check if corresponding msn is completed and try to recover the qp status
    #[allow(clippy::unwrap_used)]
    fn check_completed_and_try_recover(&self, qpn: Qpn, msn: Msn) {
        let mut perqp_map = self.recv_ctx_map.get_per_qp_ctx_mut(qpn).unwrap();
        let (is_read_resp, is_completed, last_psn, recover_psn) =
            if let Some(ctx) = perqp_map.map.get_mut(&msn) {
                let recv_map = ctx.recv_map.as_ref().unwrap();
                (
                    ctx.is_read_resp,
                    recv_map.is_complete(),
                    recv_map.last_psn(),
                    recv_map.try_get_recover_psn(),
                )
            } else {
                (false, false, Psn::default(), None)
            };

        // decrease borrow
        drop(perqp_map);

        if is_completed {
            self.recv_ctx_map.remove_ctx(qpn, msn);
            if is_read_resp {
                wakeup_user_op_ctx(&self.user_op_ctx_map, qpn, msn);
            } else {
                // we should manually send ack the packet
                self.send_ack(qpn, msn, last_psn);
                let _ignore = self.retry_map.cancel((qpn, msn));
            }
        }

        let perqp_map_len = self.recv_ctx_map.get_per_qp_ctx_mut(qpn).unwrap().map.len();
        // if there is only one recv context and it's in-order,
        // we can try to recover the qp status
        if perqp_map_len <= 1 {
            let largest_psn_recved = self
                .recv_ctx_map
                .get_per_qp_ctx_mut(qpn)
                .unwrap() // qp_ctx should have created when it enters in error status
                .largest_psn_recved();
            if let Some(recover_psn) = recover_psn {
                // recover_psn should be largest_psn_recved + 1
                if recover_psn != largest_psn_recved.wrapping_add(1) {
                    return;
                }
                #[allow(clippy::clone_on_ref_ptr)] // FIXME: refactor later
                let qp_table_ref = self.qp_table.clone();
                try_recover(&self.ctrl_desc_sender, qp_table_ref, qpn, recover_psn);
            }
            // TODO: should we remove qp context at once?
            // if perqp_map_len == 0 {
            //     self.recv_ctx_map.remove_per_qp_ctx(qpn);
            // }
        }
    }

    // store the error status in qp
    fn enter_qp_error_status(&self, qpn: Qpn, pmtu: Pmtu, expected_psn: Psn, recved_psn: Psn) {
        if let Some(qp) = self.qp_table.read().get(&qpn) {
            // set flag
            qp.status
                .store(crate::qp::QpStatus::OutOfOrder, Ordering::Release);
            log::error!("enter error");
        };

        // create context for all msn
        let mut per_qp_map = self
            .recv_ctx_map
            .get_or_create_per_qp_ctx_mut(qpn, recved_psn);
        // we know that if we are previous in the normal status,
        // we should have only one or not recv context left.
        debug_assert!(per_qp_map.map.len() <= 1, "Not in normal status");

        // the expected_psn is the psn that we should receive **next**
        let start_psn = expected_psn.wrapping_sub(1);
        for (_, ctx) in per_qp_map.map.iter_mut() {
            ctx.create_map_on_psn(start_psn, recved_psn, pmtu);
        }
    }
}

fn wakeup_user_op_ctx(
    user_op_ctx_map: &RwLock<HashMap<(Qpn, Msn), OpCtx<()>>>,
    qpn: Qpn,
    msn: Msn,
) {
    if let Some(ctx) = user_op_ctx_map.read().get(&(qpn, msn)) {
        if let Err(e) = ctx.set_result(()) {
            error!("Set result failed {:?}", e);
        }
    } else {
        error!("No op ctx found for {:?}", (qpn, msn));
    }
}

fn try_recover(
    ctrl_desc_sender: &Arc<dyn CtrlDescriptorSender>,
    qp_table: ThreadSafeHashmap<Qpn, QpContext>,
    qpn: Qpn,
    recover_psn: Psn,
) {
    let desc =
        ToCardCtrlRbDesc::UpdateErrorPsnRecoverPoint(ToCardCtrlRbDescUpdateErrPsnRecoverPoint {
            common: ToCardCtrlRbDescCommon::default(),
            qpn,
            recover_psn,
        });
    if let Ok(ctrl_ctx) = ctrl_desc_sender.send_ctrl_desc(desc) {
        ctrl_ctx.set_handler(Box::new(move |is_succ| {
            if is_succ {
                if let Some(qp_ctx) = qp_table.read().get(&qpn) {
                    qp_ctx
                        .status
                        .store(crate::qp::QpStatus::Normal, Ordering::Release);
                }
            }
        }))
    }
}

/// The RecvContextMap is a map from (Qpn, Msn) to RecvContext
#[derive(Debug, Default)]
pub(crate) struct RecvContextMap(RefCell<HashMap<Qpn, PerQpContextMap>>);

#[repr(u8)]
#[derive(Debug, Default, Clone, Copy)]
enum RecentQpMsnStatus {
    #[default]
    NoExist = 0,
    Processing = 1,
    Finished = 2,
    UnKnown = 3,
}

#[derive(Debug)]
pub(crate) struct PerQpContextMap {
    map: BTreeMap<Msn, RecvContext>,
    largest_psn_recved: Psn,
    recent_msn_finished: [(Msn, RecentQpMsnStatus); MAX_MSN_WINDOW_PER_QP],
}

impl PerQpContextMap {
    fn new(largest_psn_recved: Psn) -> Self {
        Self {
            map: BTreeMap::new(),
            largest_psn_recved,
            recent_msn_finished: [(Msn::default(), RecentQpMsnStatus::default());
                MAX_MSN_WINDOW_PER_QP],
        }
    }

    pub(crate) fn update_largest_psn_recved(&mut self, psn: Psn) {
        if !self.largest_psn_recved.larger_in_psn(psn) {
            self.largest_psn_recved = psn;
        }
    }

    pub(crate) fn largest_psn_recved(&self) -> Psn {
        self.largest_psn_recved
    }

    #[allow(clippy::indexing_slicing)]
    fn query_recent_msn_status(&self, msn: Msn) -> RecentQpMsnStatus {
        let idx = msn.get() as usize % MAX_MSN_WINDOW_PER_QP;
        match self.recent_msn_finished[idx] {
            (_, RecentQpMsnStatus::NoExist) => RecentQpMsnStatus::NoExist,
            (_, RecentQpMsnStatus::Processing) => RecentQpMsnStatus::Processing,
            (previous_msn, RecentQpMsnStatus::Finished) => {
                if previous_msn == msn {
                    RecentQpMsnStatus::Finished
                } else {
                    RecentQpMsnStatus::NoExist
                }
            }
            (_, RecentQpMsnStatus::UnKnown) => RecentQpMsnStatus::UnKnown,
        }
    }

    #[allow(clippy::indexing_slicing)]
    fn set_recent_msn_status(&mut self, msn: Msn, status: RecentQpMsnStatus) {
        let idx = msn.get() as usize % MAX_MSN_WINDOW_PER_QP;
        self.recent_msn_finished[idx] = (msn, status);
    }
}

impl RecvContextMap {
    pub(crate) fn new() -> Self {
        Self(HashMap::new().into())
    }

    fn insert_ctx(&self, qpn: Qpn, msn: Msn, ctx: RecvContext) {
        // the `per qp context` might leak here?
        let mut inner = self.0.borrow_mut();
        let per_qp_map = inner
            .entry(qpn)
            .or_insert(PerQpContextMap::new(Psn::default()));
        if per_qp_map.map.insert(msn, ctx).is_some() {
            log::error!("create duplicate msn({:?}) record for qpn={:?}", msn, qpn);
        } else {
            per_qp_map.set_recent_msn_status(msn, RecentQpMsnStatus::Processing);
        }
    }

    fn remove_ctx(&self, qpn: Qpn, msn: Msn) {
        let mut inner = self.0.borrow_mut();
        if let Some(per_qp_map) = inner.get_mut(&qpn) {
            let _dont_care = per_qp_map.map.remove(&msn);
            per_qp_map.set_recent_msn_status(msn, RecentQpMsnStatus::Finished);
        } else {
            log::error!("No recv ctx found for qpn={:?},msn={:?}", qpn, msn);
        }
    }

    fn remove_per_qp_ctx(&self, qpn: Qpn) {
        let mut inner = self.0.borrow_mut();
        let _dont_care = inner.remove(&qpn);
    }

    #[allow(clippy::unwrap_used, clippy::unwrap_in_result)] // the unwrap is checked.
    pub(crate) fn get_ctx_mut(&self, qpn: Qpn, msn: Msn) -> Option<RefMut<RecvContext>> {
        let should_ret_none = {
            let inner = self.0.borrow();
            if inner.contains_key(&qpn) {
                inner.get(&qpn).unwrap().map.contains_key(&msn)
            } else {
                false
            }
        };
        if !should_ret_none {
            None
        } else {
            Some(RefMut::map(self.0.borrow_mut(), |inner| {
                inner.get_mut(&qpn).unwrap().map.get_mut(&msn).unwrap()
            }))
        }
    }
    pub(crate) fn get_or_create_per_qp_ctx_mut(
        &self,
        qpn: Qpn,
        psn: Psn,
    ) -> RefMut<PerQpContextMap> {
        let mut inner = self.0.borrow_mut();
        let _per_qp_map = inner.entry(qpn).or_insert(PerQpContextMap::new(psn));
        drop(inner);
        #[allow(clippy::unwrap_used)] // value is create above.
        RefMut::map(self.0.borrow_mut(), |inner_mut| {
            inner_mut.get_mut(&qpn).unwrap()
        })
    }

    pub(crate) fn get_per_qp_ctx_mut(&self, qpn: Qpn) -> Option<RefMut<PerQpContextMap>> {
        let should_ret_none = {
            let inner = self.0.borrow();
            inner.contains_key(&qpn)
        };
        #[allow(clippy::unwrap_used)] // the unwrap is checked.
        if !should_ret_none {
            None
        } else {
            Some(RefMut::map(self.0.borrow_mut(), |inner| {
                inner.get_mut(&qpn).unwrap()
            }))
        }
    }

    fn query_recent_msn_status(&self, qpn: Qpn, msn: Msn) -> RecentQpMsnStatus {
        let inner = self.0.borrow();
        if let Some(per_qp_map) = inner.get(&qpn) {
            per_qp_map.query_recent_msn_status(msn)
        } else {
            RecentQpMsnStatus::NoExist
        }
    }

    fn set_recent_msn_status(&self, qpn: Qpn, msn: Msn, status: RecentQpMsnStatus) {
        let mut inner = self.0.borrow_mut();
        if let Some(per_qp_map) = inner.get_mut(&qpn) {
            per_qp_map.set_recent_msn_status(msn, status);
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct RecvContext {
    is_read_resp: bool,
    start_addr: u64,
    len_in_bytes: u32,
    start_psn: Psn,
    recv_map: Option<Box<SlidingWindow>>,
}

impl RecvContext {
    pub(crate) fn new_with_recvmap(event: &ToHostWorkRbDescWriteOrReadResp, pmtu: Pmtu) -> Self {
        let pkt_len = calculate_packet_cnt(pmtu, event.addr, event.len);
        let mut map = Box::new(SlidingWindow::new(event.psn, pkt_len));
        map.insert((event.psn, event.psn));
        Self {
            is_read_resp: event.is_read_resp,
            start_addr: event.addr,
            len_in_bytes: event.len,
            start_psn: event.psn,
            recv_map: Some(map),
        }
    }

    pub(crate) fn create_map_on_psn(
        &mut self,
        last_continous_psn: Psn,
        recved_psn: Psn,
        pmtu: Pmtu,
    ) {
        let pkt_len = calculate_packet_cnt(pmtu, self.start_addr, self.len_in_bytes);
        let mut map = Box::new(SlidingWindow::new(self.start_psn, pkt_len));
        map.insert((self.start_psn, last_continous_psn));
        map.insert((recved_psn, recved_psn));
        self.recv_map = Some(map);
    }

    #[cfg(test)]
    pub(crate) fn window(&self) -> Option<&SlidingWindow> {
        self.recv_map.as_deref()
    }
}

impl From<&ToHostWorkRbDescWriteOrReadResp> for RecvContext {
    fn from(event: &ToHostWorkRbDescWriteOrReadResp) -> Self {
        RecvContext {
            is_read_resp: event.is_read_resp,
            start_addr: event.addr,
            len_in_bytes: event.len,
            start_psn: event.psn,
            recv_map: None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct SlidingWindow {
    intervals: BTreeMap<u32, u32>,
    start_psn: Psn,
    num_of_packets: u32,
}

impl SlidingWindow {
    pub(crate) fn new(start: Psn, num_of_packets: u32) -> Self {
        Self {
            intervals: BTreeMap::new(),
            start_psn: start,
            num_of_packets,
        }
    }

    #[allow(clippy::unwrap_in_result, clippy::unwrap_used)]
    fn get_recent_gap(&self) -> Option<(Psn, Psn)> {
        if self.intervals.len() <= 1 {
            return None;
        }
        let mut iter = self.intervals.range(..).rev();
        let (last_left, _) = iter.next().unwrap();
        let (_, second_last_right) = iter.next().unwrap();
        let left = *last_left;
        let right = *second_last_right;
        assert!(left.wrapping_add(1) != right,"Invalid gap");
        Some((
            Psn::new(left.wrapping_add(1)),
            Psn::new(right.wrapping_sub(1)),
        ))
    }

    #[allow(clippy::arithmetic_side_effects, clippy::unwrap_used)]
    fn insert(&mut self, (from, to): (Psn, Psn)) {
        if self.is_complete() {
            return;
        }

        let diff_of_start_psn = from.wrapping_sub(self.start_psn.get()).get();
        if diff_of_start_psn >= PSN_MAX_WINDOW_SIZE {
            return;
        }
        let abs_left = diff_of_start_psn;
        let diff_of_end_psn = to.wrapping_sub(self.start_psn.get()).get();
        if diff_of_end_psn >= PSN_MAX_WINDOW_SIZE {
            return;
        }
        let abs_right = diff_of_end_psn;

        if self.intervals.is_empty() {
            let _: Option<u32> = self.intervals.insert(abs_left, abs_right);
            return;
        }
        let mut merge_left = None;
        let mut merge_right = None;

        if let Some((left_start, left_end)) = self
            .intervals
            .range((Bound::Unbounded, Bound::Included(abs_left)))
            .next_back()
        {
            if abs_left >= *left_start && abs_right <= *left_end {
                return; // exist
            }

            if abs_left <= left_end + 1 {
                merge_left = Some((*left_start, *left_end));
            }
        }

        if let Some((right_start, right_end)) = self
            .intervals
            .range((Bound::Excluded(abs_left), Bound::Unbounded))
            .next()
        {
            if abs_left >= *right_start && abs_right <= *right_end {
                return; // exist
            }

            if abs_right >= right_start - 1 {
                merge_right = Some((*right_start, *right_end));
            }
        }

        match (merge_left, merge_right) {
            (Some((left_start, _)), Some((right_start, right_end))) => {
                let _: Option<u32> = self.intervals.remove(&left_start);
                let _: Option<u32> = self.intervals.remove(&right_start);
                let _: Option<u32> = self.intervals.insert(left_start, right_end);
            }
            (Some((left_start, _)), None) => {
                let _: Option<u32> = self.intervals.remove(&left_start);
                let _: Option<u32> = self.intervals.insert(left_start, abs_right);
            }
            (None, Some((right_start, right_end))) => {
                let _: Option<u32> = self.intervals.remove(&right_start);
                let _: Option<u32> = self.intervals.insert(abs_left, right_end);
            }
            (None, None) => {
                let _: Option<u32> = self.intervals.insert(abs_left, abs_right);
            }
        }
    }

    #[allow(clippy::arithmetic_side_effects)]
    pub(crate) fn is_complete(&self) -> bool {
        if self.intervals.is_empty() {
            return false;
        }
        let (start, end) = self.intervals.first_key_value().unwrap_or((&0, &0));
        *end == self.num_of_packets - 1 && *start == 0
    }

    #[allow(
        clippy::unwrap_used,
        clippy::unwrap_in_result,
        clippy::arithmetic_side_effects,
        clippy::indexing_slicing
    )] // if it's out of order, it must have at least one interval
    pub(crate) fn try_get_recover_psn(&self) -> Option<Psn> {
        (!self.is_out_of_order()).then(|| {
            let offset_of_next_expected = &self.intervals[&0] + 1;
            self.start_psn.wrapping_add(offset_of_next_expected)
        })
    }

    #[allow(clippy::arithmetic_side_effects)]
    pub(crate) fn last_psn(&self) -> Psn {
        // num_of_packets > 0
        self.start_psn.wrapping_add(self.num_of_packets - 1)
    }

    pub(crate) fn is_out_of_order(&self) -> bool {
        !self.is_complete() && self.intervals.len() > 1
    }

    #[cfg(test)]
    pub(crate) fn get_intervals_len(&self) -> usize {
        self.intervals.len()
    }
}

#[derive(Debug, Clone)]
pub(crate) enum PacketCheckEvent {
    Write(ToHostWorkRbDescWriteOrReadResp),
    Ack(ToHostWorkRbDescAck),
    ReadReq(ToHostWorkRbDescRead),
}

impl From<ToHostWorkRbDescWriteOrReadResp> for PacketCheckEvent {
    fn from(desc: ToHostWorkRbDescWriteOrReadResp) -> Self {
        Self::Write(desc)
    }
}

impl From<ToHostWorkRbDescAck> for PacketCheckEvent {
    fn from(desc: ToHostWorkRbDescAck) -> Self {
        Self::Ack(desc)
    }
}

impl From<ToHostWorkRbDescRead> for PacketCheckEvent {
    fn from(desc: ToHostWorkRbDescRead) -> Self {
        Self::ReadReq(desc)
    }
}

impl Default for PacketCheckEvent {
    fn default() -> Self {
        Self::Write(ToHostWorkRbDescWriteOrReadResp::default())
    }
}

fn get_continous_range(largest_psn_recved: Psn, expected_psn: Psn) -> Option<(Psn, Psn)> {
    let left = largest_psn_recved.wrapping_add(1);
    let right = expected_psn.wrapping_sub(1);
    (right.wrapping_sub(left.get()).get() <= PSN_MAX_WINDOW_SIZE).then_some((left, right))
}

#[cfg(test)]
mod tests {

    use std::{
        collections::HashMap,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread::{sleep, spawn},
        time::Duration,
    };

    use parking_lot::{lock_api::RwLock, Mutex};

    use crate::{
        checker::get_continous_range,
        device::ToCardCtrlRbDesc,
        op_ctx::{CtrlOpCtx, OpCtx},
        types::{Msn, Psn, Qpn},
        CtrlDescriptorSender,
    };

    use super::wakeup_user_op_ctx;

    #[test]
    fn test_sliding_window() {
        let start = 0;
        let n = 10;

        // test miss one
        for miss in 1..n - 1 {
            let mut window = super::SlidingWindow::new(Psn::new(start), n);
            for i in 0..n {
                if i != miss {
                    window.insert((Psn::new(i), Psn::new(i)));
                }
            }
            assert!(!window.is_complete());
            assert!(window.is_out_of_order());
            window.insert((Psn::new(miss), Psn::new(miss)));
            assert!(window.is_complete(), "miss={}", miss);
            assert!(!window.is_out_of_order());
        }
        // inseert same psn
        {
            let mut window = super::SlidingWindow::new(Psn::new(start), n);
            for _ in 0..n {
                window.insert((Psn::new(0), Psn::new(0)));
            }
            assert!(!window.is_complete());
        }

        // insert range
        {
            let mut window = super::SlidingWindow::new(Psn::new(50), 20);
            window.insert((Psn::new(50), Psn::new(68)));
            assert!(!window.is_complete());
            assert!(!window.is_out_of_order());
            window.insert((Psn::new(66), Psn::new(69)));
            assert!(window.is_complete());
        }

        // insert out of order range
        {
            let mut window = super::SlidingWindow::new(Psn::new(50), 20);
            window.insert((Psn::new(50), Psn::new(59)));
            assert!(!window.is_complete());
            assert!(!window.is_out_of_order());
            window.insert((Psn::new(59), Psn::new(62)));
            assert!(!window.is_complete());
            assert!(!window.is_out_of_order());
            window.insert((Psn::new(65), Psn::new(68)));
            assert!(!window.is_complete());
            assert!(window.is_out_of_order());
            window.insert((Psn::new(50), Psn::new(64)));
            assert!(!window.is_complete());
            assert!(!window.is_out_of_order());
            window.insert((Psn::new(62), Psn::new(69)));
            assert!(window.is_complete());
        }

        // test cross border
        {
            let base = Psn::new(Psn::MAX_VALUE - 10);
            let mut window = super::SlidingWindow::new(base, 20);
            window.insert((base, base.wrapping_add(9))); //[0,9]
            assert!(!window.is_complete());
            assert!(!window.is_out_of_order());
            window.insert((base.wrapping_add(9), base.wrapping_add(12))); // [9,12]
            assert!(!window.is_complete());
            assert!(!window.is_out_of_order());
            window.insert((base.wrapping_add(15), base.wrapping_add(18))); // [15,18]
            assert!(!window.is_complete());
            assert!(window.is_out_of_order());
            window.insert((base, base.wrapping_add(14))); // [0,14]
            assert!(!window.is_complete());
            assert!(!window.is_out_of_order());
            window.insert((base.wrapping_add(12), base.wrapping_add(19))); // [12,19]
            assert!(window.is_complete());
        }
        // test outside range
        {
            let mut window = super::SlidingWindow::new(Psn::new(start), n);
            window.insert((Psn::new(start), Psn::new(start)));
            assert_eq!(window.intervals.len(), 1);
            window.insert((Psn::new(start), Psn::new(Psn::MAX_VALUE))); // not in range
            assert_eq!(window.intervals.len(), 1);
            window.insert((Psn::new(Psn::MAX_VALUE), Psn::new(start))); // not in range
            assert_eq!(window.intervals.len(), 1);
        }
    }

    #[derive(Debug, Default)]
    struct MockCtrlDescSender(Mutex<Vec<(ToCardCtrlRbDesc, CtrlOpCtx)>>);
    impl CtrlDescriptorSender for MockCtrlDescSender {
        fn send_ctrl_desc(&self, desc: ToCardCtrlRbDesc) -> Result<CtrlOpCtx, crate::Error> {
            let ctx = CtrlOpCtx::new_running();
            self.0.lock().push((desc, ctx.clone()));
            Ok(ctx)
        }
    }

    #[test]
    #[allow(clippy::clone_on_ref_ptr)] //FIXME: refactor later
    fn test_try_recover() {
        use std::collections::HashMap;

        use crate::{
            qp::{QpContext, QpStatus},
            types::Qpn,
        };
        use parking_lot::lock_api::RwLock;

        use super::try_recover;

        let sender = Arc::new(MockCtrlDescSender::default());
        let qpn = Qpn::new(123);
        let qp_table = Arc::new(RwLock::new(HashMap::new()));
        qp_table.write().insert(
            qpn,
            QpContext {
                pd: crate::Pd { handle: 1 },
                qpn,
                status: QpStatus::OutOfOrder.into(),
                ..Default::default()
            },
        );

        let sender_clone: Arc<dyn CtrlDescriptorSender> = Arc::<MockCtrlDescSender>::clone(&sender);

        try_recover(&sender_clone, qp_table.clone(), qpn, Psn::default());

        assert_eq!(sender.0.lock().len(), 1);
        let (desc, ctx) = sender.0.lock().pop().unwrap();
        if let ToCardCtrlRbDesc::UpdateErrorPsnRecoverPoint(desc) = desc {
            assert_eq!(desc.qpn, qpn);
            assert_eq!(desc.recover_psn, Psn::default());
        } else {
            panic!("unexpected desc type");
        }

        let handler = ctx.take_handler().unwrap();
        handler(true);
        let guard = qp_table.read();
        let status = guard
            .get(&qpn)
            .unwrap()
            .status
            .load(std::sync::atomic::Ordering::Acquire);
        assert!(matches!(status, QpStatus::Normal));
    }

    #[test]
    fn test_wakeup_user_op_ctx() {
        let user_op_ctx_map = RwLock::new(HashMap::new());
        let qpn = Qpn::new(123);
        let msn = Msn::new(0x123);
        let ctx = OpCtx::new_running();
        user_op_ctx_map.write().insert((qpn, msn), ctx.clone());
        let flag = Arc::new(AtomicBool::new(false));
        let clone_flag = Arc::<AtomicBool>::clone(&flag);
        spawn(move || {
            let _u = ctx.wait();
            clone_flag.store(true, Ordering::Release);
        });
        wakeup_user_op_ctx(&user_op_ctx_map, qpn, msn);
        sleep(Duration::from_millis(10));
        assert!(flag.load(Ordering::Acquire));
    }

    #[test]
    fn test_recv_ctx() {
        let mut per_qp_map = super::PerQpContextMap::new(Psn::new(10));
        per_qp_map.largest_psn_recved = Psn::new(10);
        per_qp_map.update_largest_psn_recved(Psn::new(20));
        assert_eq!(per_qp_map.largest_psn_recved, Psn::new(20));
        per_qp_map.update_largest_psn_recved(Psn::new(0));
        assert_eq!(per_qp_map.largest_psn_recved, Psn::new(20));
        per_qp_map.update_largest_psn_recved(Psn::new(0));
        assert_eq!(per_qp_map.largest_psn_recved, Psn::new(20));

        let r = get_continous_range(Psn::new(10), Psn::new(20));
        assert_eq!(r, Some((Psn::new(11), Psn::new(19))));
        let r = get_continous_range(Psn::new(10), Psn::new(11));
        assert_eq!(r, None);
        let r = get_continous_range(Psn::new(10), Psn::new(10));
        assert_eq!(r, None);
        let r = get_continous_range(Psn::new(10), Psn::new(9));
        assert_eq!(r, None);
    }
}
