use std::{collections::LinkedList, error::Error, sync::Arc};

use parking_lot::Mutex;

use crate::types::Qpn;

use super::{SchedulerStrategy, SealedDesc, POP_BATCH_SIZE};

/// The round-robin strategy for the scheduler.
#[allow(clippy::module_name_repetitions, clippy::linkedlist)]
#[derive(Debug, Clone)]
pub struct RoundRobinStrategy(Arc<Mutex<RoundRobinStrategyInner>>);

#[derive(Debug)]
struct RoundRobinStrategyInner {
    queue: LinkedList<(u32, LinkedList<SealedDesc>)>,
}

impl RoundRobinStrategy {
    /// Create a new round-robin strategy.
    pub fn new() -> Self {
        Self(
            Mutex::new(RoundRobinStrategyInner {
                queue: LinkedList::new(),
            })
            .into(),
        )
    }
}

impl Default for RoundRobinStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl SchedulerStrategy for RoundRobinStrategy {
    fn push<I>(&self, qpn: Qpn, desc: I) -> Result<(), Box<dyn Error>>
    where
        I: Iterator<Item = SealedDesc>,
    {
        let guard = &mut self.0.lock().queue;
        for i in guard.iter_mut() {
            // merge the descriptor if the qpn is already in the queue
            if i.0 == qpn.get() {
                i.1.extend(desc);
                return Ok(());
            }
        }

        guard.push_back((qpn.get(), desc.collect()));
        Ok(())
    }

    #[allow(
        clippy::unwrap_in_result,
        clippy::unwrap_used,
        clippy::arithmetic_side_effects,
        clippy::indexing_slicing
    )]
    fn pop_batch(&self) -> Result<([Option<SealedDesc>; POP_BATCH_SIZE], u32), Box<dyn Error>> {
        const ARRAY_REPEAT_VALUE: Option<SealedDesc> = None;
        let mut result = [ARRAY_REPEAT_VALUE; POP_BATCH_SIZE];
        let mut counter: u32 = 0;
        let guard = &mut self.0.lock().queue;

        while !guard.is_empty() {
            if let Some((_, list)) = guard.front_mut() {
                // the front_mut is existed,so the pop_front will not return None
                result[counter as usize] = Some(list.pop_front().unwrap()); // counter is always less than POP_BATCH_SIZE
                counter += 1;
            }

            // the front_mut is existed,so the pop_front will not return None
            let (qpn, list) = guard.pop_front().unwrap();
            if !list.is_empty() {
                guard.push_back((qpn, list));
            }
            if counter as usize == POP_BATCH_SIZE {
                return Ok((result, counter));
            }
        }
        Ok((result, counter))
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::LinkedList, net::Ipv4Addr};

    use eui48::MacAddress;

    use crate::{
        device::{
            scheduler::{round_robin::RoundRobinStrategy, SchedulerStrategy},
            DescSge, ToCardWorkRbDesc, ToCardWorkRbDescCommon, ToCardWorkRbDescWrite,
        },
        types::{Key, Msn, Pmtu, Psn, QpType, Qpn, WorkReqSendFlag},
        SealedDesc,
    };

    pub(crate) fn generate_random_descriptors(qpn: u32, num: usize) -> LinkedList<SealedDesc> {
        let desc = Box::new(ToCardWorkRbDesc::Write(ToCardWorkRbDescWrite {
            common: ToCardWorkRbDescCommon {
                total_len: 512,
                raddr: 0x0,
                rkey: Key::new(1234_u32),
                dqp_ip: Ipv4Addr::new(127, 0, 0, 1),
                dqpn: Qpn::new(qpn),
                mac_addr: MacAddress::default(),
                pmtu: Pmtu::Mtu1024,
                flags: WorkReqSendFlag::empty(),
                qp_type: QpType::Rc,
                psn: Psn::new(1234),
                msn: Msn::new(0),
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
        let mut ret = LinkedList::new();
        for _ in 0..num {
            ret.push_back(SealedDesc::from(desc.clone()));
        }
        ret
    }

    #[test]
    fn test_round_robin() {
        let round_robin = RoundRobinStrategy::new();
        let qpn1 = Qpn::new(1);
        let qpn2 = Qpn::new(2);
        let qpn1_descs = generate_random_descriptors(1, 2).into_iter();
        round_robin.push(qpn1, qpn1_descs).unwrap();
        let qpn2_descs = generate_random_descriptors(2, 3).into_iter();
        round_robin.push(qpn2, qpn2_descs).unwrap();
        let (desc, n) = round_robin.pop_batch().unwrap();
        assert_eq!(n, 5);
        let descs = desc
            .into_iter()
            .flatten()
            .map(|s| s.get_dqpn().get())
            .collect::<Vec<u32>>();
        let result_dqpns = vec![1, 2, 1, 2, 2];
        assert_eq!(descs, result_dqpns);

        // test merge descriptors
        let (_desc, n) = round_robin.pop_batch().unwrap();
        assert_eq!(n, 0);
        let qpn1_descs = generate_random_descriptors(1, 9).into_iter();
        round_robin.push(qpn1, qpn1_descs).unwrap();
        let qpn2_descs = generate_random_descriptors(2, 1).into_iter();
        round_robin.push(qpn2, qpn2_descs).unwrap();
        let (desc, _n) = round_robin.pop_batch().unwrap();
        let descs = desc
            .into_iter()
            .flatten()
            .map(|s| s.get_dqpn().get())
            .collect::<Vec<u32>>();
        let result_dqpns = vec![1, 2, 1, 1, 1, 1, 1, 1];
        assert_eq!(descs, result_dqpns);
    }
}
