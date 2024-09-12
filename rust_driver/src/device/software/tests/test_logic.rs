use std::{cell::RefCell, collections::LinkedList, net::Ipv4Addr, sync::Arc};

use flume::unbounded;

use crate::{
    device::{
        software::{
            logic::BlueRDMALogic,
            net_agent::{NetAgentError, NetSendAgent},
            tests::{SGListBuilder, ToCardWorkRbDescBuilder},
            types::{Metadata, PayloadInfo, RdmaMessage},
        },
        ToCardWorkRbDescOpcode, ToHostWorkRbDescOpcode,
    },
    types::{Pmtu, QpType},
};

#[derive(Debug)]
struct DummpyProxy {
    message: RefCell<LinkedList<RdmaMessage>>,
    payload: RefCell<LinkedList<PayloadInfo>>,
}

impl DummpyProxy {
    fn new() -> Self {
        DummpyProxy {
            message: LinkedList::new().into(),
            payload: LinkedList::new().into(),
        }
    }
}
impl NetSendAgent for DummpyProxy {
    fn send(&self, _: Ipv4Addr, _: u16, message: &RdmaMessage) -> Result<(), NetAgentError> {
        self.message.borrow_mut().push_back(message.clone());
        Ok(())
    }

    fn send_raw(&self, _: Ipv4Addr, _: u16, payload: &PayloadInfo) -> Result<(), NetAgentError> {
        self.payload.borrow_mut().push_back(payload.clone());
        Ok(())
    }
}

unsafe impl Send for DummpyProxy {}
unsafe impl Sync for DummpyProxy {}

#[test]
fn test_logic_send() {
    let agent = Arc::new(DummpyProxy::new());
    let (ctrl_sender, _ctrl_receiver) = unbounded();
    let (work_sender, _work_receiver) = unbounded();
    let logic = BlueRDMALogic::new(Arc::<DummpyProxy>::clone(&agent), ctrl_sender, work_sender);

    // expect write_only
    {
        let desc = ToCardWorkRbDescBuilder::default()
            .with_opcode(ToCardWorkRbDescOpcode::Write)
            .with_total_len(512)
            .with_raddr(0)
            .with_rkey(1234)
            .with_pmtu(Pmtu::Mtu1024)
            .with_psn(1234)
            .with_dqpn(12)
            .with_sg_list(
                SGListBuilder::new()
                    .with_sge(0x1000, 512, 0x1234_u32)
                    .build(),
            )
            .build();

        logic.send(desc).unwrap();
        assert_eq!(agent.message.borrow().len(), 1);
        let message = agent.message.borrow_mut().pop_front().unwrap();
        assert_eq!(
            message.meta_data.get_opcode(),
            ToHostWorkRbDescOpcode::RdmaWriteOnly
        );
        assert_eq!(message.payload.get_length(), 512);
        assert_eq!(message.payload.get_sg_list()[0].data as u64, 0x1000);
    }

    // va=512,length = 1024, expect write_first + write_last
    {
        let desc = ToCardWorkRbDescBuilder::default()
            .with_opcode(ToCardWorkRbDescOpcode::Write)
            .with_total_len(1024)
            .with_raddr(512)
            .with_rkey(1234)
            .with_pmtu(Pmtu::Mtu1024)
            .with_psn(1234)
            .with_dqpn(12)
            .with_sg_list(
                SGListBuilder::new()
                    .with_sge(0x1000, 256, 0x1234_u32)
                    .with_sge(0x2000, 768, 0x1234_u32)
                    .build(),
            )
            .build();
        logic.send(desc).unwrap();
        assert_eq!(agent.message.borrow().len(), 2);
        let message1 = agent.message.borrow_mut().pop_front().unwrap();
        assert_eq!(
            message1.meta_data.get_opcode(),
            ToHostWorkRbDescOpcode::RdmaWriteFirst
        );
        assert_eq!(message1.payload.get_length(), 512);
        let message2 = agent.message.borrow_mut().pop_front().unwrap();
        assert_eq!(
            message2.meta_data.get_opcode(),
            ToHostWorkRbDescOpcode::RdmaWriteLast
        );
        assert_eq!(message2.payload.get_length(), 512);
    }

    // va=1023,length = 4096, expect write_first + write_middle + write_middle + write_middle + write_last
    {
        let desc = ToCardWorkRbDescBuilder::default()
            .with_opcode(ToCardWorkRbDescOpcode::Write)
            .with_total_len(4096)
            .with_raddr(1023)
            .with_rkey(1234)
            .with_pmtu(Pmtu::Mtu1024)
            .with_psn(1234)
            .with_dqpn(12)
            .with_sg_list(
                SGListBuilder::new()
                    .with_sge(0x1000, 4096, 0x1234_u32)
                    .build(),
            )
            .build();
        logic.send(desc).unwrap();
        assert_eq!(agent.message.borrow().len(), 5);
        let message_first = agent.message.borrow_mut().pop_front().unwrap();
        assert_eq!(
            message_first.meta_data.get_opcode(),
            ToHostWorkRbDescOpcode::RdmaWriteFirst
        );
        assert_eq!(message_first.payload.get_length(), 1);
        let message_middle = agent.message.borrow_mut().pop_front().unwrap();
        assert_eq!(
            message_middle.meta_data.get_opcode(),
            ToHostWorkRbDescOpcode::RdmaWriteMiddle
        );
        assert_eq!(message_middle.payload.get_length(), 1024);
        let message_middle = agent.message.borrow_mut().pop_front().unwrap();
        assert_eq!(
            message_middle.meta_data.get_opcode(),
            ToHostWorkRbDescOpcode::RdmaWriteMiddle
        );
        assert_eq!(message_middle.payload.get_length(), 1024);
        let message_middle = agent.message.borrow_mut().pop_front().unwrap();
        assert_eq!(
            message_middle.meta_data.get_opcode(),
            ToHostWorkRbDescOpcode::RdmaWriteMiddle
        );
        assert_eq!(message_middle.payload.get_length(), 1024);
        let message_last = agent.message.borrow_mut().pop_front().unwrap();
        assert_eq!(
            message_last.meta_data.get_opcode(),
            ToHostWorkRbDescOpcode::RdmaWriteLast
        );
        assert_eq!(message_last.payload.get_length(), 1023);
    }

    // read, va=1023,length = 4096, expect write_first + write_middle + write_middle + write_middle + write_last
    {
        let desc = ToCardWorkRbDescBuilder::default()
            .with_opcode(ToCardWorkRbDescOpcode::ReadResp)
            .with_total_len(4096)
            .with_raddr(1023)
            .with_rkey(1234)
            .with_pmtu(Pmtu::Mtu1024)
            .with_psn(1234)
            .with_dqpn(12)
            .with_sg_list(
                SGListBuilder::new()
                    .with_sge(0x1000, 4096, 0x1234_u32)
                    .build(),
            )
            .build();
        logic.send(desc).unwrap();
        assert_eq!(agent.message.borrow().len(), 5);
        let message_first = agent.message.borrow_mut().pop_front().unwrap();
        assert_eq!(
            message_first.meta_data.get_opcode(),
            ToHostWorkRbDescOpcode::RdmaReadResponseFirst
        );
        assert_eq!(message_first.payload.get_length(), 1);
        let psn1 = message_first.meta_data.common_meta().psn;
        let message_middle = agent.message.borrow_mut().pop_front().unwrap();
        assert_eq!(
            message_middle.meta_data.get_opcode(),
            ToHostWorkRbDescOpcode::RdmaReadResponseMiddle
        );
        assert_eq!(message_middle.payload.get_length(), 1024);
        let psn2 = message_middle.meta_data.common_meta().psn;
        assert!(psn2 == psn1.wrapping_add(1));
        let message_middle = agent.message.borrow_mut().pop_front().unwrap();
        assert_eq!(
            message_middle.meta_data.get_opcode(),
            ToHostWorkRbDescOpcode::RdmaReadResponseMiddle
        );
        assert_eq!(message_middle.payload.get_length(), 1024);
        let psn3 = message_middle.meta_data.common_meta().psn;
        assert!(psn3 == psn2.wrapping_add(1));
        let message_middle = agent.message.borrow_mut().pop_front().unwrap();
        assert_eq!(
            message_middle.meta_data.get_opcode(),
            ToHostWorkRbDescOpcode::RdmaReadResponseMiddle
        );
        assert_eq!(message_middle.payload.get_length(), 1024);
        let psn4 = message_middle.meta_data.common_meta().psn;
        assert!(psn4 == psn3.wrapping_add(1));
        let message_last = agent.message.borrow_mut().pop_front().unwrap();
        assert_eq!(
            message_last.meta_data.get_opcode(),
            ToHostWorkRbDescOpcode::RdmaReadResponseLast
        );
        assert_eq!(message_last.payload.get_length(), 1023);
    }

    // test write only with imm
    {
        let desc = ToCardWorkRbDescBuilder::default()
            .with_opcode(ToCardWorkRbDescOpcode::WriteWithImm)
            .with_total_len(20)
            .with_raddr(0)
            .with_rkey(1234)
            .with_pmtu(Pmtu::Mtu1024)
            .with_psn(1234)
            .with_imm(0x1234)
            .with_dqpn(12)
            .with_sg_list(
                SGListBuilder::new()
                    .with_sge(0x1000, 20, 0x1234_u32)
                    .build(),
            )
            .build();
        logic.send(desc).unwrap();
        assert_eq!(agent.message.borrow().len(), 1);
        let message = agent.message.borrow_mut().pop_front().unwrap();
        assert_eq!(
            message.meta_data.get_opcode(),
            ToHostWorkRbDescOpcode::RdmaWriteOnlyWithImmediate
        );
        match message.meta_data {
            Metadata::General(meta) => {
                assert_eq!(meta.imm.unwrap(), 0x1234);
            }
            _ => unreachable!(),
        }
    }

    // test read request
    {
        let desc = ToCardWorkRbDescBuilder::default()
            .with_opcode(ToCardWorkRbDescOpcode::Read)
            .with_total_len(1024)
            .with_raddr(0)
            .with_rkey(1234)
            .with_pmtu(Pmtu::Mtu1024)
            .with_psn(1234)
            .with_dqpn(12)
            .with_sg_list(
                SGListBuilder::new()
                    .with_sge(0x1000, 1024, 4567_u32)
                    .build(),
            )
            .build();
        logic.send(desc).unwrap();
        assert_eq!(agent.message.borrow().len(), 1);
        let message = agent.message.borrow_mut().pop_front().unwrap();
        assert_eq!(
            message.meta_data.get_opcode(),
            ToHostWorkRbDescOpcode::RdmaReadRequest
        );
        match message.meta_data {
            Metadata::General(meta) => {
                assert_eq!(meta.reth.va, 0);
                assert_eq!(meta.reth.len, 1024);
                assert_eq!(meta.reth.rkey.get(), 1234);
                let secondary_reth = meta.secondary_reth.as_ref().unwrap();
                assert_eq!(secondary_reth.va, 0x1000);
                assert_eq!(secondary_reth.len, 1024);
                assert_eq!(secondary_reth.rkey.get(), 4567);
            }
            _ => unreachable!(),
        }
    }

    // test large packet (64k)
    {
        let desc1 = ToCardWorkRbDescBuilder::default()
            .with_opcode(ToCardWorkRbDescOpcode::Write)
            .with_total_len(1024 * 64)
            .with_raddr(0)
            .with_rkey(1234)
            .with_pmtu(Pmtu::Mtu4096)
            .with_psn(0)
            .with_dqpn(12)
            .with_is_last(false)
            .with_sg_list(
                SGListBuilder::new()
                    .with_sge(0, 1024 * 32, 0x1234_u32)
                    .build(),
            )
            .build();
        let desc2 = ToCardWorkRbDescBuilder::default()
            .with_opcode(ToCardWorkRbDescOpcode::Write)
            .with_total_len(1024 * 32)
            .with_raddr(1024 * 32)
            .with_rkey(1234)
            .with_pmtu(Pmtu::Mtu4096)
            .with_psn(8)
            .with_dqpn(12)
            .with_is_first(false)
            .with_sg_list(
                SGListBuilder::new()
                    .with_sge(0, 1024 * 32, 0x1234_u32)
                    .build(),
            )
            .build();
        logic.send(desc1).unwrap();
        logic.send(desc2).unwrap();
        assert_eq!(agent.message.borrow().len(), 16);

        let message = agent.message.borrow_mut().pop_front().unwrap();
        assert_eq!(
            message.meta_data.get_opcode(),
            ToHostWorkRbDescOpcode::RdmaWriteFirst
        );
        assert_eq!(message.payload.get_length(), 4096);
        let meta = match message.meta_data {
            Metadata::General(meta) => meta,
            _ => unreachable!(),
        };
        assert_eq!(meta.common_meta.psn.get(), 0,);
        assert_eq!(meta.reth.va, 0);
        assert_eq!(meta.reth.len, 1024 * 64);
        assert_eq!(meta.reth.rkey.get(), 1234);
        for i in 1..15 {
            let message = agent.message.borrow_mut().pop_front().unwrap();
            assert_eq!(
                message.meta_data.get_opcode(),
                ToHostWorkRbDescOpcode::RdmaWriteMiddle
            );
            assert_eq!(message.meta_data.common_meta().psn.get(), i,);
            assert_eq!(message.payload.get_length(), 4096);
        }

        let message = agent.message.borrow_mut().pop_front().unwrap();
        assert_eq!(
            message.meta_data.get_opcode(),
            ToHostWorkRbDescOpcode::RdmaWriteLast
        );
        assert_eq!(message.meta_data.common_meta().psn.get(), 15,);
        assert_eq!(message.payload.get_length(), 1024 * 4);
    }
    // test first packet less than pmtu and total is larger than 32k
    {
        let desc1 = ToCardWorkRbDescBuilder::default()
            .with_opcode(ToCardWorkRbDescOpcode::Write)
            .with_total_len(1024 * 33)
            .with_raddr(1024 * 31)
            .with_rkey(1234)
            .with_pmtu(Pmtu::Mtu4096)
            .with_psn(0)
            .with_dqpn(12)
            .with_is_last(false)
            .with_sg_list(SGListBuilder::new().with_sge(0, 1024, 0x1234_u32).build())
            .build();
        let desc2 = ToCardWorkRbDescBuilder::default()
            .with_opcode(ToCardWorkRbDescOpcode::Write)
            .with_total_len(1024 * 32)
            .with_raddr(1024 * 32)
            .with_rkey(1234)
            .with_pmtu(Pmtu::Mtu4096)
            .with_psn(1)
            .with_dqpn(12)
            .with_is_first(false)
            .with_sg_list(
                SGListBuilder::new()
                    .with_sge(1024 * 32, 1024 * 32, 0x1234_u32)
                    .build(),
            )
            .build();
        logic.send(desc1).unwrap();
        logic.send(desc2).unwrap();
        assert_eq!(agent.message.borrow().len(), 9);
        let message = agent.message.borrow_mut().pop_front().unwrap();
        assert_eq!(
            message.meta_data.get_opcode(),
            ToHostWorkRbDescOpcode::RdmaWriteFirst
        );
        assert_eq!(message.meta_data.common_meta().psn.get(), 0,);
        assert_eq!(message.payload.get_length(), 1024);
        let meta = match message.meta_data {
            Metadata::General(meta) => meta,
            _ => unreachable!(),
        };
        assert_eq!(meta.reth.va, 1024 * 31);
        assert_eq!(meta.reth.len, 1024 * 33);
        assert_eq!(meta.reth.rkey.get(), 1234);
        for i in 1..8 {
            let message = agent.message.borrow_mut().pop_front().unwrap();
            assert_eq!(
                message.meta_data.get_opcode(),
                ToHostWorkRbDescOpcode::RdmaWriteMiddle
            );
            assert_eq!(message.meta_data.common_meta().psn.get(), i,);
            assert_eq!(message.payload.get_length(), 4096);
        }

        let message = agent.message.borrow_mut().pop_front().unwrap();
        assert_eq!(
            message.meta_data.get_opcode(),
            ToHostWorkRbDescOpcode::RdmaWriteLast
        );
        assert_eq!(message.meta_data.common_meta().psn.get(), 8);
        assert_eq!(message.payload.get_length(), 1024 * 4);
    }
}

#[test]
fn test_logic_send_raw() {
    let agent = Arc::new(DummpyProxy::new());
    let (ctrl_sender, _ctrl_receiver) = unbounded();
    let (work_sender, _work_receiver) = unbounded();
    let logic = BlueRDMALogic::new(Arc::<DummpyProxy>::clone(&agent), ctrl_sender, work_sender);
    {
        let desc = ToCardWorkRbDescBuilder::default()
            .with_qp_type(QpType::RawPacket)
            .with_opcode(ToCardWorkRbDescOpcode::WriteWithImm)
            .with_total_len(4096)
            .with_raddr(0)
            .with_rkey(1234)
            .with_pmtu(Pmtu::Mtu4096)
            .with_psn(1234)
            .with_dqpn(12)
            .with_sg_list(
                SGListBuilder::new()
                    .with_sge(1000, 1024, 4567_u32)
                    .with_sge(3000, 1024, 4567_u32)
                    .with_sge(5000, 1024, 4567_u32)
                    .with_sge(7000, 1024, 4567_u32)
                    .build(),
            )
            .build();
        logic.send(desc).unwrap();
        assert_eq!(agent.payload.borrow().len(), 1);
        let payload = agent.payload.borrow_mut().pop_front().unwrap();
        assert_eq!(payload.get_length(), 4096);
        assert_eq!(payload.get_sg_list()[0].data, 1000 as *const u8);
        assert_eq!(payload.get_sg_list()[1].data, 3000 as *const u8);
        assert_eq!(payload.get_sg_list()[2].data, 5000 as *const u8);
        assert_eq!(payload.get_sg_list()[3].data, 7000 as *const u8);
    }
}
