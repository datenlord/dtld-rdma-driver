use std::{
    collections::LinkedList,
    error::Error,
    fmt::Debug,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::spawn,
};

use core_affinity::CoreId;
use flume::{unbounded, Receiver, Sender, TryRecvError};
use log::{debug, error};
use parking_lot::Mutex;

use super::{
    ringbuf::{CsrWriterAdaptor, Ringbuf},
    software::BlueRDMALogic,
    DescSge, DeviceError, ToCardRb, ToCardWorkRbDesc, ToCardWorkRbDescCommon,
};

use crate::{
    types::{Msn, Pmtu, Psn, Qpn},
    utils::{calculate_packet_cnt, get_first_packet_max_length, Buffer},
};

const MAX_SGL_LENGTH: usize = 1;

pub(crate) mod round_robin;
pub(crate) mod testing;

/// A sealed struct of `ToCardWorkRbDesc`
#[derive(Debug, Clone)]
pub struct SealedDesc(Box<ToCardWorkRbDesc>);

impl SealedDesc {
    #[cfg(test)]
    pub(crate) fn new(desc: Box<ToCardWorkRbDesc>) -> Self {
        SealedDesc(desc)
    }

    /// reorder a descripotr
    #[allow(
        clippy::arithmetic_side_effects,
        clippy::indexing_slicing,
        clippy::unwrap_used
    )] // it should only used for testing, so panic is fine
    pub fn reorder(&self, idx_map: Vec<i32>) -> Vec<SealedDesc> {
        let mut results = vec![];
        if let ToCardWorkRbDesc::Write(desc) = &*self.0 {
            let start_raddr = desc.common.raddr;
            let start_laddr = desc.sge0.addr;
            let start_psn = desc.common.psn;
            let pmtu = u64::from(&desc.common.pmtu);
            let cnt = calculate_packet_cnt(desc.common.pmtu, start_raddr, desc.common.total_len);
            let max_len = get_first_packet_max_length(start_raddr, u32::from(&desc.common.pmtu));
            let each_len = (0..cnt)
                .map(|i| {
                    if i == 0 {
                        max_len
                    } else if i == cnt - 1 {
                        (desc.common.total_len - max_len) % (pmtu as u32)
                    } else {
                        pmtu as u32
                    }
                })
                .collect::<Vec<u32>>();
            let mut presums = Vec::with_capacity(each_len.len() + 1);
            let mut presum = 0;
            for i in 0..cnt {
                presums.push(presum);
                presum += each_len[i as usize];
            }
            presums.push(presum);

            let mut all_sgements = vec![];
            all_sgements.push((0_i32, 0_i32));
            let mut last = 0_i32;
            for i in &idx_map[1..] {
                if *i != last + 1_i32 {
                    let (start, _) = all_sgements.pop().unwrap();
                    all_sgements.push((start, last));
                    all_sgements.push((*i, *i));
                }
                last = *i
            }
            let (last_start, _) = all_sgements.pop().unwrap();
            all_sgements.push((last_start, idx_map[idx_map.len() - 1]));

            for (start, end) in all_sgements {
                let is_first = start == 0_i32;
                let is_last = end == cnt as i32 - 1_i32;
                let mut new_desc = desc.clone();
                new_desc.is_first = is_first;
                new_desc.is_last = is_last;
                new_desc.common.psn = start_psn.wrapping_add(start as u32);
                new_desc.common.total_len = presums[end as usize + 1] - presums[start as usize];
                new_desc.sge0.len = new_desc.common.total_len;
                new_desc.common.raddr = start_raddr + presums[start as usize] as u64;
                new_desc.sge0.addr = start_laddr + presums[start as usize] as u64;
                results.push(SealedDesc(Box::new(ToCardWorkRbDesc::Write(new_desc))));
            }
        }
        results
    }

    /// get msn of a descriptor
    pub fn msn(&self) -> Option<Msn> {
        if let ToCardWorkRbDesc::Write(desc) = &*self.0 {
            Some(desc.common.msn)
        } else {
            None
        }
    }
}

/// Size of each batch pop from the scheduler
pub const POP_BATCH_SIZE: usize = 8;

/// A descriptor scheduler that cut descriptor into `SCHEDULER_SIZE` size and schedule with a strategy.
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct DescriptorScheduler<Strat: SchedulerStrategy> {
    sender: Sender<Box<ToCardWorkRbDesc>>,
    receiver: Receiver<Box<ToCardWorkRbDesc>>,
    strategy: Strat,
    thread_handler: Option<std::thread::JoinHandle<()>>,
    stop_flag: Arc<AtomicBool>,
}

/// A batch of descriptors.
pub type BatchDescs = [Option<SealedDesc>; POP_BATCH_SIZE];

/// A scheduler strategy that schedule the descriptor to the device.
#[allow(clippy::module_name_repetitions)]
pub trait SchedulerStrategy: Send + Sync + Clone + 'static {
    /// Push the descriptor to the scheduler, where the descriptor is of same `MSN`.
    #[allow(clippy::linkedlist)]
    fn push<I>(&self, qpn: Qpn, desc: I) -> Result<(), Box<dyn Error>>
    where
        I: Iterator<Item = SealedDesc>;

    /// Pop a batch of descriptors from the scheduler.
    fn pop_batch(&self) -> Result<(BatchDescs, u32), Box<dyn Error>>;
}

struct SGList {
    pub(crate) data: [DescSge; MAX_SGL_LENGTH],
    pub(crate) cur_level: u32,
    pub(crate) len: u32,
}

impl SGList {
    pub(crate) fn new_from_sge(sge: DescSge) -> Self {
        let mut sge_list = Self {
            data: [DescSge::default(); MAX_SGL_LENGTH],
            cur_level: 0,
            len: 1,
        };
        sge_list.data[0] = sge;
        sge_list
    }
}

impl Default for SGList {
    fn default() -> Self {
        Self {
            data: [DescSge::default(); MAX_SGL_LENGTH],
            cur_level: 0,
            len: 0,
        }
    }
}

impl<Strat: SchedulerStrategy> DescriptorScheduler<Strat> {
    pub(super) fn new<
        T: CsrWriterAdaptor + Send + 'static,
        const DEPTH: usize,
        const ELEM_SIZE: usize,
        const PAGE_SIZE: usize,
    >(
        strategy: Strat,
        ringbuf: Mutex<Ringbuf<T, Buffer, DEPTH, ELEM_SIZE, PAGE_SIZE>>,
        core_id: Option<CoreId>,
        scheduler_size: u32,
    ) -> Self {
        let (sender, receiver) = unbounded();
        let thread_receiver: Receiver<Box<ToCardWorkRbDesc>> = receiver.clone();
        let stop_flag = Arc::new(AtomicBool::new(false));
        let thread_stop_flag = Arc::clone(&stop_flag);
        let strategy_clone = strategy.clone();
        let thread_handler = spawn(move || {
            // try set core_affinity, but not exit if set failed.
            if let Some(core_id) = core_id {
                if !core_affinity::set_for_current(core_id) {
                    log::error!("failed to set core_affinity {:?} in scheduler", core_id);
                } else {
                    log::info!("set core_affinity in scheduler successfully");
                }
            }

            while !thread_stop_flag.load(Ordering::Relaxed) {
                let desc = match thread_receiver.try_recv() {
                    Ok(desc) => Some(desc),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => return,
                };
                if let Some(desc) = desc {
                    let dqpn = get_to_card_desc_common(&desc).dqpn;
                    let splited_descs = split_descriptor(desc, scheduler_size);
                    if let Err(e) = strategy.push(dqpn, splited_descs.into_iter()) {
                        error!("failed to push descriptors: {:?}", e);
                    }
                }

                if let Ok((descs, len)) = strategy.pop_batch() {
                    // avoid live lock if no descriptor
                    if len == 0 {
                        continue;
                    }
                    let mut guard = ringbuf.lock();
                    let mut writer = guard.write();
                    #[allow(clippy::unwrap_used, clippy::unwrap_in_result)]
                    for sealed_scheduled_desc in descs.into_iter().flatten() {
                        let scheduled_desc = sealed_scheduled_desc.into_desc();
                        debug!("driver send to card SQ: {:?}", &scheduled_desc);

                        let desc_cnt = scheduled_desc.serialized_desc_cnt();
                        scheduled_desc.write_0(writer.next().unwrap());
                        scheduled_desc.write_1(writer.next().unwrap());
                        scheduled_desc.write_2(writer.next().unwrap());

                        if desc_cnt == 4 {
                            scheduled_desc.write_3(writer.next().unwrap());
                        }
                    }
                }
            }
        });
        Self {
            sender,
            strategy: strategy_clone,
            thread_handler: Some(thread_handler),
            receiver,
            stop_flag,
        }
    }

    pub(crate) fn new_with_software(
        strategy: Strat,
        device: Arc<BlueRDMALogic>,
        scheduler_size: u32,
    ) -> Self {
        let (sender, receiver) = unbounded();
        let thread_receiver: Receiver<Box<ToCardWorkRbDesc>> = receiver.clone();
        let stop_flag = Arc::new(AtomicBool::new(false));
        let thread_stop_flag = Arc::clone(&stop_flag);
        let strategy_clone = strategy.clone();
        let thread_handler = spawn(move || {
            while !thread_stop_flag.load(Ordering::Relaxed) {
                let desc = match thread_receiver.try_recv() {
                    Ok(desc) => Some(desc),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => return,
                };
                if let Some(desc) = desc {
                    let dqpn = get_to_card_desc_common(&desc).dqpn;
                    let splited_descs = split_descriptor(desc, scheduler_size);
                    if let Err(e) = strategy.push(dqpn, splited_descs.into_iter()) {
                        error!("failed to push descriptors: {:?}", e);
                    }
                }

                if let Ok((descs, len)) = strategy.pop_batch() {
                    if len == 0 {
                        continue;
                    }
                    for sealed_scheduled_desc in descs.into_iter().flatten() {
                        let scheduled_desc = sealed_scheduled_desc.into_desc();
                        debug!("driver send to card SQ: {:?}", &scheduled_desc);
                        if let Err(e) = device.send(scheduled_desc) {
                            error!("failed to send descriptor: {:?}", e);
                        }
                    }
                }
            }
        });
        Self {
            sender,
            strategy: strategy_clone,
            thread_handler: Some(thread_handler),
            receiver,
            stop_flag,
        }
    }
}

impl<Strat: SchedulerStrategy> Drop for DescriptorScheduler<Strat> {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread_handler.take() {
            if let Err(e) = thread.join() {
                panic!(
                    "{}",
                    format!("DescriptorScheduler thread join failed: {e:?}")
                );
            }
        }
    }
}

impl<Strat: SchedulerStrategy> ToCardRb<Box<ToCardWorkRbDesc>> for DescriptorScheduler<Strat> {
    fn push(&self, desc: Box<ToCardWorkRbDesc>) -> Result<(), DeviceError> {
        self.sender
            .send(desc)
            .map_err(|e| DeviceError::Scheduler(e.to_string()))
    }
}

impl SealedDesc {
    pub(crate) fn into_desc(self) -> Box<ToCardWorkRbDesc> {
        self.0
    }

    /// Get the destination QPN of the descriptor
    pub fn get_dqpn(&self) -> Qpn {
        match &*self.0 {
            ToCardWorkRbDesc::Read(desc) => desc.common.dqpn,
            ToCardWorkRbDesc::Write(desc) | ToCardWorkRbDesc::ReadResp(desc) => desc.common.dqpn,
            ToCardWorkRbDesc::WriteWithImm(desc) => desc.common.dqpn,
        }
    }

    /// Get the PSN of the descriptor
    pub fn get_psn(&self) -> Psn {
        match &*self.0 {
            ToCardWorkRbDesc::Read(desc) => desc.common.psn,
            ToCardWorkRbDesc::Write(desc) | ToCardWorkRbDesc::ReadResp(desc) => desc.common.psn,
            ToCardWorkRbDesc::WriteWithImm(desc) => desc.common.psn,
        }
    }
}

impl From<Box<ToCardWorkRbDesc>> for SealedDesc {
    fn from(desc: Box<ToCardWorkRbDesc>) -> Self {
        SealedDesc(desc)
    }
}

#[allow(clippy::arithmetic_side_effects)]
fn get_first_schedule_segment_length(va: u64, scheduler_size: u32) -> u32 {
    let offset = va.wrapping_rem(scheduler_size.into());
    // the `SCHEDULER_SIZE` is in range of u32 and offset is less than `SCHEDULER_SIZE`
    // So is safe to convert
    // And the offset is less than `SCHEDULER_SIZE`, which will never downflow
    scheduler_size.wrapping_sub(offset as u32)
}

fn get_to_card_desc_common(desc: &ToCardWorkRbDesc) -> &ToCardWorkRbDescCommon {
    match desc {
        ToCardWorkRbDesc::Read(req) => &req.common,
        ToCardWorkRbDesc::Write(req) | ToCardWorkRbDesc::ReadResp(req) => &req.common,
        ToCardWorkRbDesc::WriteWithImm(req) => &req.common,
    }
}

// We allow indexing_slicing because
// * `new_sgl_level` will always smaller than `origin_sgl` sgl level, which is less than `MAX_SGL_LENGTH`
// * `current_level` won't be greater than `origin_sgl.len`, which is less than `MAX_SGL_LENGTH`
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
fn cut_from_sgl(mut length: u32, origin_sgl: &mut SGList) -> SGList {
    let mut current_level = origin_sgl.cur_level as usize;
    let mut new_sgl = SGList::default();
    let mut new_sgl_level: usize = 0;
    #[allow(clippy::cast_possible_truncation)]
    while (current_level as u32) < origin_sgl.len {
        // if we can cut from current level, just cut and return
        if origin_sgl.data[current_level].len >= length {
            let addr = origin_sgl.data[current_level].addr;
            new_sgl.data[new_sgl_level] = DescSge {
                addr,
                len: length,
                key: origin_sgl.data[current_level].key,
            };
            new_sgl.len = new_sgl_level as u32 + 1;
            origin_sgl.data[current_level].addr = origin_sgl.data[current_level]
                .addr
                .wrapping_add(u64::from(length));
            origin_sgl.data[current_level].len =
                origin_sgl.data[current_level].len.wrapping_sub(length);
            if origin_sgl.data[current_level].len == 0 {
                current_level += 1;
            }
            origin_sgl.cur_level = current_level as u32;
            return new_sgl;
        }
        // otherwise check next level
        let addr = origin_sgl.data[current_level].addr as *mut u8;
        new_sgl.data[new_sgl_level] = DescSge {
            addr: addr as u64,
            len: origin_sgl.data[current_level].len,
            key: origin_sgl.data[current_level].key,
        };
        new_sgl_level += 1;
        length = length.wrapping_sub(origin_sgl.data[current_level].len);
        origin_sgl.data[current_level].len = 0;
        current_level += 1;
    }
    unreachable!("The length is too long");
}

fn get_total_len(desc: &ToCardWorkRbDesc) -> u32 {
    match desc {
        ToCardWorkRbDesc::Read(req) => req.common.total_len,
        ToCardWorkRbDesc::Write(req) | ToCardWorkRbDesc::ReadResp(req) => req.common.total_len,
        ToCardWorkRbDesc::WriteWithImm(req) => req.common.total_len,
    }
}

/// Split the descriptor into multiple descriptors if it is greater than the `scheduler_size` size.
#[allow(clippy::linkedlist)]
pub(crate) fn split_descriptor(
    desc: Box<ToCardWorkRbDesc>,
    scheduler_size: u32,
) -> LinkedList<SealedDesc> {
    let is_read = matches!(*desc, ToCardWorkRbDesc::Read(_));
    let total_len = get_total_len(&desc);
    #[allow(clippy::cast_possible_truncation)]
    if is_read || total_len < scheduler_size {
        let mut list = LinkedList::new();
        list.push_back(SealedDesc(desc));
        return list;
    }

    let (raddr, pmtu, psn, sge) = match &*desc {
        ToCardWorkRbDesc::Read(_) => unreachable!(),
        ToCardWorkRbDesc::Write(req) | ToCardWorkRbDesc::ReadResp(req) => {
            (req.common.raddr, req.common.pmtu, req.common.psn, req.sge0)
        }
        ToCardWorkRbDesc::WriteWithImm(req) => {
            (req.common.raddr, req.common.pmtu, req.common.psn, req.sge0)
        }
    };

    let mut sg_list = SGList::new_from_sge(sge);

    let mut descs = LinkedList::new();
    let mut this_length = get_first_schedule_segment_length(raddr, scheduler_size);
    let mut remain_data_length = total_len;
    let mut current_va = raddr;
    let mut base_psn = psn;
    while remain_data_length > 0 {
        let mut new_desc = desc.clone();
        let new_sge = cut_from_sgl(this_length, &mut sg_list).data[0];
        match &mut *new_desc {
            ToCardWorkRbDesc::Read(_) => unreachable!(),
            ToCardWorkRbDesc::Write(ref mut req) | ToCardWorkRbDesc::ReadResp(ref mut req) => {
                req.sge0 = new_sge;
                req.common.total_len = this_length;
                req.common.raddr = current_va;
                req.common.psn = base_psn;
                req.is_first = false;
                req.is_last = false;
            }
            ToCardWorkRbDesc::WriteWithImm(ref mut req) => {
                req.sge0 = new_sge;
                req.common.total_len = this_length;
                req.common.raddr = current_va;
                req.common.psn = base_psn;
                req.is_first = false;
                req.is_last = false;
            }
        }
        base_psn = recalculate_psn(current_va, pmtu, this_length, base_psn);
        descs.push_back(SealedDesc(new_desc));
        current_va = current_va.wrapping_add(u64::from(this_length));
        remain_data_length = remain_data_length.wrapping_sub(this_length);
        this_length = if remain_data_length > scheduler_size {
            scheduler_size
        } else {
            remain_data_length
        };
    }
    // The above code guarantee there at least 2 descriptors in the list
    if let Some(req) = descs.front_mut() {
        match &mut *req.0 {
            ToCardWorkRbDesc::Read(_) => unreachable!(),
            ToCardWorkRbDesc::Write(ref mut req) | ToCardWorkRbDesc::ReadResp(ref mut req) => {
                req.is_first = true;
                req.common.total_len = total_len;
            }
            ToCardWorkRbDesc::WriteWithImm(ref mut req) => {
                req.is_first = true;
                req.common.total_len = total_len;
            }
        }
    }

    if let Some(req) = descs.back_mut() {
        match &mut *req.0 {
            ToCardWorkRbDesc::Read(_) => unreachable!(),
            ToCardWorkRbDesc::Write(ref mut req) | ToCardWorkRbDesc::ReadResp(ref mut req) => {
                req.is_last = true;
            }
            ToCardWorkRbDesc::WriteWithImm(ref mut req) => {
                req.is_last = true;
            }
        }
    }

    descs
}

/// Recalculate the PSN of the descriptor
///
/// # Example
/// `base_psn` = 0 , `desc.raddr` = 4095,`pmtu` = 4096 and `desc.common_header.total_len` = 4096 * 4.
///
/// so the `first_packet_length` = 4096 - 4095 = 1
/// then the psn = 0 + ceil((4096 * 4 - `first_packet_length`),4096) = 4
/// That means we will send 5 packets in total(psn=0,1,2,3,4)
/// And the next psn will be 5
fn recalculate_psn(raddr: u64, pmtu: Pmtu, total_len: u32, base_psn: Psn) -> Psn {
    let pmtu = u32::from(&pmtu);
    let first_packet_length = get_first_packet_max_length(raddr, pmtu);
    let first_packet_length = total_len.min(first_packet_length);
    // first packet psn = base_psn
    // so the total psn = base_psn + (desc.common_header.total_len - first_packet_length) / pmtu + 1
    #[allow(clippy::arithmetic_side_effects)] // total always greater than first_packet_length
    let last_packet_psn = base_psn.wrapping_add((total_len - first_packet_length).div_ceil(pmtu));
    last_packet_psn.wrapping_add(1)
}

#[cfg(test)]
mod test {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::thread::sleep;
    use std::{collections::LinkedList, sync::Arc};

    use parking_lot::lock_api::Mutex;

    use crate::device::ringbuf::{CsrWriterAdaptor, Ringbuf};
    use crate::device::{
        DescSge, DeviceError, ToCardRb, ToCardWorkRbDesc, ToCardWorkRbDescCommon,
        ToCardWorkRbDescWrite, ToCardWorkRbDescWriteWithImm,
    };

    use crate::types::{Key, Msn, Qpn, WorkReqSendFlag};
    use crate::utils::Buffer;
    use crate::SealedDesc;

    use super::{SGList, MAX_SGL_LENGTH};

    pub(crate) struct SGListBuilder {
        sg_list: Vec<DescSge>,
    }

    impl SGListBuilder {
        pub(crate) fn new() -> Self {
            SGListBuilder {
                sg_list: Vec::new(),
            }
        }

        pub(crate) fn with_sge(&mut self, addr: u64, len: u32, key: Key) -> &mut Self {
            self.sg_list.push(DescSge { addr, len, key });
            self
        }

        pub(crate) fn build(&self) -> SGList {
            let mut sg_list = SGList::default();
            for sge in self.sg_list.iter() {
                sg_list.data[sg_list.len as usize] = *sge;
                sg_list.len += 1;
            }
            while sg_list.len < MAX_SGL_LENGTH.try_into().unwrap() {
                sg_list.data[sg_list.len as usize] = DescSge {
                    addr: 0,
                    len: 0,
                    key: Key::default(),
                };
            }
            sg_list
        }
    }

    #[test]
    fn test_helper_function_first_length() {
        const SCHEDULER_SIZE: u32 = 1024 * 32;
        let length = super::get_first_schedule_segment_length(0, SCHEDULER_SIZE);
        assert_eq!(length, 1024 * 32);
        let length = super::get_first_schedule_segment_length(1024 * 29, SCHEDULER_SIZE);
        assert_eq!(length, 1024 * 3);
        let length = super::get_first_schedule_segment_length(1024 * 32 + 1, SCHEDULER_SIZE);
        assert_eq!(length, 1024 * 32 - 1);
    }

    #[test]
    fn test_cut_from_sgl() {
        let mut sgl = SGListBuilder::new()
            .with_sge(0, 1024, Key::default())
            .build();
        let new_sgl = super::cut_from_sgl(512, &mut sgl);
        assert_eq!(new_sgl.len, 1);
        assert_eq!(new_sgl.data[0].len, 512);
        assert_eq!(sgl.data[0].len, 512);
        assert_eq!(sgl.data[0].addr, 512);
    }

    #[allow(dead_code)]
    fn convert_list_to_vec<T>(list: LinkedList<T>) -> Vec<T> {
        let mut vec = Vec::new();
        for i in list {
            vec.push(i);
        }
        vec
    }

    #[derive(Default, Debug, Clone)]
    struct Proxy(Arc<ProxyInner>);

    #[derive(Default, Debug)]
    struct ProxyInner {
        head: AtomicU32,
        tail: AtomicU32,
    }
    impl CsrWriterAdaptor for Proxy {
        fn write_head(&self, data: u32) -> Result<(), DeviceError> {
            self.0.head.store(data, Ordering::Release);
            Ok(())
        }
        fn read_tail(&self) -> Result<u32, DeviceError> {
            Ok(self.0.tail.load(Ordering::Acquire))
        }
    }
    #[test]
    fn test_scheduler() {
        let va = 29 * 1024;
        let length = 1024 * 36; // should cut into 3 segments: 29k - 32k, 32k - 64k, 64k-65k
        let strategy = super::round_robin::RoundRobinStrategy::new();
        let buffer = Buffer::new(4096, false).unwrap();
        let proxy = Proxy::default();
        let ringbuf = Mutex::new(Ringbuf::<Proxy, Buffer, 128, 32, 4096>::new(
            proxy.clone(),
            buffer,
        ));
        let scheduler = Arc::new(super::DescriptorScheduler::new(
            strategy,
            ringbuf,
            None,
            1024 * 32,
        ));
        let desc = ToCardWorkRbDesc::Write(ToCardWorkRbDescWrite {
            common: ToCardWorkRbDescCommon {
                total_len: length,
                raddr: va,
                dqpn: Qpn::new(2),
                pmtu: crate::types::Pmtu::Mtu4096,
                flags: WorkReqSendFlag::empty(),
                msn: Msn::new(0x27),
                ..Default::default()
            },
            sge0: DescSge {
                addr: 0,
                len: length,
                key: Key::new(3),
            },
            ..Default::default()
        })
        .into();
        scheduler.push(desc).unwrap();

        sleep(std::time::Duration::from_millis(10));
        let head = proxy.0.head.load(Ordering::Acquire);
        assert_eq!(head, 9); // 3 descriptors, each has 3 segments
                             // we do not check it accuracy here

        // test a raw packet
        let desc = ToCardWorkRbDesc::WriteWithImm(ToCardWorkRbDescWriteWithImm {
            common: ToCardWorkRbDescCommon {
                total_len: 66,
                qp_type: crate::types::QpType::RawPacket,
                ..Default::default()
            },
            ..Default::default()
        })
        .into();
        scheduler.push(desc).unwrap();
        sleep(std::time::Duration::from_millis(10));
        let head = proxy.0.head.load(Ordering::Acquire);
        assert_eq!(head, 9 + 3); // 1 descriptor, which has 3 segments
    }

    #[test]
    fn test() {
        let va = 128;
        let desc = SealedDesc::new(Box::new(ToCardWorkRbDesc::Write(ToCardWorkRbDescWrite {
            common: ToCardWorkRbDescCommon {
                total_len: 256 * 10,
                raddr: va,
                dqpn: Qpn::new(2),
                pmtu: crate::types::Pmtu::Mtu256,
                flags: WorkReqSendFlag::empty(),
                msn: Msn::new(0x27),
                ..Default::default()
            },
            sge0: DescSge {
                addr: 0,
                len: 256 * 10,
                key: Key::new(3),
            },
            ..Default::default()
        })));
        let results = desc.reorder(vec![0, 1, 5, 4, 2, 3, 6, 7, 8, 9, 10]);
        assert_eq!(results.len(), 5);
        // should cut into [0,1] [5],[4] [2,3],[6,10]
    }
}
