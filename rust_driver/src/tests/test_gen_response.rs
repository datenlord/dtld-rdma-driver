use eui48::MacAddress;
use parking_lot::lock_api::{Mutex, RwLock};

use crate::{
    buf::{PacketBuf, RDMA_ACK_BUFFER_SLOT_SIZE},
    device::{ToHostWorkRbDescCommon, ToHostWorkRbDescRead},
    qp::QpContext,
    responser::{make_ack, make_nack, make_read_resp, ACKPACKET_SIZE},
    types::{Key, Msn, Pmtu, Psn, Qpn, WorkReqSendFlag},
};

const BUFFER_SIZE: usize = 1024 * RDMA_ACK_BUFFER_SLOT_SIZE;

#[test]
fn test_make_ack() {
    let buffer = Box::new([0u8; BUFFER_SIZE]);
    let buffer = Box::leak(buffer);
    let lkey = Key::new(0x1000);
    let ack_buffers: PacketBuf<RDMA_ACK_BUFFER_SLOT_SIZE> =
        PacketBuf::new(buffer.as_ptr() as usize, BUFFER_SIZE, lkey);
    let qp_table = std::sync::Arc::new(RwLock::new(std::collections::HashMap::new()));
    let qpn = Qpn::new(321);
    let msn = Msn::new(0x123);
    let psn = Psn::new(0x456);
    let local_mac = MacAddress::new([0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc]);
    let dqp_mac_addr = MacAddress::new([0x21, 0x43, 0x65, 0x87, 0x9a, 0xbc]);
    qp_table.write().insert(
        qpn,
        QpContext {
            pd: crate::Pd { handle: 1 },
            qpn,
            local_mac,
            dqp_mac_addr,
            ..Default::default()
        },
    );
    let ack_buf = ack_buffers.recycle_buf();
    let desc = make_ack(ack_buf, &qp_table, qpn, msn, psn).unwrap();
    // check the desc
    match *desc {
        crate::device::ToCardWorkRbDesc::WriteWithImm(desc) => {
            assert_eq!(desc.common.dqpn.get(), qpn.get());
            assert_eq!(desc.common.total_len, ACKPACKET_SIZE as u32);
            assert!(matches!(desc.common.pmtu, Pmtu::Mtu4096));
            assert_eq!(desc.common.flags.bits(), 0);
            assert!(matches!(
                desc.common.qp_type,
                crate::types::QpType::RawPacket
            ));
            assert_eq!(desc.sge0.len, ACKPACKET_SIZE as u32);
            assert_eq!(desc.sge0.key.get(), lkey.get());
        }
        crate::device::ToCardWorkRbDesc::Read(_)
        | crate::device::ToCardWorkRbDesc::Write(_)
        | crate::device::ToCardWorkRbDesc::ReadResp(_) => {
            panic!("Unexpected desc type");
        }
    }
    // check the content
    // the following context is checked manually
    let expected_buffer = [
        0x21, 0x43, 0x65, 0x87, 0x9a, 0xbc, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0x08, 0x00, 0x45,
        0x00, 0x00, 0x34, 0x27, 0x00, 0x00, 0x00, 0x40, 0x11, 0x55, 0xb7, 0x7f, 0x00, 0x00, 0x01,
        0x7f, 0x00, 0x00, 0x01, 0x12, 0xb7, 0x12, 0xb7, 0x00, 0x20, 0x00, 0x00, 0x11, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x01, 0x41, 0x00, 0x00, 0x04, 0x56, 0x00, 0x00, 0x01, 0x23, 0x00, 0x00,
        0x00, 0x00, 0xa6, 0x91, 0x8e, 0xe5,
    ];
    assert_eq!(&buffer[0..ACKPACKET_SIZE], &expected_buffer);
}

#[test]
fn test_make_nack() {
    let buffer = Box::new([0u8; BUFFER_SIZE]);
    let buffer = Box::leak(buffer);
    let lkey = Key::new(0x1000);
    let ack_buffers: PacketBuf<RDMA_ACK_BUFFER_SLOT_SIZE> =
        PacketBuf::new(buffer.as_ptr() as usize, BUFFER_SIZE, lkey);
    let qp_table = std::sync::Arc::new(RwLock::new(std::collections::HashMap::new()));
    let qpn = Qpn::new(321);
    let msn = Msn::new(0x123);
    let psn = Psn::new(0x456);
    let expeceted_psn = Psn::new(0x789);
    let local_mac = MacAddress::new([0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc]);
    let dqp_mac_addr = MacAddress::new([0x21, 0x43, 0x65, 0x87, 0x9a, 0xbc]);
    qp_table.write().insert(
        qpn,
        QpContext {
            pd: crate::Pd { handle: 1 },
            qpn,
            local_mac,
            dqp_mac_addr,
            ..Default::default()
        },
    );
    let ack_buf = ack_buffers.recycle_buf();
    let desc = make_nack(ack_buf, &qp_table, qpn, msn, psn, expeceted_psn).unwrap();
    // check the desc
    match *desc {
        crate::device::ToCardWorkRbDesc::WriteWithImm(desc) => {
            assert_eq!(desc.common.dqpn.get(), qpn.get());
            assert_eq!(desc.common.total_len, ACKPACKET_SIZE as u32);
            assert!(matches!(desc.common.pmtu, Pmtu::Mtu4096));
            assert_eq!(desc.common.flags.bits(), 0);
            assert!(matches!(
                desc.common.qp_type,
                crate::types::QpType::RawPacket
            ));
            assert_eq!(desc.sge0.len, ACKPACKET_SIZE as u32);
            assert_eq!(desc.sge0.key.get(), lkey.get());
        }
        crate::device::ToCardWorkRbDesc::Read(_)
        | crate::device::ToCardWorkRbDesc::Write(_)
        | crate::device::ToCardWorkRbDesc::ReadResp(_) => {
            panic!("Unexpected desc type");
        }
    }

    // check the content
    // the following context is checked manually
    let expected_buffer = [
        33, 67, 101, 135, 154, 188, 18, 52, 86, 120, 154, 188, 8, 0, 69, 0, 0, 52, 39, 0, 0, 0, 64,
        17, 85, 183, 127, 0, 0, 1, 127, 0, 0, 1, 18, 183, 18, 183, 0, 32, 0, 0, 17, 0, 0, 0, 0, 0,
        1, 65, 0, 0, 4, 86, 96, 0, 1, 35, 0, 7, 137, 0, 154, 4, 107, 218,
    ];
    assert_eq!(&buffer[0..ACKPACKET_SIZE], &expected_buffer);
}

#[test]
fn test_make_read_resp() {
    let qp_table = std::sync::Arc::new(RwLock::new(std::collections::HashMap::new()));
    let qpn = Qpn::new(321);
    let msn = Msn::new(0x123);
    let start_psn = Psn::new(0x1234);
    qp_table.write().insert(
        qpn,
        QpContext {
            pd: crate::Pd { handle: 1 },
            qpn,
            sending_psn: Mutex::new(start_psn),
            ..Default::default()
        },
    );
    let len = 4321;
    let laddr = 0x12345678;
    let lkey = Key::new(0x1000);
    let raddr = 0x87654321;
    let rkey = Key::new(0x2000);
    let read_req = ToHostWorkRbDescRead {
        common: ToHostWorkRbDescCommon {
            dqpn: qpn,
            msn,
            ..Default::default()
        },
        len,
        laddr,
        lkey,
        raddr,
        rkey,
    };
    let desc = make_read_resp(&qp_table, &read_req).unwrap();
    match *desc {
        crate::device::ToCardWorkRbDesc::ReadResp(desc) => {
            assert_eq!(desc.common.dqpn.get(), qpn.get());
            assert_eq!(desc.common.total_len, len);
            assert_eq!(desc.common.rkey, rkey);
            assert_eq!(desc.common.raddr, raddr);
            assert_eq!(desc.common.dqp_ip, std::net::Ipv4Addr::LOCALHOST);
            assert_eq!(desc.common.mac_addr, MacAddress::default());
            assert!(matches!(desc.common.pmtu, Pmtu::Mtu4096));
            assert_eq!(desc.common.flags, WorkReqSendFlag::empty());
            assert!(matches!(desc.common.qp_type, crate::types::QpType::Rc));
            assert_eq!(desc.common.psn, start_psn);
            assert_eq!(desc.sge0.len, len);
            assert_eq!(desc.sge0.key, lkey);
        }
        crate::device::ToCardWorkRbDesc::Read(_)
        | crate::device::ToCardWorkRbDesc::Write(_)
        | crate::device::ToCardWorkRbDesc::WriteWithImm(_) => {
            panic!("Unexpected desc type");
        }
    }
}
