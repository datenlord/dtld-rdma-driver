use std::mem::size_of;

use crate::device::software::packet::Immediate;
use crate::device::software::packet::AETH;
use crate::device::software::packet::BTH;
use crate::device::software::packet::RETH;
use crate::device::software::packet_processor::PacketProcessor;
use crate::device::software::types::Key;
use crate::device::software::types::Metadata;
use crate::device::software::types::PKey;
use crate::device::software::types::PayloadInfo;
use crate::device::software::types::Qpn;
use crate::device::software::types::RdmaGeneralMeta;
use crate::device::software::types::RdmaMessage;
use crate::device::software::types::RdmaMessageMetaCommon;
use crate::device::software::types::RethHeader;
use crate::device::ToHostWorkRbDescOpcode;
use crate::device::ToHostWorkRbDescTransType;
use crate::types::Psn;

const BTH_SIZE: usize = size_of::<BTH>();
const RETH_SIZE: usize = size_of::<RETH>();
const AETH_SIZE: usize = size_of::<AETH>();
const IMM_SIZE: usize = size_of::<Immediate>();

#[test]
fn test_header_bth_reth() {
    let buf = [0u8; BTH_SIZE + RETH_SIZE + 512];
    let bth = BTH::from_bytes(&buf);
    bth.set_opcode_and_type(
        ToHostWorkRbDescOpcode::RdmaWriteFirst,
        crate::device::ToHostWorkRbDescTransType::Rc,
    );
    bth.set_destination_qpn(1);
    bth.set_psn(1);
    bth.set_ack_req(false);
    bth.set_flags_solicited(true);
    bth.set_pkey(0x1234);
    let reth = RETH::from_bytes(&buf[BTH_SIZE..]);
    reth.set_va(1);
    reth.set_rkey(0x12345678);
    reth.set_dlen(1);

    let message = PacketProcessor::to_rdma_message(&buf).unwrap();
    let meta = &message.meta_data;
    match meta {
        Metadata::General(header) => {
            assert_eq!(
                header.common_meta.tran_type as u8,
                crate::device::ToHostWorkRbDescTransType::Rc as u8
            );
            assert_eq!(
                header.common_meta.opcode.clone() as u8,
                ToHostWorkRbDescOpcode::RdmaWriteFirst as u8
            );
            assert!(header.common_meta.solicited);
            assert_eq!(header.common_meta.dqpn.get(), 1);
            assert!(!header.common_meta.ack_req);
            assert_eq!(header.common_meta.psn.get(), 1);
            assert_eq!(header.common_meta.pkey.get(), 0x1234);
            assert_eq!(header.reth.va, 1);
            assert_eq!(header.reth.rkey.get(), 0x12345678);
            assert_eq!(header.reth.len, 1);
            assert_eq!(message.payload.get_length(), 512);
        }
        _ => panic!("wrong meta data"),
    }
    let mut new_buf = [0u8; BTH_SIZE + RETH_SIZE + 512];
    let size = PacketProcessor::set_from_rdma_message(&mut new_buf, &message).unwrap();
    assert!(size == BTH_SIZE + RETH_SIZE);
    assert!(buf[..size] == new_buf[..size]);
}

#[test]
fn test_header_bth_reth_imm() {
    let mut buf = [0u8; BTH_SIZE + RETH_SIZE + IMM_SIZE + 512];
    let bth = BTH::from_bytes(&buf);
    bth.set_opcode_and_type(
        ToHostWorkRbDescOpcode::RdmaWriteLastWithImmediate,
        crate::device::ToHostWorkRbDescTransType::Rc,
    );
    bth.set_destination_qpn(1);
    bth.set_psn(1);
    bth.set_ack_req(false);
    bth.set_flags_solicited(true);
    bth.set_pkey(0x1234);
    let reth = RETH::from_bytes(&buf[BTH_SIZE..]);
    reth.set_va(0x1234567812345678);
    reth.set_rkey(0x12345678);
    reth.set_dlen(0x12345678);
    let imm = &mut buf[BTH_SIZE + RETH_SIZE..BTH_SIZE + RETH_SIZE + IMM_SIZE];
    imm.copy_from_slice(&[1u8; IMM_SIZE]);
    let message = PacketProcessor::to_rdma_message(&buf).unwrap();
    let meta = &message.meta_data;
    match meta {
        Metadata::General(header) => {
            assert_eq!(
                header.common_meta.tran_type as u8,
                crate::device::ToHostWorkRbDescTransType::Rc as u8
            );
            assert_eq!(
                header.common_meta.opcode.clone() as u8,
                ToHostWorkRbDescOpcode::RdmaWriteLastWithImmediate as u8
            );
            assert!(header.common_meta.solicited);
            assert_eq!(header.common_meta.dqpn.get(), 1);
            assert!(!header.common_meta.ack_req);
            assert_eq!(header.common_meta.psn.get(), 1);
            assert_eq!(header.common_meta.pkey.get(), 0x1234);
            assert_eq!(header.reth.va, 0x1234567812345678);
            assert_eq!(header.reth.rkey.get(), 0x12345678);
            assert_eq!(header.reth.len, 0x12345678);
            assert_eq!(message.payload.get_length(), 512);
            assert_eq!(header.imm.unwrap(), u32::from_le_bytes([1u8; IMM_SIZE]));
        }
        _ => panic!("wrong meta data"),
    }
    let mut new_buf = [0u8; BTH_SIZE + RETH_SIZE + IMM_SIZE + 512];
    let size = PacketProcessor::set_from_rdma_message(&mut new_buf, &message).unwrap();
    assert!(size == BTH_SIZE + RETH_SIZE + IMM_SIZE);
    assert!(buf[..size] == new_buf[..size]);
}

#[test]
fn test_header_bth_reth_reth() {
    let buf = [0u8; BTH_SIZE + RETH_SIZE + RETH_SIZE];
    let bth = BTH::from_bytes(&buf);
    bth.set_opcode_and_type(
        ToHostWorkRbDescOpcode::RdmaReadRequest,
        crate::device::ToHostWorkRbDescTransType::Rc,
    );
    bth.set_destination_qpn(1);
    bth.set_psn(1);
    bth.set_ack_req(false);
    bth.set_flags_solicited(true);
    bth.set_pkey(0x1234);
    let reth = RETH::from_bytes(&buf[BTH_SIZE..]);
    reth.set_va(0x1234567812345678);
    reth.set_rkey(0x12345678_u32);
    reth.set_dlen(0x12345678);
    let reth = RETH::from_bytes(&buf[BTH_SIZE + RETH_SIZE..]);
    reth.set_va(0x1234567812345678);
    reth.set_rkey(0x12345678_u32);
    reth.set_dlen(0x12345678);
    let message = PacketProcessor::to_rdma_message(&buf).unwrap();
    let meta = &message.meta_data;
    match meta {
        Metadata::General(header) => {
            assert_eq!(
                header.common_meta.tran_type as u8,
                crate::device::ToHostWorkRbDescTransType::Rc as u8
            );
            assert_eq!(
                header.common_meta.opcode.clone() as u8,
                ToHostWorkRbDescOpcode::RdmaReadRequest as u8
            );
            assert!(header.common_meta.solicited);
            assert_eq!(header.common_meta.dqpn.get(), 1);
            assert!(!header.common_meta.ack_req);
            assert_eq!(header.common_meta.psn.get(), 1);
            assert_eq!(header.common_meta.pkey.get(), 0x1234);
            assert_eq!(header.reth.va, 0x1234567812345678);
            assert_eq!(header.reth.rkey.get(), 0x12345678);
            assert_eq!(header.reth.len, 0x12345678);
            assert_eq!(message.payload.get_length(), 0);
            let secondary_reth = header.secondary_reth.as_ref().unwrap();
            assert_eq!(secondary_reth.va, 0x1234567812345678);
            assert_eq!(secondary_reth.rkey.get(), 0x12345678);
            assert_eq!(secondary_reth.len, 0x12345678);
        }
        _ => panic!("wrong meta data"),
    }
    let mut new_buf = [0u8; BTH_SIZE + RETH_SIZE + RETH_SIZE + 512];
    let size = PacketProcessor::set_from_rdma_message(&mut new_buf, &message).unwrap();
    assert!(size == BTH_SIZE + RETH_SIZE + RETH_SIZE);
    assert!(buf[..size] == new_buf[..size]);
}

#[test]
fn test_header_bth_aeth() {
    let buf = [0u8; BTH_SIZE + AETH_SIZE];
    let bth = BTH::from_bytes(&buf);
    bth.set_opcode_and_type(
        ToHostWorkRbDescOpcode::Acknowledge,
        crate::device::ToHostWorkRbDescTransType::Rc,
    );
    bth.set_destination_qpn(1);
    bth.set_psn(1);
    bth.set_ack_req(false);
    bth.set_flags_solicited(true);
    bth.set_pkey(1);
    let aeth = AETH::from_bytes(&buf[BTH_SIZE..]);
    aeth.set_aeth_code_and_value(2, 5);
    aeth.set_msn(0x123456);
    let message = PacketProcessor::to_rdma_message(&buf).unwrap();
    let meta = &message.meta_data;
    match meta {
        Metadata::Acknowledge(header) => {
            assert_eq!(
                header.common_meta.tran_type as u8,
                crate::device::ToHostWorkRbDescTransType::Rc as u8
            );
            assert_eq!(
                header.common_meta.opcode.clone() as u8,
                ToHostWorkRbDescOpcode::Acknowledge as u8
            );
            assert!(header.common_meta.solicited);
            assert_eq!(header.common_meta.dqpn.get(), 1);
            assert!(!header.common_meta.ack_req);
            assert_eq!(header.common_meta.psn.get(), 1);
            assert_eq!(header.msn, 0x123456);
            assert_eq!(header.aeth_code.clone() as u8, 2);
            assert_eq!(header.aeth_value, 5);
        }
        _ => panic!("wrong meta data"),
    }
    let mut new_buf = [0u8; BTH_SIZE + AETH_SIZE];
    let size = PacketProcessor::set_from_rdma_message(&mut new_buf, &message).unwrap();
    assert!(size == BTH_SIZE + AETH_SIZE);
    assert!(buf[..size] == new_buf[..size]);
}

#[test]
fn test_payload_copy_to() {
    // test one source
    {
        let mut payload = PayloadInfo::new();
        let mut src_buf = [1u8; 512];
        let mut dest_buf = [0u8; 512];
        payload.add(src_buf.as_mut_ptr(), 512);
        payload.copy_to(dest_buf.as_mut_ptr());
        assert_eq!(src_buf, dest_buf);
    }

    // test multiple source
    {
        let mut payload = PayloadInfo::new();
        let mut src_buf1 = [1u8; 128];
        let mut src_buf2 = [2u8; 128];
        let mut src_buf3 = [3u8; 128];
        let mut src_buf4 = [4u8; 128];
        let mut dest_buf = [0u8; 512];
        payload.add(src_buf1.as_mut_ptr(), src_buf1.len());
        payload.add(src_buf2.as_mut_ptr(), src_buf2.len());
        payload.add(src_buf3.as_mut_ptr(), src_buf3.len());
        payload.add(src_buf4.as_mut_ptr(), src_buf4.len());

        payload.copy_to(dest_buf.as_mut_ptr());
        assert_eq!(src_buf1, dest_buf[..128]);
        assert_eq!(src_buf2, dest_buf[128..256]);
        assert_eq!(src_buf3, dest_buf[256..384]);
        assert_eq!(src_buf4, dest_buf[384..512]);
    }
}

#[test]
fn test_pkt_processor_to_buf() {
    let mut payload = PayloadInfo::new();
    const DATA_SIZE: usize = 512;
    let data_buf = [1u8; DATA_SIZE];
    payload.add(data_buf.as_ptr(), data_buf.len());
    let msg = RdmaMessage {
        meta_data: Metadata::General(RdmaGeneralMeta {
            common_meta: RdmaMessageMetaCommon {
                tran_type: ToHostWorkRbDescTransType::Rc,
                opcode: ToHostWorkRbDescOpcode::RdmaWriteFirst,
                solicited: false,
                pkey: PKey::new(0),
                dqpn: Qpn::new(3),
                ack_req: false,
                psn: Psn::new(0x123456),
            },
            reth: RethHeader {
                va: 0x1234567812345678,
                rkey: Key::new(0x12345678),
                len: 0x12345678,
            },
            imm: None,
            secondary_reth: None,
        }),
        payload,
    };
    let mut buf = [0u8; 4096];
    let size = PacketProcessor::set_from_rdma_message(&mut buf, &msg).unwrap();
    assert!(size == BTH_SIZE + RETH_SIZE);
    // read bth
    let bth = BTH::from_bytes(&buf);
    assert_eq!(
        bth.get_opcode(),
        ToHostWorkRbDescOpcode::RdmaWriteFirst as u8
    );
    assert_eq!(bth.get_destination_qpn(), 3);
    assert_eq!(bth.get_psn(), 0x123456);
    assert!(!bth.get_ack_req());
    assert_eq!(bth.get_pkey(), 0);
    // read reth
    let reth = RETH::from_bytes(&buf[BTH_SIZE..]);
    assert_eq!(reth.get_va(), 0x1234567812345678);
    assert_eq!(reth.get_rkey(), 0x12345678);
    assert_eq!(reth.get_dlen(), 0x12345678);
}
