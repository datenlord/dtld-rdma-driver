use flume::Sender;
use thiserror::Error;

use crate::{
    device::{
        CtrlRbDescOpcode, ToCardCtrlRbDesc, ToCardWorkRbDesc, ToHostCtrlRbDesc, ToHostCtrlRbDescCommon, ToHostWorkRbDesc, ToHostWorkRbDescAck, ToHostWorkRbDescAethCode, ToHostWorkRbDescCommon, ToHostWorkRbDescOpcode, ToHostWorkRbDescRead, ToHostWorkRbDescStatus, ToHostWorkRbDescTransType, ToHostWorkRbDescWriteOrReadResp, ToHostWorkRbDescWriteType, ToHostWorkRbDescWriteWithImm
    },
    types::{MemAccessTypeFlag, Msn, Pmtu, Psn, QpType},
    utils::get_first_packet_max_length,
};

use super::{
    net_agent::{NetAgentError, NetReceiveLogic, NetSendAgent},
    types::{
        Key, Metadata, PDHandle, PKey, PayloadInfo, Qpn, RdmaGeneralMeta, RdmaMessage,
        RdmaMessageMetaCommon, RethHeader, ToCardDescriptor, ToCardReadDescriptor,
        ToCardWriteDescriptor,
    },
};
use std::{
    collections::HashMap,
    sync::{Arc, PoisonError, RwLock},
};

#[derive(Debug,Clone)]
struct QueuePairInner {
    pmtu: Pmtu,
    qp_type: QpType,
    qp_access_flags: MemAccessTypeFlag,
    pdkey: PDHandle,
}

/// The hardware queue pair context
#[derive(Debug)]
struct QueuePair {
    #[allow(dead_code)]
    inner: QueuePairInner,
}

/// The hardware memory region context
#[allow(dead_code)]
#[derive(Debug)]
struct MemoryRegion {
    key: Key,
    acc_flags: MemAccessTypeFlag,
    pdkey: PDHandle,
    addr: u64,
    len: usize,
    pgt_offset: u32,
}

/// The simulating hardware logic of `BlueRDMA`
///
/// Typically, the logic needs a `NetSendAgent` and a `NetReceiveAgent` to send and receive packets.
/// User use the `send` method to send a `ToCardWorkRbDesc` to the network, and use the `update` method to update the hardware context.
/// And when the `recv_agent` is binded, the received packets will be parsed and be pushed to the `to_host_data_descriptor_queue`.
#[derive(Debug)]
pub(crate) struct BlueRDMALogic {
    mr_rkey_table: RwLock<HashMap<Key, Arc<RwLock<MemoryRegion>>>>,
    qp_table: RwLock<HashMap<Qpn, Arc<QueuePair>>>,
    net_send_agent: Arc<dyn NetSendAgent>,
    to_host_data_descriptor_queue: Sender<ToHostWorkRbDesc>,
    to_host_ctrl_descriptor_queue: Sender<ToHostCtrlRbDesc>,
}

#[derive(Error, Debug)]
pub(crate) enum BlueRdmaLogicError {
    #[error("packet process error")]
    NetAgentError(#[from] NetAgentError),
    #[error("Raw packet length is too long. Pmtu is `{0}`, length is `{1}`")]
    RawPacketLengthTooLong(u32, u32),
    #[error("Poison error")]
    Poison,
    #[error("Unreachable")]
    Unreachable,
}

impl<T> From<PoisonError<T>> for BlueRdmaLogicError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::Poison
    }
}

impl BlueRDMALogic {
    pub(crate) fn new(
        net_sender: Arc<dyn NetSendAgent>,
        ctrl_sender: Sender<ToHostCtrlRbDesc>,
        work_sender: Sender<ToHostWorkRbDesc>,
    ) -> Self {
        BlueRDMALogic {
            mr_rkey_table: RwLock::new(HashMap::new()),
            qp_table: RwLock::new(HashMap::new()),
            net_send_agent: net_sender,
            to_host_data_descriptor_queue: work_sender,
            to_host_ctrl_descriptor_queue: ctrl_sender,
        }
    }

    fn send_raw_packet(&self, mut desc: ToCardDescriptor) -> Result<(), BlueRdmaLogicError> {
        let common = desc.common();
        let total_length = common.total_len;
        let pmtu = u32::from(&common.pmtu);
        if total_length > pmtu {
            return Err(BlueRdmaLogicError::RawPacketLengthTooLong(
                pmtu,
                total_length,
            ));
        }
        let dqp_ip = common.dqp_ip;
        let payload = desc.first_sge_mut().cut(total_length)?;
        self.net_send_agent.send_raw(dqp_ip, 4791, &payload)?;
        Ok(())
    }

    fn send_write_only_packet(
        &self,
        mut req: ToCardWriteDescriptor,
        mut meta_data: RdmaGeneralMeta,
    ) -> Result<(), BlueRdmaLogicError> {
        // RdmaWriteOnly or RdmaWriteOnlyWithImmediate
        let payload = req.sg_list.cut_all_levels();

        // if it's a RdmaWriteOnlyWithImmediate, add the immediate data
        let (opcode, imm) = req.write_only_opcode_with_imm();
        meta_data.common_meta.opcode = opcode;
        meta_data.imm = imm;
        meta_data.reth.len = if meta_data.common_meta.opcode.is_first() {
            req.common.total_len
        } else {
            req.sg_list.get_total_length()
        };

        let msg = RdmaMessage {
            meta_data: Metadata::General(meta_data),
            payload,
        };

        self.net_send_agent.send(req.common.dqp_ip, 4791, &msg)?;
        Ok(())
    }

    fn send_read_packet(
        &self,
        req: &ToCardReadDescriptor,
        mut common_meta: RdmaMessageMetaCommon,
    ) -> Result<(), BlueRdmaLogicError> {
        let local_sa = &req.sge.data[0];
        common_meta.opcode = ToHostWorkRbDescOpcode::RdmaReadRequest;

        let msg = RdmaMessage {
            meta_data: Metadata::General(RdmaGeneralMeta {
                common_meta,
                reth: RethHeader {
                    va: req.common.raddr,
                    rkey: Key::new(req.common.rkey.get()),
                    len: req.common.total_len,
                },
                imm: None,
                secondary_reth: Some(RethHeader {
                    va: local_sa.addr,
                    rkey: local_sa.key,
                    len: local_sa.len,
                }),
            }),
            payload: PayloadInfo::new(),
        };

        self.net_send_agent.send(req.common.dqp_ip, 4791, &msg)?;
        Ok(())
    }

    /// Convert a `ToCardWorkRbDesc` to a `RdmaMessage` and call the `net_send_agent` to send through the network.
    pub(crate) fn send(&self, desc: Box<ToCardWorkRbDesc>) -> Result<(), BlueRdmaLogicError> {
        let desc = ToCardDescriptor::from(desc);
        // if it's a raw packet, send it directly
        if desc.is_raw_packet() {
            return self.send_raw_packet(desc);
        }

        let common_meta = {
            let common = desc.common();
            RdmaMessageMetaCommon {
                tran_type: desc.common().qp_type.into(),
                opcode: ToHostWorkRbDescOpcode::RdmaWriteOnly,
                solicited: false,
                // We use the pkey to store msn
                pkey: PKey::new(common.msn.get()),
                dqpn: Qpn::new(common.dqpn.get()),
                ack_req: false,
                psn: Psn::new(common.psn.get()),
            }
        };

        #[allow(clippy::arithmetic_side_effects)]
        match desc {
            ToCardDescriptor::Write(mut req) => {
                log::info!("{:?}", req);
                let pmtu = u32::from(&req.common.pmtu);
                let first_packet_max_length = get_first_packet_max_length(req.common.raddr, pmtu);

                // a default metadata. It will be updated later
                let mut meta_data = RdmaGeneralMeta {
                    common_meta,
                    reth: RethHeader {
                        va: req.common.raddr,
                        rkey: Key::new(req.common.rkey.get()),
                        len: req.common.total_len,
                    },
                    imm: None,
                    secondary_reth: None,
                };
                let sge_total_length = req.sg_list.get_total_length();
                if sge_total_length <= first_packet_max_length {
                    return self.send_write_only_packet(req, meta_data);
                }
                // othetrwise send the data in multiple packets
                // we specifically handle the first and last packet
                // The first va might not align to pmtu
                let mut cur_va = req.common.raddr;
                let mut cur_len = sge_total_length;
                let mut psn = req.common.psn;

                // since the packet size is larger than first_packet_max_length, first_packet_length should equals
                // to first_packet_max_length
                let first_packet_length = first_packet_max_length;

                let payload = req.sg_list.cut(first_packet_length)?;
                meta_data.common_meta.opcode = req.write_first_opcode();
                meta_data.reth.len = if meta_data.common_meta.opcode.is_first() {
                    req.common.total_len
                } else {
                    first_packet_length
                };
                meta_data.reth.va = cur_va;
                let msg = RdmaMessage {
                    meta_data: Metadata::General(meta_data.clone()),
                    payload,
                };
                cur_len -= first_packet_length;
                psn = psn.wrapping_add(1);
                cur_va = cur_va.wrapping_add(u64::from(first_packet_length));
                self.net_send_agent.send(req.common.dqp_ip, 4791, &msg)?;

                // send the middle packets
                meta_data.reth.len = pmtu;
                while cur_len > pmtu {
                    let middle_payload = req.sg_list.cut(pmtu)?;
                    meta_data.common_meta.opcode = req.write_middle_opcode();
                    meta_data.reth.va = cur_va;
                    meta_data.common_meta.psn = psn;
                    let middle_msg = RdmaMessage {
                        meta_data: Metadata::General(meta_data.clone()),
                        payload: middle_payload,
                    };
                    cur_len -= pmtu;
                    psn = psn.wrapping_add(1);
                    cur_va = cur_va.wrapping_add(u64::from(pmtu));
                    self.net_send_agent
                        .send(req.common.dqp_ip, 4791, &middle_msg)?;
                }

                // cur_len <= pmtu, send last packet
                let last_payload = req.sg_list.cut(cur_len)?;

                // The last packet may be with immediate data
                let (opcode, imm) = req.write_last_opcode_with_imm();
                meta_data.common_meta.opcode = opcode;
                meta_data.common_meta.psn = psn;
                meta_data.imm = imm;
                meta_data.reth.va = cur_va;
                meta_data.reth.len = cur_len;
                let last_msg = RdmaMessage {
                    meta_data: Metadata::General(meta_data),
                    payload: last_payload,
                };
                self.net_send_agent
                    .send(req.common.dqp_ip, 4791, &last_msg)?;
            }
            ToCardDescriptor::Read(req) => {
                self.send_read_packet(&req, common_meta)?;
            }
        }
        Ok(())
    }

    #[allow(clippy::unwrap_in_result)]
    pub(crate) fn update(&self, desc: ToCardCtrlRbDesc) -> Result<(), BlueRdmaLogicError> {
        let opcode = to_host_ctrl_opcode(&desc);
        let (op_id,is_succ) = match desc {
            ToCardCtrlRbDesc::QpManagement(desc) => {
                let mut qp_table = self.qp_table.write()?;
                let qpn = Qpn::new(desc.qpn.get());
                let qp_inner = QueuePairInner {
                    pmtu: desc.pmtu,
                    qp_type: desc.qp_type,
                    qp_access_flags: desc.rq_acc_flags,
                    pdkey: PDHandle::new(desc.pd_hdl),
                };
                let is_success = if desc.is_valid {
                    // create
                    let _result = qp_table
                        .entry(qpn)
                        .and_modify(|existing_qp| {
                            *existing_qp = Arc::new(QueuePair {
                                inner: qp_inner.clone(),
                            });
                        })
                        .or_insert(Arc::new(QueuePair { inner: qp_inner }));
                    true
                } else {
                    // delete
                    if qp_table.get(&qpn).is_some() {
                        // exist
                        let _: Option<Arc<QueuePair>> = qp_table.remove(&qpn);
                        true
                    } else {
                        false
                    }
                };
                (desc.common.op_id, is_success)
            }
            ToCardCtrlRbDesc::UpdateMrTable(desc) => {
                let mut mr_table = self.mr_rkey_table.write()?;
                let key = Key::new(desc.key.get());
                let mr = MemoryRegion {
                    key,
                    acc_flags: desc.acc_flags,
                    pdkey: PDHandle::new(desc.pd_hdl),
                    addr: desc.addr,
                    len: desc.len as usize,
                    pgt_offset: desc.pgt_offset,
                };
                if let Some(mr_context) = mr_table.get(&mr.key) {
                    let mut guard = mr_context.write()?;
                    *guard = mr;
                } else {
                    let mr = Arc::new(RwLock::new(mr));
                    // we have ensured that the qpn is not exists.
                    let _: Option<Arc<RwLock<MemoryRegion>>> = mr_table.insert(key, mr);
                }
                (desc.common.op_id,true)
            }
            // Userspace types use virtual address directly
            ToCardCtrlRbDesc::UpdatePageTable(desc) => {
                (desc.common.op_id,true)
            }
            ToCardCtrlRbDesc::SetNetworkParam(desc) => {
                (desc.common.op_id,true)
            }
            ToCardCtrlRbDesc::SetRawPacketReceiveMeta(desc) => {
                (desc.common.op_id,true)
            }
            ToCardCtrlRbDesc::UpdateErrorPsnRecoverPoint(desc) => {
                (desc.common.op_id,true)
            }
        };
        let resp_desc = ToHostCtrlRbDesc{
            common: ToHostCtrlRbDescCommon{
                op_id,
                is_success: is_succ,
                opcode
            },
        };  
        #[allow(clippy::unwrap_used)] // if the pipe in software is broken, we should panic.
        {
            self.to_host_ctrl_descriptor_queue
                .send(resp_desc)
                .unwrap();
        }
        Ok(())
    }

    /// Validate the permission, va and length of corresponding memory region.
    ///
    /// The function will check the following things:
    /// * if the rkey is valid. If not, return `InvMrKey`
    /// * if the permission is valid. If not, return `InvAccFlag`
    /// * if the va and length are valid. If not, return `InvMrRegion`
    /// Otherwise, return `RDMA_REQ_ST_NORMAL`
    fn validate_rkey(
        &self,
        rkey: Key,
        needed_permissions: MemAccessTypeFlag,
        va: u64,
        length: u32,
    ) -> Result<ToHostWorkRbDescStatus, BlueRdmaLogicError> {
        let mr_rkey_table = self.mr_rkey_table.read()?;
        let Some(mr) = mr_rkey_table.get(&rkey) else {
            return Ok(ToHostWorkRbDescStatus::InvMrKey);
        };

        let read_guard = mr.read()?;

        // check the permission.
        if !read_guard.acc_flags.contains(needed_permissions) {
            return Ok(ToHostWorkRbDescStatus::InvAccFlag);
        }

        // check if the va and length are valid.
        if read_guard.addr > va
            || read_guard.addr.wrapping_add(read_guard.len as u64)
                < va.wrapping_add(u64::from(length))
        {
            return Ok(ToHostWorkRbDescStatus::InvMrRegion);
        }
        Ok(ToHostWorkRbDescStatus::Normal)
    }
}

unsafe impl Send for BlueRDMALogic {}
unsafe impl Sync for BlueRDMALogic {}

fn recv_default_meta(message: &RdmaMessage) -> ToHostWorkRbDescCommon {
    #[allow(clippy::cast_possible_truncation)]
    ToHostWorkRbDescCommon {
        status: ToHostWorkRbDescStatus::Unknown,
        trans: ToHostWorkRbDescTransType::Rc,
        dqpn: crate::types::Qpn::new(message.meta_data.common_meta().dqpn.get()),
        msn: Msn::new(message.meta_data.common_meta().pkey.get()),
        expected_psn: crate::types::Psn::new(0),
    }
}

impl NetReceiveLogic<'_> for BlueRDMALogic {
    fn recv(&self, message: &mut RdmaMessage) {
        let meta = &message.meta_data;
        let mut common = recv_default_meta(message);
        let descriptor = match meta {
            Metadata::General(header) => {
                // validate the rkey
                let reky = header.reth.rkey;
                let needed_permissions = header.needed_permissions();
                let va = header.reth.va;
                let len = header.reth.len;
                let Ok(status) = self.validate_rkey(reky, needed_permissions, va, len) else {
                    log::error!("Failed to validate the rkey");
                    return;
                };

                // Copy the payload to the memory
                if status.is_ok() && header.has_payload() {
                    message.payload.copy_to(va as *mut u8);
                }

                // The default value will not be used since the `write_type` will only appear
                // in those write related opcodes.
                let write_type = header
                    .common_meta
                    .opcode
                    .write_type()
                    .unwrap_or(ToHostWorkRbDescWriteType::Only);

                common.status = status;
                let is_read_resp = header.common_meta.opcode.is_resp();

                // Write a descriptor to host
                match header.common_meta.opcode {
                    ToHostWorkRbDescOpcode::RdmaWriteFirst
                    | ToHostWorkRbDescOpcode::RdmaWriteMiddle
                    | ToHostWorkRbDescOpcode::RdmaWriteLast
                    | ToHostWorkRbDescOpcode::RdmaWriteOnly
                    | ToHostWorkRbDescOpcode::RdmaReadResponseFirst
                    | ToHostWorkRbDescOpcode::RdmaReadResponseMiddle
                    | ToHostWorkRbDescOpcode::RdmaReadResponseLast
                    | ToHostWorkRbDescOpcode::RdmaReadResponseOnly => {
                        ToHostWorkRbDesc::WriteOrReadResp(ToHostWorkRbDescWriteOrReadResp {
                            common,
                            is_read_resp,
                            write_type,
                            psn: header.common_meta.psn,
                            addr: header.reth.va,
                            len: header.reth.len,
                            can_auto_ack: false,
                        })
                    }
                    ToHostWorkRbDescOpcode::RdmaWriteLastWithImmediate
                    | ToHostWorkRbDescOpcode::RdmaWriteOnlyWithImmediate => {
                        ToHostWorkRbDesc::WriteWithImm(ToHostWorkRbDescWriteWithImm {
                            common,
                            write_type,
                            psn: header.common_meta.psn,
                            imm: header.imm.unwrap_or_else(|| {
                                log::error!("The immediate data is not found");
                                0
                            }),
                            addr: header.reth.va,
                            len: header.reth.len,
                            key: header.reth.rkey.into(),
                        })
                    }
                    ToHostWorkRbDescOpcode::RdmaReadRequest => {
                        let Some(sec_reth) = header.secondary_reth else {
                            log::error!("The secondary reth is not found");
                            return;
                        };
                        ToHostWorkRbDesc::Read(ToHostWorkRbDescRead {
                            common,
                            len: header.reth.len,
                            laddr: header.reth.va,
                            lkey: header.reth.rkey.into(),
                            raddr: sec_reth.va,
                            rkey: sec_reth.rkey.into(),
                        })
                    }
                    ToHostWorkRbDescOpcode::Acknowledge => {
                        unimplemented!()
                    }
                }
            }
            Metadata::Acknowledge(header) => {
                common.status = ToHostWorkRbDescStatus::Normal;
                match header.aeth_code {
                    ToHostWorkRbDescAethCode::Ack => ToHostWorkRbDesc::Ack(ToHostWorkRbDescAck {
                        common,
                        #[allow(clippy::cast_possible_truncation)]
                        msn: crate::types::Msn::new(header.msn as u16), // msn is u16 currently. So we can just truncate it.
                        value: header.aeth_value,
                        psn: crate::types::Psn::new(header.common_meta.psn.get()),
                        code: ToHostWorkRbDescAethCode::Ack,
                        retry_psn: Psn::default(),
                    }),
                    ToHostWorkRbDescAethCode::Rnr
                    | ToHostWorkRbDescAethCode::Rsvd
                    | ToHostWorkRbDescAethCode::Nak => {
                        // just ignore
                        unimplemented!()
                    }
                }
            }
        };

        // push the descriptor to the ring buffer
        #[allow(clippy::unwrap_used)] // if the pipe in software is broken, we should panic.
        {
            self.to_host_data_descriptor_queue.send(descriptor).unwrap();
        }
    }
}

fn to_host_ctrl_opcode(desc : &ToCardCtrlRbDesc) -> CtrlRbDescOpcode {
    match desc {
        ToCardCtrlRbDesc::UpdateMrTable(_) => CtrlRbDescOpcode::UpdateMrTable,
        ToCardCtrlRbDesc::UpdatePageTable(_) => CtrlRbDescOpcode::UpdatePageTable,
        ToCardCtrlRbDesc::QpManagement(_) => CtrlRbDescOpcode::QpManagement,
        ToCardCtrlRbDesc::SetNetworkParam(_) => CtrlRbDescOpcode::SetNetworkParam,
        ToCardCtrlRbDesc::SetRawPacketReceiveMeta(_) => CtrlRbDescOpcode::SetRawPacketReceiveMeta,
        ToCardCtrlRbDesc::UpdateErrorPsnRecoverPoint(_) => CtrlRbDescOpcode::UpdateErrorPsnRecoverPoint,
    }
}

#[cfg(test)]
mod tests {
    use std::{net::Ipv4Addr, sync::Arc};

    use flume::unbounded;

    use crate::{
        device::{
            software::{
                net_agent::{NetAgentError, NetSendAgent},
                types::{Key, PayloadInfo, Qpn, RdmaMessage},
            },
            ToCardCtrlRbDesc, ToCardCtrlRbDescCommon, ToCardCtrlRbDescQpManagement,
            ToCardCtrlRbDescUpdateMrTable,
        },
        types::{MemAccessTypeFlag, Pmtu, QpType},
    };

    use super::BlueRDMALogic;

    // test update mr table, qp table
    #[test]
    fn test_logic_update() {
        #[derive(Debug)]
        struct DummpyProxy;

        impl NetSendAgent for DummpyProxy {
            fn send(
                &self,
                _: Ipv4Addr,
                _: u16,
                _message: &RdmaMessage,
            ) -> Result<(), NetAgentError> {
                Ok(())
            }

            fn send_raw(
                &self,
                _: Ipv4Addr,
                _: u16,
                _payload: &PayloadInfo,
            ) -> Result<(), NetAgentError> {
                Ok(())
            }
        }
        let agent = Arc::new(DummpyProxy);
        let (ctrl_sender, _ctrl_receiver) = unbounded();
        let (work_sender, _work_receiver) = unbounded();
        let logic = BlueRDMALogic::new(Arc::<DummpyProxy>::clone(&agent), ctrl_sender, work_sender);
        // test updating qp
        {
            let desc = ToCardCtrlRbDesc::QpManagement(ToCardCtrlRbDescQpManagement {
                common: ToCardCtrlRbDescCommon { op_id: 0 },
                is_valid: true,
                qpn: crate::Qpn::new(1234),
                pd_hdl: 1,
                qp_type: QpType::Rc,
                rq_acc_flags: MemAccessTypeFlag::IbvAccessRemoteWrite,
                pmtu: Pmtu::Mtu1024,
                peer_qpn: crate::Qpn::new(1234),
            });
            logic.update(desc).unwrap();
            {
                let guard = logic.qp_table.read().unwrap();
                let qp_context = guard.get(&Qpn::new(1234)).unwrap();
                let inner = &qp_context.inner;
                assert!(matches!(inner.pmtu, Pmtu::Mtu1024));
                assert!(matches!(inner.qp_type, QpType::Rc));
                assert!(inner
                    .qp_access_flags
                    .contains(MemAccessTypeFlag::IbvAccessRemoteWrite));
            }

            // write again
            let desc = ToCardCtrlRbDesc::QpManagement(ToCardCtrlRbDescQpManagement {
                common: ToCardCtrlRbDescCommon { op_id: 0 },
                is_valid: true,
                qpn: crate::Qpn::new(1234),
                pd_hdl: 1,
                qp_type: QpType::Rc,
                rq_acc_flags: MemAccessTypeFlag::IbvAccessRemoteWrite,
                pmtu: Pmtu::Mtu2048,
                peer_qpn: crate::Qpn::new(1234),
            });
            logic.update(desc).unwrap();
            {
                let guard = logic.qp_table.read().unwrap();
                let qp_context = guard.get(&Qpn::new(1234)).unwrap();
                let inner = &qp_context.inner;
                assert!(matches!(inner.pmtu, Pmtu::Mtu2048));
                assert!(matches!(inner.qp_type, QpType::Rc));
                assert!(inner
                    .qp_access_flags
                    .contains(MemAccessTypeFlag::IbvAccessRemoteWrite));
            }
        }

        // test updating mr
        {
            let desc = ToCardCtrlRbDesc::UpdateMrTable(ToCardCtrlRbDescUpdateMrTable {
                common: ToCardCtrlRbDescCommon { op_id: 0 },
                addr: 0x1234567812345678,
                len: 1024 * 16,
                key: crate::types::Key::new(1234),
                pd_hdl: 0,
                acc_flags: MemAccessTypeFlag::IbvAccessRemoteWrite,
                pgt_offset: 0,
            });
            logic.update(desc).unwrap();
            {
                let guard = logic.mr_rkey_table.read().unwrap();
                let mr_context = guard.get(&Key::new(1234_u32)).unwrap();
                let read_guard = mr_context.read().unwrap();
                assert_eq!(read_guard.addr, 0x1234567812345678);
                assert_eq!(read_guard.len, 1024 * 16);
                assert_eq!(read_guard.pdkey.get(), 0);
                assert!(read_guard
                    .acc_flags
                    .contains(MemAccessTypeFlag::IbvAccessRemoteWrite));
                assert_eq!(read_guard.pgt_offset, 0);
            }

            // update again
            let desc = ToCardCtrlRbDesc::UpdateMrTable(ToCardCtrlRbDescUpdateMrTable {
                common: ToCardCtrlRbDescCommon { op_id: 0 },
                addr: 0x1234567812345678,
                len: 1024 * 24,
                key: crate::types::Key::new(1234),
                pd_hdl: 0,
                acc_flags: (MemAccessTypeFlag::IbvAccessRemoteWrite
                    | MemAccessTypeFlag::IbvAccessRemoteRead),
                pgt_offset: 0,
            });
            logic.update(desc).unwrap();
            {
                let guard = logic.mr_rkey_table.read().unwrap();
                let mr_context = guard.get(&Key::new(1234_u32)).unwrap();
                let read_guard = mr_context.read().unwrap();
                assert_eq!(read_guard.addr, 0x1234567812345678);
                assert_eq!(read_guard.len, 1024 * 24);
                assert_eq!(read_guard.pdkey.get(), 0);
                assert!(read_guard
                    .acc_flags
                    .contains(MemAccessTypeFlag::IbvAccessRemoteWrite));
                assert!(read_guard
                    .acc_flags
                    .contains(MemAccessTypeFlag::IbvAccessRemoteRead));
                assert_eq!(read_guard.pgt_offset, 0);
            }
        }
    }
}
