use std::{
    collections::{hash_map::Entry, HashMap},
    fmt::Debug,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::sleep,
    time::{Duration, SystemTime, UNIX_EPOCH},
    u128,
};

use parking_lot::{Mutex, RwLock};

use crate::{
    device::ToCardWorkRbDesc,
    op_ctx::OpCtx,
    types::{Msn, Pmtu, Qpn},
    utils::{calculate_packet_cnt, get_first_packet_max_length},
    Error, ThreadSafeHashmap, WorkDescriptorSender,
};

#[derive(Debug)]
pub(crate) struct RetryContext {
    descriptor: Box<ToCardWorkRbDesc>,
    retry_counter: u32,
    next_timeout: u128,
    is_initiative: bool,
}

#[allow(clippy::type_complexity)]
#[derive(Debug, Clone)]
pub(crate) struct RetryMap {
    max_retry: u32,
    retry_timeout: u128,
    map: Arc<RwLock<HashMap<(Qpn, Msn), Arc<Mutex<RetryContext>>>>>,
}

/// Giving the absolute PSN, return the offset to that PSN
#[allow(clippy::arithmetic_side_effects)] //wrapping_div is safe
fn psn_addr_offset(base_addr: u64, pmtu: Pmtu, psn: u32) -> u64 {
    if psn == 0 {
        return 0;
    }
    let first_pkt_length: u64 = get_first_packet_max_length(base_addr, u32::from(&pmtu)).into();
    let second_pkt_addr = base_addr.wrapping_add(first_pkt_length);
    let pmtu = u64::from(&pmtu);
    let psn: u64 = psn.into();
    let next_addr = (psn.wrapping_sub(1).wrapping_mul(pmtu)).wrapping_add(second_pkt_addr);
    next_addr.wrapping_sub(base_addr)
}

impl RetryMap {
    pub(crate) fn new(max_retry: u32, retry_timeout: Duration) -> Self {
        Self {
            map: Arc::new(RwLock::new(HashMap::new())),
            retry_timeout: retry_timeout.as_millis(),
            max_retry,
        }
    }

    pub(crate) fn add(
        &self,
        key: (Qpn, Msn),
        descriptor: Box<ToCardWorkRbDesc>,
        is_initiative: bool,
    ) -> bool {
        let mut guard = self.map.write();
        let next_timeout = get_current_time().wrapping_add(self.retry_timeout);
        if let Entry::Vacant(entry) = guard.entry(key) {
            let _inner = entry.insert(Arc::new(Mutex::new(RetryContext {
                descriptor,
                retry_counter: self.max_retry,
                next_timeout,
                is_initiative,
            })));
            false
        } else {
            true
        }
    }

    pub(crate) fn cancel(&self, key: (Qpn, Msn)) -> bool {
        let mut map = self.map.write();
        map.remove(&key).is_none()
    }

    /// fetch the descriptor from the map.
    ///
    /// If a range is given, the descriptor will be cut into the range
    pub(crate) fn get_descritpor(
        &self,
        key: (Qpn, Msn),
        range: Option<(u32, u32)>,
    ) -> Result<Option<Box<ToCardWorkRbDesc>>, Error> {
        let guard = self.map.read();
        let mut desc = if let Some(ctx) = guard.get(&key) {
            let inner = ctx.lock();
            inner.descriptor.clone()
        } else {
            return Ok(None);
        };
        if let Some((from, to)) = range {
            if let ToCardWorkRbDesc::Write(ref mut write_desc) = *desc {
                let pmtu = write_desc.common.pmtu;
                let max_pkt = calculate_packet_cnt(
                    pmtu,
                    write_desc.common.raddr,
                    write_desc.common.total_len,
                );
                if from > to || from >= max_pkt || to >= max_pkt {
                    return Err(Error::Invalid("Invalid psn range".to_owned()));
                }
                let start_offset = psn_addr_offset(write_desc.sge0.addr, pmtu, from);
                let end_offset = psn_addr_offset(write_desc.sge0.addr, pmtu, to);
                let new_length = end_offset.wrapping_sub(start_offset);

                write_desc.is_first = from == 0;
                write_desc.is_last = max_pkt.wrapping_sub(1) == to;
                write_desc.sge0.addr = write_desc.sge0.addr.wrapping_add(start_offset);
                write_desc.sge0.len = new_length as u32;
                write_desc.common.psn = write_desc.common.psn.wrapping_add(from);
                write_desc.common.total_len = new_length as u32;
                write_desc.common.raddr = write_desc.common.raddr.wrapping_add(start_offset);
            } else {
                return Err(Error::Invalid("Invalid descriptor type".to_owned()));
            }
        };
        Ok(Some(desc))
    }

    fn iter_key_and_value(&self) -> Vec<(Qpn, Msn, Arc<Mutex<RetryContext>>)> {
        let guard = self.map.read();
        guard
            .iter()
            .map(|(k, v)| (k.0, k.1, Arc::<Mutex<RetryContext>>::clone(v)))
            .collect()
    }

    fn clean_exceed_timeout(&self) {
        let mut guard = self.map.write();
        guard.retain(|_, ctx| {
            let inner = ctx.lock();
            inner.retry_counter > 0
        });
    }
}

/// Typically the checking_interval should at most 1% of retry_timeout
/// So that the retrying won't drift too much
#[derive(Debug, Clone, Copy)]
pub struct RetryConfig {
    pub(crate) is_enable: bool,
    pub(crate) max_retry: u32,
    pub(crate) retry_timeout: Duration,
    pub(crate) checking_interval: Duration,
}

impl RetryConfig {
    /// Create a new retry config
    pub fn new(
        is_enable: bool,
        max_retry: u32,
        retry_timeout: Duration,
        checking_interval: Duration,
    ) -> Self {
        Self {
            is_enable,
            max_retry,
            retry_timeout,
            checking_interval,
        }
    }
}

// Main thread will send a retry record to retry monitor
pub(crate) struct RetryRecord {
    descriptor: Box<ToCardWorkRbDesc>,
    qpn: Qpn,
    msn: Msn,
    initiative: bool,
}

impl RetryRecord {
    ///
    pub(crate) fn new(
        descriptor: Box<ToCardWorkRbDesc>,
        qpn: Qpn,
        msn: Msn,
        initiative: bool,
    ) -> Self {
        Self {
            descriptor,
            qpn,
            msn,
            initiative,
        }
    }
}

pub(crate) struct RetryMonitorContext {
    pub(crate) map: RetryMap,
    pub(crate) device: Arc<dyn WorkDescriptorSender>,
    pub(crate) user_op_ctx_map: ThreadSafeHashmap<(Qpn, Msn), OpCtx<()>>,
    pub(crate) config: RetryConfig,
}

/// get current time in ms
#[allow(clippy::unwrap_used)]
fn get_current_time() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

#[derive(Debug)]
pub(crate) struct RetryMonitor {
    stop_flag: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl RetryMonitor {
    pub(crate) fn new(mut context: RetryMonitorContext) -> Self {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = Arc::<AtomicBool>::clone(&stop_flag);
        let thread = std::thread::spawn(move || {
            if context.config.is_enable {
                retry_monitor_working_thread(&stop_flag_clone, &mut context);
            }
        });
        Self {
            stop_flag,
            thread: Some(thread),
        }
    }
}

impl RetryMonitorContext {
    #[allow(clippy::arithmetic_side_effects)]
    fn check_timeout(&mut self) {
        let now = get_current_time();
        let mut has_removed = false;
        for (qpn, msn, ctx) in self.map.iter_key_and_value() {
            let mut guard = ctx.lock();
            if guard.is_initiative && guard.next_timeout <= now {
                if guard.retry_counter > 0 {
                    guard.retry_counter -= 1;
                    guard.next_timeout = now + self.config.retry_timeout.as_millis();
                    if self
                        .device
                        .send_work_desc(guard.descriptor.clone())
                        .is_err()
                    {
                        log::error!("Retry send work descriptor failed")
                    } else {
                        log::warn!("Retry desc:{:?}", guard.descriptor);
                    }
                } else {
                    // Encounter max retry, remove it and tell user the error
                    has_removed = true;
                    let user_op_ctx_guard = self.user_op_ctx_map.write();
                    if let Some(user_op_ctx) = user_op_ctx_guard.get(&(qpn, msn)) {
                        user_op_ctx.set_error("exceed max retry count");
                    } else {
                        log::warn!("Remove retry record failed: Can not find {:?}", (qpn, msn));
                    }
                }
            }
        }

        if has_removed {
            self.map.clean_exceed_timeout();
        }
    }
}

fn retry_monitor_working_thread(stop_flag: &AtomicBool, monitor: &mut RetryMonitorContext) {
    while !stop_flag.load(Ordering::Relaxed) {
        // monitor.check_receive();
        monitor.check_timeout();
        // sleep for an interval
        sleep(monitor.config.checking_interval);
    }
}

#[cfg(test)]
mod test {
    use std::{collections::HashMap, sync::Arc, time::Duration};

    use parking_lot::{lock_api::RwLock, Mutex, RawRwLock};

    use crate::{
        device::{DescSge, ToCardWorkRbDesc, ToCardWorkRbDescCommon, ToCardWorkRbDescWrite},
        op_ctx::{self, CtxStatus},
        retry::RetryMap,
        types::{Key, Msn, Qpn, ThreeBytesStruct},
        Error, WorkDescriptorSender,
    };

    use super::{psn_addr_offset, RetryConfig, RetryMonitorContext};
    struct MockDevice(Mutex<Vec<ToCardWorkRbDesc>>);

    impl WorkDescriptorSender for MockDevice {
        fn send_work_desc(&self, desc: Box<ToCardWorkRbDesc>) -> Result<(), Error> {
            self.0.lock().push(*desc);
            Ok(())
        }
    }
    #[test]
    fn test_psn_addr_offset() {
        assert_eq!(psn_addr_offset(0, crate::types::Pmtu::Mtu2048, 0), 0);
        assert_eq!(psn_addr_offset(0, crate::types::Pmtu::Mtu2048, 1), 2048);
        assert_eq!(psn_addr_offset(0, crate::types::Pmtu::Mtu2048, 2), 4096);
        assert_eq!(psn_addr_offset(1234, crate::types::Pmtu::Mtu2048, 0), 0);
        assert_eq!(psn_addr_offset(1234, crate::types::Pmtu::Mtu2048, 1), 814);
        assert_eq!(psn_addr_offset(1234, crate::types::Pmtu::Mtu2048, 2), 2862);
    }
    #[test]
    fn test_retry_monitor() {
        let map = Arc::new(RwLock::new(HashMap::new()));
        let device = Arc::new(MockDevice(Vec::new().into()));
        let retry_map = RetryMap::new(3, Duration::from_millis(1000));
        let context = RetryMonitorContext {
            map: retry_map.clone(),
            device: Arc::<MockDevice>::clone(&device),
            user_op_ctx_map: Arc::<
                RwLock<RawRwLock, HashMap<(ThreeBytesStruct, Msn), op_ctx::OpCtx<()>>>,
            >::clone(&map),
            config: RetryConfig::new(
                true,
                3,
                Duration::from_millis(1000),
                std::time::Duration::from_millis(10),
            ),
        };
        map.write().insert(
            (Qpn::default(), Msn::default()),
            op_ctx::OpCtx::new_running(),
        );
        let _monitor = super::RetryMonitor::new(context);
        let desc = Box::new(ToCardWorkRbDesc::Write(ToCardWorkRbDescWrite {
            common: ToCardWorkRbDescCommon {
                ..Default::default()
            },
            is_last: true,
            is_first: true,
            sge0: DescSge {
                addr: 0x1000,
                len: 512,
                key: Key::new(0x1234_u32),
            },
            sge1: None,
            sge2: None,
            sge3: None,
        }));
        // for _i in 0..4 {
        retry_map.add((Qpn::default(), Msn::default()), desc.clone(), true);

        // should send first retry
        std::thread::sleep(std::time::Duration::from_millis(1020));
        assert_eq!(device.0.lock().len(), 1);
        // should send second retry
        std::thread::sleep(std::time::Duration::from_millis(1020));
        assert_eq!(device.0.lock().len(), 2);
        // should send last retry
        std::thread::sleep(std::time::Duration::from_millis(1020));
        assert_eq!(device.0.lock().len(), 3);

        std::thread::sleep(std::time::Duration::from_millis(1020));
        // should remove the record
        matches!(
            map.read()
                .get(&(Qpn::default(), Msn::default()))
                .unwrap()
                .status(),
            CtxStatus::Failed(_)
        );
        device.0.lock().clear();
        std::thread::sleep(std::time::Duration::from_millis(1000));
    }
    // }
}
