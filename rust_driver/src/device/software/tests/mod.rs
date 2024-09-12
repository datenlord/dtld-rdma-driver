use std::net::Ipv4Addr;

use eui48::MacAddress;

use crate::{
    device::{
        ToCardCtrlRbDesc, ToCardCtrlRbDescCommon, ToCardCtrlRbDescQpManagement,
        ToCardCtrlRbDescUpdateMrTable, ToCardWorkRbDesc, ToCardWorkRbDescCommon,
        ToCardWorkRbDescOpcode, ToCardWorkRbDescRead, ToCardWorkRbDescWrite,
        ToCardWorkRbDescWriteWithImm,
    },
    types::{MemAccessTypeFlag, Pmtu, QpType, WorkReqSendFlag},
};

use super::types::{Key, SGList, SGListElementWithKey};

mod test_device;
mod test_logic;
mod test_packet;
mod test_utils;

pub(crate) struct SGListBuilder {
    sg_list: Vec<SGListElementWithKey>,
}

impl SGListBuilder {
    pub(crate) fn new() -> Self {
        SGListBuilder {
            sg_list: Vec::new(),
        }
    }

    pub(crate) fn with_sge(&mut self, addr: u64, len: u32, key: u32) -> &mut Self {
        self.sg_list.push(SGListElementWithKey {
            addr,
            len,
            key: Key::new(key),
        });
        self
    }

    pub(crate) fn build(&self) -> SGList {
        let mut sg_list = SGList::new();
        for sge in self.sg_list.iter() {
            sg_list.data[sg_list.len as usize] = *sge;
            sg_list.len += 1;
        }
        while sg_list.len < 4 {
            sg_list.data[sg_list.len as usize] = SGListElementWithKey::default();
            sg_list.len += 1;
        }
        sg_list
    }
}

pub(crate) struct ToCardWorkRbDescBuilder {
    opcode: Option<ToCardWorkRbDescOpcode>,
    total_len: Option<u32>,
    raddr: Option<u64>,
    rkey: Option<u32>,
    dqpn: Option<u32>,
    pmtu: Option<Pmtu>,
    qp_type: Option<QpType>,
    psn: Option<u32>,
    flags: Option<WorkReqSendFlag>,
    is_first: Option<bool>,
    is_last: Option<bool>,
    imm: Option<u32>,
    sg_list: Option<SGList>,
}

impl ToCardWorkRbDescBuilder {
    pub(crate) fn default() -> Self {
        Self {
            opcode: None,
            total_len: None,
            raddr: None,
            rkey: None,
            dqpn: None,
            pmtu: None,
            qp_type: Some(QpType::Rc),
            psn: Some(0),
            flags: Some(WorkReqSendFlag::empty()),
            is_first: Some(true),
            is_last: Some(true),
            imm: None,
            sg_list: None,
        }
    }
    pub(crate) fn with_is_first(&mut self, is_first: bool) -> &mut Self {
        self.is_first = Some(is_first);
        self
    }

    pub(crate) fn with_is_last(&mut self, is_last: bool) -> &mut Self {
        self.is_last = Some(is_last);
        self
    }

    pub(crate) fn with_opcode(&mut self, opcode: ToCardWorkRbDescOpcode) -> &mut Self {
        self.opcode = Some(opcode);
        self
    }

    pub(crate) fn with_total_len(&mut self, total_len: u32) -> &mut Self {
        self.total_len = Some(total_len);
        self
    }

    pub(crate) fn with_raddr(&mut self, raddr: u64) -> &mut Self {
        self.raddr = Some(raddr);
        self
    }

    pub(crate) fn with_rkey(&mut self, rkey: u32) -> &mut Self {
        self.rkey = Some(rkey);
        self
    }

    pub(crate) fn with_dqpn(&mut self, dqpn: u32) -> &mut Self {
        self.dqpn = Some(dqpn);
        self
    }

    pub(crate) fn with_pmtu(&mut self, pmtu: Pmtu) -> &mut Self {
        self.pmtu = Some(pmtu);
        self
    }

    pub(crate) fn with_qp_type(&mut self, qp_type: QpType) -> &mut Self {
        self.qp_type = Some(qp_type);
        self
    }

    pub(crate) fn with_psn(&mut self, psn: u32) -> &mut Self {
        self.psn = Some(psn);
        self
    }

    pub(crate) fn with_flags(&mut self, flags: WorkReqSendFlag) -> &mut Self {
        self.flags = Some(flags);
        self
    }

    pub(crate) fn with_imm(&mut self, imm: u32) -> &mut Self {
        self.imm = Some(imm);
        self
    }

    pub(crate) fn with_sg_list(&mut self, sg_list: SGList) -> &mut Self {
        self.sg_list = Some(sg_list);
        self
    }

    pub(crate) fn build(&mut self) -> Box<ToCardWorkRbDesc> {
        let common = ToCardWorkRbDescCommon {
            total_len: self.total_len.unwrap(),
            raddr: self.raddr.unwrap(),
            rkey: crate::types::Key::new(self.rkey.unwrap()),
            dqpn: crate::types::Qpn::new(self.dqpn.unwrap()),
            pmtu: self.pmtu.unwrap(),
            qp_type: self.qp_type.unwrap(),
            psn: crate::types::Psn::new(self.psn.unwrap()),
            flags: self.flags.unwrap(),
            dqp_ip: Ipv4Addr::LOCALHOST,
            mac_addr: MacAddress::default(),
            msn: crate::types::Msn::new(0),
        };
        let (sge0, sge1, sge2, sge3) = self.sg_list.take().unwrap().into_four_sges();
        let desc = match self.opcode.clone().unwrap() {
            ToCardWorkRbDescOpcode::Write => ToCardWorkRbDesc::Write(ToCardWorkRbDescWrite {
                common,
                is_first: self.is_first.unwrap(),
                is_last: self.is_last.unwrap(),
                sge0,
                sge1,
                sge2,
                sge3,
            }),
            ToCardWorkRbDescOpcode::Read => {
                ToCardWorkRbDesc::Read(ToCardWorkRbDescRead { common, sge: sge0 })
            }
            ToCardWorkRbDescOpcode::WriteWithImm => {
                let imm = if matches!(common.qp_type, QpType::RawPacket) {
                    0
                }else{
                    self.imm.unwrap()
                };
                ToCardWorkRbDesc::WriteWithImm(ToCardWorkRbDescWriteWithImm {
                    common,
                    is_first: self.is_first.unwrap(),
                    is_last: self.is_last.unwrap(),
                    imm,
                    sge0,
                    sge1,
                    sge2,
                    sge3,
                })
            }
            ToCardWorkRbDescOpcode::ReadResp => ToCardWorkRbDesc::ReadResp(ToCardWorkRbDescWrite {
                common,
                is_first: self.is_first.unwrap(),
                is_last: self.is_last.unwrap(),
                sge0,
                sge1,
                sge2,
                sge3,
            }),
        };
        Box::new(desc)
    }
}

pub(crate) enum ToCardCtrlRbDescBuilderType {
    UpdateMrTable,
    QpManagement,
}
pub(crate) struct ToCardCtrlRbDescBuilder {
    type_: ToCardCtrlRbDescBuilderType,
    op_id: Option<u32>,
    addr: Option<u64>,
    len: Option<u32>,
    key: Option<u32>,
    pd_hdl: Option<u32>,
    acc_flags: Option<MemAccessTypeFlag>,
    pgt_offset: Option<u32>,
    is_valid: Option<bool>,
    qpn: Option<u32>,
    qp_type: Option<QpType>,
    rq_acc_flags: Option<MemAccessTypeFlag>,
    pmtu: Option<Pmtu>,
}

impl ToCardCtrlRbDescBuilder {
    pub(crate) fn new(type_: ToCardCtrlRbDescBuilderType) -> Self {
        Self {
            type_,
            op_id: Some(0),
            addr: None,
            len: None,
            key: None,
            pd_hdl: None,
            acc_flags: None,
            pgt_offset: None,
            is_valid: None,
            qpn: None,
            qp_type: None,
            rq_acc_flags: Some(MemAccessTypeFlag::empty()),
            pmtu: None,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn with_op_id(&mut self, op_id: u32) -> &mut Self {
        self.op_id = Some(op_id);
        self
    }

    pub(crate) fn with_addr(&mut self, addr: u64) -> &mut Self {
        self.addr = Some(addr);
        self
    }

    pub(crate) fn with_len(&mut self, len: u32) -> &mut Self {
        self.len = Some(len);
        self
    }

    pub(crate) fn with_key(&mut self, key: u32) -> &mut Self {
        self.key = Some(key);
        self
    }

    pub(crate) fn with_pd_hdl(&mut self, pd_hdl: u32) -> &mut Self {
        self.pd_hdl = Some(pd_hdl);
        self
    }

    pub(crate) fn with_acc_flags(&mut self, acc_flags: MemAccessTypeFlag) -> &mut Self {
        self.acc_flags = Some(acc_flags);
        self
    }

    pub(crate) fn with_pgt_offset(&mut self, pgt_offset: u32) -> &mut Self {
        self.pgt_offset = Some(pgt_offset);
        self
    }

    pub(crate) fn with_is_valid(&mut self, is_valid: bool) -> &mut Self {
        self.is_valid = Some(is_valid);
        self
    }

    pub(crate) fn with_qpn(&mut self, qpn: u32) -> &mut Self {
        self.qpn = Some(qpn);
        self
    }

    pub(crate) fn with_qp_type(&mut self, qp_type: QpType) -> &mut Self {
        self.qp_type = Some(qp_type);
        self
    }

    pub(crate) fn with_rq_acc_flags(&mut self, rq_acc_flags: MemAccessTypeFlag) -> &mut Self {
        self.rq_acc_flags = Some(rq_acc_flags);
        self
    }

    pub(crate) fn with_pmtu(&mut self, pmtu: Pmtu) -> &mut Self {
        self.pmtu = Some(pmtu);
        self
    }

    pub(crate) fn build(&self) -> ToCardCtrlRbDesc {
        let common = ToCardCtrlRbDescCommon {
            op_id: self.op_id.unwrap(),
        };
        match &self.type_ {
            ToCardCtrlRbDescBuilderType::UpdateMrTable => {
                ToCardCtrlRbDesc::UpdateMrTable(ToCardCtrlRbDescUpdateMrTable {
                    common,
                    addr: self.addr.unwrap(),
                    len: self.len.unwrap(),
                    key: crate::types::Key::new(self.key.unwrap()),
                    pd_hdl: self.pd_hdl.unwrap(),
                    acc_flags: self.acc_flags.unwrap(),
                    pgt_offset: self.pgt_offset.unwrap(),
                })
            }
            ToCardCtrlRbDescBuilderType::QpManagement => {
                ToCardCtrlRbDesc::QpManagement(ToCardCtrlRbDescQpManagement {
                    common,
                    is_valid: self.is_valid.unwrap(),
                    qpn: crate::Qpn::new(self.qpn.unwrap()),
                    pd_hdl: self.pd_hdl.unwrap(),
                    qp_type: self.qp_type.unwrap(),
                    rq_acc_flags: self.rq_acc_flags.unwrap(),
                    pmtu: self.pmtu.unwrap(),
                    peer_qpn: crate::Qpn::new(1234),
                })
            }
        }
    }
}
