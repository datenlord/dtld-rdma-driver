#[cfg(test)]
use std::{collections::HashMap, sync::Arc, thread::sleep};

use crate::{
    device::{
        DeviceError, ToHostRb, ToHostWorkRbDesc, ToHostWorkRbDescAck, ToHostWorkRbDescAethCode,
        ToHostWorkRbDescCommon, ToHostWorkRbDescRaw, ToHostWorkRbDescRead,
        ToHostWorkRbDescWriteOrReadResp, ToHostWorkRbDescWriteType,
    },
    qp::QpContext,
    types::{Key, Msn, Psn, Qpn}, work_poller::{WorkDescPoller, WorkDescPollerContext},
};


use parking_lot::{Mutex, RwLock};

struct MockToHostRb {
    rb: Mutex<Vec<ToHostWorkRbDesc>>,
}
impl MockToHostRb {
    fn new(v: Vec<ToHostWorkRbDesc>) -> Self {
        MockToHostRb { rb: Mutex::new(v) }
    }
}
impl ToHostRb<ToHostWorkRbDesc> for MockToHostRb {
    fn pop(&self) -> Result<ToHostWorkRbDesc, DeviceError> {
        let is_empty = self.rb.lock().is_empty();
        if is_empty {
            sleep(std::time::Duration::from_secs(1));
            return Ok(ToHostWorkRbDesc::Raw(ToHostWorkRbDescRaw::default()));
        }
        Ok(self.rb.lock().pop().unwrap())
    }
}
#[test]
fn test_work_desc_poller() {
    let mut input = vec![
        // test writeFirst
        ToHostWorkRbDesc::WriteOrReadResp(ToHostWorkRbDescWriteOrReadResp {
            common: ToHostWorkRbDescCommon {
                dqpn: Qpn::new(3),
                ..Default::default()
            },
            len: 4096,
            write_type: ToHostWorkRbDescWriteType::First,
            psn: Psn::new(0),
            ..Default::default()
        }),
        // test writeMiddle
        ToHostWorkRbDesc::WriteOrReadResp(ToHostWorkRbDescWriteOrReadResp {
            common: ToHostWorkRbDescCommon {
                dqpn: Qpn::new(3),
                expected_psn: Psn::new(1),
                ..Default::default()
            },
            addr: 1024,
            len: 1024,
            write_type: ToHostWorkRbDescWriteType::First,
            psn: Psn::new(1),
            ..Default::default()
        }),
        // test writeLast
        ToHostWorkRbDesc::WriteOrReadResp(ToHostWorkRbDescWriteOrReadResp {
            common: ToHostWorkRbDescCommon {
                dqpn: Qpn::new(3),
                expected_psn: Psn::new(2),
                ..Default::default()
            },
            addr: 1024,
            len: 1024,
            write_type: ToHostWorkRbDescWriteType::First,
            psn: Psn::new(2),
            ..Default::default()
        }),
        // test read
        ToHostWorkRbDesc::Read(ToHostWorkRbDescRead {
            common: ToHostWorkRbDescCommon {
                dqpn: Qpn::new(3),
                ..Default::default()
            },
            len: 2048,
            laddr: 0,
            lkey: Key::new(0),
            raddr: 0,
            rkey: Key::new(0),
        }),
        ToHostWorkRbDesc::Ack(ToHostWorkRbDescAck {
            common: ToHostWorkRbDescCommon {
                dqpn: Qpn::new(3),
                ..Default::default()
            },
            psn: Psn::new(2),
            code: ToHostWorkRbDescAethCode::Ack,
            ..Default::default()
        }),
        ToHostWorkRbDesc::Raw(ToHostWorkRbDescRaw {
            common: ToHostWorkRbDescCommon {
                dqpn: Qpn::new(3),
                ..Default::default()
            },
            addr: 0xa00_0000_0000,
            len: 4096,
            key: Default::default(),
        }),
    ];

    input.reverse();

    let work_rb = Arc::new(MockToHostRb::new(input));
    let qp_table = Arc::new(RwLock::new(HashMap::new()));
    qp_table.write().insert(
        Qpn::new(3),
        QpContext {
            qpn: Qpn::new(3),
            ..Default::default()
        },
    );
    let (checker_channel, checker_recv_queue) = flume::unbounded();
    let (notification_send_queue, _notification_recv_queue) = flume::unbounded();

    let work_ctx = WorkDescPollerContext {
        work_rb,
        checker_channel,
        nic_channel: notification_send_queue,
    };
    let _poller = WorkDescPoller::new(work_ctx,None);
    if let crate::checker::PacketCheckEvent::Write(w) = checker_recv_queue.recv().unwrap() {
        assert_eq!(w.psn.get(), 0);
    } else {
        panic!("not a write event");
    }
    if let crate::checker::PacketCheckEvent::Write(w) = checker_recv_queue.recv().unwrap() {
        assert_eq!(w.psn.get(), 1);
    } else {
        panic!("not a write event");
    }
    if let crate::checker::PacketCheckEvent::Write(w) = checker_recv_queue.recv().unwrap() {
        assert_eq!(w.psn.get(), 2);
    } else {
        panic!("not a write event");
    }
    if let crate::checker::PacketCheckEvent::ReadReq(w) = checker_recv_queue.recv().unwrap() {
        assert_eq!(w.len, 2048);
    } else {
        panic!("not a read event");
    }
    if let crate::checker::PacketCheckEvent::Ack(a) = checker_recv_queue.recv().unwrap() {
        assert_eq!(a.psn.get(), 2);
    } else {
        panic!("not a ack event");
    }
}
