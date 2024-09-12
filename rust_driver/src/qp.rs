use atomic_enum::atomic_enum;
use eui48::MacAddress;

use crate::{
    device::{ToCardCtrlRbDesc, ToCardCtrlRbDescCommon, ToCardCtrlRbDescQpManagement},
    types::{MemAccessTypeFlag, Msn, Pmtu, Psn, Qp, QpType, Qpn},
    Device, Error, Pd,
};
use std::{
    hash::{Hash, Hasher},
    net::Ipv4Addr,
    sync::atomic::{AtomicBool, AtomicU16, Ordering},
};

use parking_lot::Mutex;

const QP_MAX_CNT: usize = 1024;

/// The status of current QP
#[atomic_enum]
#[non_exhaustive]
#[derive(PartialEq, Eq)]
pub enum QpStatus {
    /// The QP is normal
    Normal = 0,
    /// The QP is out of order
    OutOfOrder = 1,
}

impl QpStatus{
    pub(crate) fn is_normal(&self) -> bool{
        matches!(self, QpStatus::Normal)
    }
}

impl Default for QpStatus {
    fn default() -> Self {
        Self::Normal
    }
}

/// QP context
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct QpContext {
    pub(crate) pd: Pd,
    pub(crate) qpn: Qpn,
    pub(crate) peer_qpn: Qpn,
    pub(crate) qp_type: QpType,
    #[allow(unused)] // a field of QP, we may use it later
    pub(crate) rq_acc_flags: MemAccessTypeFlag,
    pub(crate) pmtu: Pmtu,
    pub(crate) local_mac: MacAddress,
    pub(crate) local_ip: Ipv4Addr,
    pub(crate) dqp_ip: Ipv4Addr,
    pub(crate) dqp_mac_addr: MacAddress,
    pub(crate) sending_psn: Mutex<Psn>,
    pub(crate) status: AtomicQpStatus,
    pub(crate) _next_msn: AtomicU16,
}

impl QpContext {
    /// create a qp context
    ///
    /// currently, `sending_psn` is set to 0 at begining
    #[must_use]
    pub fn new(qp: &Qp, local_ip: Ipv4Addr, local_mac: MacAddress) -> Self {
        Self {
            pd: qp.pd,
            qpn: qp.qpn,
            peer_qpn: qp.peer_qpn,
            qp_type: qp.qp_type,
            rq_acc_flags: qp.rq_acc_flags,
            pmtu: qp.pmtu,
            local_ip,
            local_mac,
            dqp_ip: qp.dqp_ip,
            dqp_mac_addr: qp.dqp_mac,
            sending_psn: Mutex::new(Psn::new(0)),
            status: AtomicQpStatus::new(QpStatus::Normal),
            _next_msn: AtomicU16::default(),
        }
    }

    pub(crate) fn next_msn(&self) -> Msn {
        Msn::new(self._next_msn.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for QpContext {
    fn default() -> Self {
        Self {
            pd: Pd::default(),
            qpn: Default::default(),
            peer_qpn: Default::default(),
            qp_type: QpType::Rc,
            rq_acc_flags: MemAccessTypeFlag::empty(),
            pmtu: Pmtu::Mtu4096,
            local_mac: Default::default(),
            local_ip: Ipv4Addr::LOCALHOST,
            dqp_ip: Ipv4Addr::LOCALHOST,
            dqp_mac_addr: Default::default(),
            sending_psn: Default::default(),
            status: AtomicQpStatus::new(QpStatus::Normal),
            _next_msn: Default::default(),
        }
    }
}

impl Device {
    /// create a qp
    ///
    /// # Errors
    ///
    /// Will return `Err` if:
    /// * lock poisoned
    /// * opeartion failed
    /// * Operating system not support
    /// * Setted context result failed
    pub fn create_qp(&self, qp: &Qp) -> Result<(), Error> {
        let mut qp_pool = self.0.qp_table.write();
        let mut pd_pool = self.0.pd.lock();
        let pd = &qp.pd;
        let pd_ctx = pd_pool
            .get_mut(pd)
            .ok_or(Error::Invalid(format!("PD :{pd:?}")))?;

        let qpc = QpContext::new(
            qp,
            self.0.local_network.ipaddr,
            self.0.local_network.macaddr,
        );
        let op_id = self.get_ctrl_op_id();

        let desc = ToCardCtrlRbDesc::QpManagement(ToCardCtrlRbDescQpManagement {
            common: ToCardCtrlRbDescCommon { op_id },
            is_valid: true,
            qpn: qp.qpn,
            pd_hdl: qp.pd.handle,
            qp_type: qp.qp_type,
            rq_acc_flags: qp.rq_acc_flags,
            pmtu: qp.pmtu,
            peer_qpn: qp.peer_qpn
        });

        let ctx = self.do_ctrl_op(op_id, desc)?;

        let res = ctx.wait_result()?.ok_or(Error::SetCtxResultFailed)?;

        if !res {
            return Err(Error::DeviceReturnFailed("create qp"));
        }

        let pd_res = pd_ctx.qp.insert(qp.qpn);
        if !pd_res {
            return Err(Error::Invalid(format!("qp :{0:?}", qp.qpn)));
        }

        let qp_res = qp_pool.insert(qp.qpn, qpc);
        if qp_res.is_some() {
            return Err(Error::Invalid(format!("qp :{0:?}", qp.qpn)));
        }

        Ok(())
    }

    /// destory a qp
    ///
    /// # Errors
    ///
    /// Will return `Err` if:
    /// * lock poisoned
    /// * opeartion failed
    /// * Setted context result failed
    pub fn destroy_qp(&self, qp: Qpn) -> Result<(), Error> {
        let mut qp_pool = self.0.qp_table.write();
        let mut pd_pool = self.0.pd.lock();

        let op_id = self.get_ctrl_op_id();

        let (pd_ctx, desc) = if let Some(qp_ctx) = qp_pool.get(&qp) {
            let pd_ctx = pd_pool
                .get_mut(&qp_ctx.pd)
                .ok_or(Error::Invalid(format!("PD :{:?}", &qp_ctx.pd)))?;
            let desc = ToCardCtrlRbDesc::QpManagement(ToCardCtrlRbDescQpManagement {
                common: ToCardCtrlRbDescCommon { op_id },
                is_valid: false,
                qpn: qp_ctx.qpn,
                pd_hdl: 0,
                qp_type: qp_ctx.qp_type,
                rq_acc_flags: MemAccessTypeFlag::IbvAccessNoFlags,
                pmtu: qp_ctx.pmtu,
                peer_qpn: qp_ctx.peer_qpn
            });
            (pd_ctx, desc)
        } else {
            return Err(Error::Invalid(format!("Qpn :{qp:?}")));
        };

        let ctx = self.do_ctrl_op(op_id, desc)?;

        let res = ctx.wait_result()?.ok_or(Error::SetCtxResultFailed)?;

        if !res {
            return Err(Error::DeviceReturnFailed("destroy qp"));
        }

        let _: bool = pd_ctx.qp.remove(&qp);
        let _: Option<QpContext> = qp_pool.remove(&qp);

        Ok(())
    }
}

impl Hash for Qp {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.qpn.hash(state);
    }
}

impl PartialEq for Qp {
    fn eq(&self, other: &Self) -> bool {
        self.qpn == other.qpn
    }
}

impl Eq for Qp {}

/// QP manager
///
/// The QP manager is used to allocate and free QP numbers(QPN).
///
/// The max QP number is `QP_MAX_CNT`, and QP0 and QP1 are reserved.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct QpManager {
    qp_availability: Box<[AtomicBool]>,
}

impl QpManager {
    /// create a QP manager
    #[must_use]
    pub fn new() -> Self {
        let qp_availability: Vec<AtomicBool> =
            (0..QP_MAX_CNT).map(|_| AtomicBool::new(true)).collect();

        // by IB spec, QP0 and QP1 are reserved, so qpn should start with 2
        #[allow(clippy::indexing_slicing)]
        {
            qp_availability[0].store(false, Ordering::Relaxed);
            qp_availability[1].store(false, Ordering::Relaxed);
        }

        Self {
            qp_availability: qp_availability.into_boxed_slice(),
        }
    }

    /// allocate a qp number
    ///
    /// # Errors
    ///
    /// Will return `Err` if:
    /// not have enough qp number
    pub fn alloc(&self) -> Result<Qpn, Error> {
        // QP_MAX_CNT is guaranteed to be less than u32::MAX by RDMA spec.
        #[allow(clippy::cast_possible_truncation)]
        self.qp_availability
            .iter()
            .enumerate()
            .find_map(|(idx, n)| {
                n.swap(false, Ordering::AcqRel)
                    .then_some(Qpn::new(idx as u32))
            })
            .ok_or_else(|| Error::ResourceNoAvailable("QP".to_owned()))
    }

    /// free a qp number
    ///
    /// If the QP number is not allocated, it will be ignored.
    pub fn free(&self, qpn: Qpn) {
        if let Some(qp_availability) = self.qp_availability.get(qpn.get() as usize) {
            qp_availability.store(true, Ordering::Relaxed);
        }
    }
}

impl Default for QpManager {
    fn default() -> Self {
        Self::new()
    }
}
