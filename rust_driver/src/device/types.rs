// TODO: implement for handling in big-endian machine
use crate::{
    device::layout::{
        CmdQueueReqDescQpManagementSeg0, CmdQueueReqDescSetNetworkParam,
        CmdQueueReqDescSetRawPacketReceiveMeta, CmdQueueReqDescUpdateErrRecoverPoint,
        CmdQueueReqDescUpdateMrTable, CmdQueueReqDescUpdatePGT,
        MetaReportQueueDescFragSecondaryRETH,
    },
    types::{Imm, Key, MemAccessTypeFlag, Msn, Pmtu, Psn, QpType, Qpn, Sge, WorkReqSendFlag},
    utils::u8_slice_to_u64,
    Error,
};
use eui48::MacAddress;
use num_enum::TryFromPrimitive;
use std::{net::Ipv4Addr, thread::sleep, time::{Duration, Instant}};

use super::layout::{
    CmdQueueDescCommonHead, MetaReportQueueDescBthReth, MetaReportQueueDescFragAETH,
    MetaReportQueueDescFragBTH, MetaReportQueueDescFragImmDT, MetaReportQueueDescFragRETH,
    SendQueueDescCommonHead, SendQueueReqDescFragSGE, SendQueueReqDescSeg0, SendQueueReqDescSeg1,
};

#[derive(Debug)]
pub(crate) enum ToCardCtrlRbDesc {
    UpdateMrTable(ToCardCtrlRbDescUpdateMrTable),
    UpdatePageTable(ToCardCtrlRbDescUpdatePageTable),
    QpManagement(ToCardCtrlRbDescQpManagement),
    SetNetworkParam(ToCardCtrlRbDescSetNetworkParam),
    SetRawPacketReceiveMeta(ToCardCtrlRbDescSetRawPacketReceiveMeta),
    UpdateErrorPsnRecoverPoint(ToCardCtrlRbDescUpdateErrPsnRecoverPoint),
}

impl ToCardCtrlRbDesc {
    pub(crate) fn set_id(&mut self, id: u32) {
        match self {
            ToCardCtrlRbDesc::UpdateMrTable(desc) => desc.common.op_id = id,
            ToCardCtrlRbDesc::UpdatePageTable(desc) => desc.common.op_id = id,
            ToCardCtrlRbDesc::QpManagement(desc) => desc.common.op_id = id,
            ToCardCtrlRbDesc::SetNetworkParam(desc) => desc.common.op_id = id,
            ToCardCtrlRbDesc::SetRawPacketReceiveMeta(desc) => desc.common.op_id = id,
            ToCardCtrlRbDesc::UpdateErrorPsnRecoverPoint(desc) => desc.common.op_id = id,
        }
    }
}

#[derive(Debug)]
pub(crate) struct ToHostCtrlRbDesc {
    pub(crate) common: ToHostCtrlRbDescCommon,
}

#[derive(Clone, Debug)]
pub(crate) enum ToCardWorkRbDesc {
    Read(ToCardWorkRbDescRead),
    Write(ToCardWorkRbDescWrite),
    WriteWithImm(ToCardWorkRbDescWriteWithImm),
    ReadResp(ToCardWorkRbDescWrite),
}

#[derive(Debug)]
pub(crate) enum ToHostWorkRbDesc {
    Read(ToHostWorkRbDescRead),
    WriteOrReadResp(ToHostWorkRbDescWriteOrReadResp),
    WriteWithImm(ToHostWorkRbDescWriteWithImm),
    Ack(ToHostWorkRbDescAck),
    Raw(ToHostWorkRbDescRaw),
}

impl ToHostWorkRbDesc {
    pub(crate) fn status(&self) -> &ToHostWorkRbDescStatus {
        match self {
            ToHostWorkRbDesc::Read(desc) => &desc.common.status,
            ToHostWorkRbDesc::WriteOrReadResp(desc) => &desc.common.status,
            ToHostWorkRbDesc::WriteWithImm(desc) => &desc.common.status,
            ToHostWorkRbDesc::Ack(desc) => &desc.common.status,
            ToHostWorkRbDesc::Raw(desc) => &desc.common.status,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct ToCardCtrlRbDescCommon {
    pub(crate) op_id: u32, // user_data
}

#[derive(Debug)]
pub(crate) struct ToCardCtrlRbDescUpdateMrTable {
    pub(crate) common: ToCardCtrlRbDescCommon,
    pub(crate) addr: u64,
    pub(crate) len: u32,
    pub(crate) key: Key,
    pub(crate) pd_hdl: u32,
    pub(crate) acc_flags: MemAccessTypeFlag,
    pub(crate) pgt_offset: u32,
}

#[derive(Debug)]
pub(crate) struct ToCardCtrlRbDescUpdatePageTable {
    pub(crate) common: ToCardCtrlRbDescCommon,
    pub(crate) start_addr: u64,
    pub(crate) pgt_idx: u32,  //offset
    pub(crate) pgte_cnt: u32, //bytes
}

#[derive(Debug)]
pub(crate) struct ToCardCtrlRbDescQpManagement {
    pub(crate) common: ToCardCtrlRbDescCommon,
    pub(crate) is_valid: bool,
    pub(crate) qpn: Qpn,
    pub(crate) pd_hdl: u32,
    pub(crate) qp_type: QpType,
    pub(crate) rq_acc_flags: MemAccessTypeFlag,
    pub(crate) pmtu: Pmtu,
    pub(crate) peer_qpn: Qpn,
}

#[derive(Debug)]
pub(crate) struct ToCardCtrlRbDescSetNetworkParam {
    pub(crate) common: ToCardCtrlRbDescCommon,
    pub(crate) gateway: Ipv4Addr,
    pub(crate) netmask: Ipv4Addr,
    pub(crate) ipaddr: Ipv4Addr,
    pub(crate) macaddr: MacAddress,
}

#[derive(Debug)]
pub(crate) struct ToCardCtrlRbDescSetRawPacketReceiveMeta {
    pub(crate) common: ToCardCtrlRbDescCommon,
    pub(crate) base_write_addr: u64,
    pub(crate) key: Key,
}

#[derive(Debug)]
pub(crate) struct ToCardCtrlRbDescUpdateErrPsnRecoverPoint {
    pub(crate) common: ToCardCtrlRbDescCommon,
    pub(crate) qpn: Qpn,
    pub(crate) recover_psn: Psn,
}

#[derive(Debug)]
pub(crate) struct ToHostCtrlRbDescCommon {
    pub(crate) op_id: u32, // user_data
    pub(crate) opcode: CtrlRbDescOpcode,
    pub(crate) is_success: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ToCardWorkRbDescCommon {
    pub(crate) total_len: u32,
    pub(crate) raddr: u64,
    pub(crate) rkey: Key,
    pub(crate) dqp_ip: Ipv4Addr,
    pub(crate) dqpn: Qpn,
    pub(crate) mac_addr: MacAddress,
    pub(crate) pmtu: Pmtu,
    pub(crate) flags: WorkReqSendFlag,
    pub(crate) qp_type: QpType,
    pub(crate) psn: Psn,
    pub(crate) msn: Msn,
}

impl Default for ToCardWorkRbDescCommon {
    fn default() -> Self {
        Self {
            total_len: 0,
            raddr: 0,
            rkey: Key::default(),
            dqp_ip: Ipv4Addr::new(0, 0, 0, 0),
            dqpn: Qpn::default(),
            mac_addr: MacAddress::default(),
            pmtu: Pmtu::Mtu256,
            flags: WorkReqSendFlag::empty(),
            qp_type: QpType::Rc,
            psn: Psn::default(),
            msn: Msn::default(),
        }
    }
}

#[derive(Default, Clone, Debug)]
pub(crate) struct ToCardWorkRbDescRead {
    pub(crate) common: ToCardWorkRbDescCommon,
    pub(crate) sge: DescSge,
}

#[derive(Default, Clone, Debug)]
pub(crate) struct ToCardWorkRbDescWrite {
    pub(crate) common: ToCardWorkRbDescCommon,
    pub(crate) is_last: bool,
    pub(crate) is_first: bool,
    pub(crate) sge0: DescSge,
    pub(crate) sge1: Option<DescSge>,
    pub(crate) sge2: Option<DescSge>,
    pub(crate) sge3: Option<DescSge>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ToCardWorkRbDescWriteWithImm {
    pub(crate) common: ToCardWorkRbDescCommon,
    pub(crate) is_last: bool,
    pub(crate) is_first: bool,
    pub(crate) imm: u32,
    pub(crate) sge0: DescSge,
    pub(crate) sge1: Option<DescSge>,
    pub(crate) sge2: Option<DescSge>,
    pub(crate) sge3: Option<DescSge>,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct ToHostWorkRbDescCommon {
    pub(crate) status: ToHostWorkRbDescStatus,
    pub(crate) trans: ToHostWorkRbDescTransType,
    pub(crate) dqpn: Qpn,
    pub(crate) msn: Msn,
    pub(crate) expected_psn: Psn,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct ToHostWorkRbDescRead {
    pub(crate) common: ToHostWorkRbDescCommon,
    pub(crate) len: u32,
    pub(crate) laddr: u64,
    pub(crate) lkey: Key,
    pub(crate) raddr: u64,
    pub(crate) rkey: Key,
}

#[derive(Debug, Clone)]
pub(crate) struct ToHostWorkRbDescWriteOrReadResp {
    pub(crate) common: ToHostWorkRbDescCommon,
    pub(crate) is_read_resp: bool,
    pub(crate) write_type: ToHostWorkRbDescWriteType,
    pub(crate) psn: Psn,
    pub(crate) addr: u64,
    pub(crate) len: u32,
    pub(crate) can_auto_ack: bool,
}

impl Default for ToHostWorkRbDescWriteOrReadResp {
    fn default() -> Self {
        Self {
            common: ToHostWorkRbDescCommon::default(),
            is_read_resp: false,
            write_type: ToHostWorkRbDescWriteType::Only,
            psn: Psn::default(),
            addr: 0,
            len: 0,
            can_auto_ack: false,
        }
    }
}

#[allow(unused)] // Currently we don't have write imm descriptor
#[derive(Debug)]
pub(crate) struct ToHostWorkRbDescWriteWithImm {
    pub(crate) common: ToHostWorkRbDescCommon,
    pub(crate) write_type: ToHostWorkRbDescWriteType,
    pub(crate) psn: Psn,
    pub(crate) imm: u32,
    pub(crate) addr: u64,
    pub(crate) len: u32,
    pub(crate) key: Key,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct ToHostWorkRbDescAck {
    pub(crate) common: ToHostWorkRbDescCommon,
    #[allow(unused)]
    pub(crate) msn: Msn,
    pub(crate) psn: Psn,
    pub(crate) retry_psn: Psn,
    pub(crate) code: ToHostWorkRbDescAethCode,
    #[allow(unused)] // used in nack checking
    pub(crate) value: u8,
}

#[derive(Debug, Default)]
pub(crate) struct ToHostWorkRbDescRaw {
    pub(crate) common: ToHostWorkRbDescCommon,
    pub(crate) addr: u64,
    pub(crate) len: u32,
    pub(crate) key: Key,
}

#[derive(Default, Debug, Clone, Copy)]
pub(crate) struct DescSge {
    pub(crate) addr: u64,
    pub(crate) len: u32,
    pub(crate) key: Key,
}

impl From<Sge> for DescSge {
    fn from(sge: Sge) -> Self {
        Self {
            addr: sge.addr,
            len: sge.len,
            key: sge.key,
        }
    }
}

#[derive(TryFromPrimitive, Debug, Clone)]
#[repr(u8)]
pub(crate) enum ToHostWorkRbDescStatus {
    Normal = 1,
    InvAccFlag = 2,
    InvOpcode = 3,
    InvMrKey = 4,
    InvMrRegion = 5,
    Unknown = 6,
}

impl Default for ToHostWorkRbDescStatus {
    fn default() -> Self {
        Self::Normal
    }
}

impl ToHostWorkRbDescStatus {
    pub(crate) fn is_ok(&self) -> bool {
        matches!(self, ToHostWorkRbDescStatus::Normal)
    }
}

#[derive(TryFromPrimitive, Debug, Clone, Copy)]
#[repr(u8)]
pub(crate) enum ToHostWorkRbDescTransType {
    Rc = 0x00,
    Uc = 0x01,
    Rd = 0x02,
    Ud = 0x03,
    Cnp = 0x04,
    Xrc = 0x05,
    DtldExtended = 0x06, // Customize for normal packet.
}

impl Default for ToHostWorkRbDescTransType {
    fn default() -> Self {
        Self::Rc
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ToHostWorkRbDescWriteType {
    First,
    Middle,
    Last,
    Only,
}

pub(crate) struct IncompleteToHostWorkRbDesc {
    parsed: ToHostWorkRbDesc,
    #[allow(unused)] // we reserve this in case that we will receive more than 2 descriptors.
    parsed_cnt: usize,
}

pub(crate) enum ToHostWorkRbDescError {
    Incomplete(IncompleteToHostWorkRbDesc),
    DeviceError(DeviceError),
}

#[derive(Debug, TryFromPrimitive)]
#[repr(u8)]
pub(crate) enum CtrlRbDescOpcode {
    UpdateMrTable = 0x00,
    UpdatePageTable = 0x01,
    QpManagement = 0x02,
    SetNetworkParam = 0x03,
    SetRawPacketReceiveMeta = 0x04,
    UpdateErrorPsnRecoverPoint = 0x05,
}

#[derive(Debug, Clone)]
pub(crate) enum ToCardWorkRbDescOpcode {
    // IBV_WR_RDMA_WRITE           =  0,
    // IBV_WR_RDMA_WRITE_WITH_IMM  =  1,
    // IBV_WR_SEND                 =  2,
    // IBV_WR_SEND_WITH_IMM        =  3,
    // IBV_WR_RDMA_READ            =  4,
    // IBV_WR_ATOMIC_CMP_AND_SWP   =  5,
    // IBV_WR_ATOMIC_FETCH_AND_ADD =  6,
    // IBV_WR_LOCAL_INV            =  7,
    // IBV_WR_BIND_MW              =  8,
    // IBV_WR_SEND_WITH_INV        =  9,
    // IBV_WR_TSO                  = 10,
    // IBV_WR_DRIVER1              = 11,
    // IBV_WR_RDMA_READ_RESP       = 12, // Not defined in rdma-core
    // IBV_WR_FLUSH                = 14,
    // IBV_WR_ATOMIC_WRITE         = 15
    Write = 0,
    WriteWithImm = 1,
    Read = 4,
    ReadResp = 12, // Not defined in rdma-core
}

#[derive(TryFromPrimitive, PartialEq, Eq, Debug, Clone)]
#[repr(u8)]
pub(crate) enum ToHostWorkRbDescOpcode {
    // SendFirst = 0x00,
    // SendMiddle = 0x01,
    // SendLast = 0x02,
    // SendLastWithImmediate = 0x03,
    // SendOnly = 0x04,
    // SendOnlyWithImmediate = 0x05,
    // RdmaWriteFirst = 0x06,
    // RdmaWriteMiddle = 0x07,
    // RdmaWriteLast = 0x08,
    // RdmaWriteLastWithImmediate = 0x09,
    // RdmaWriteOnly = 0x0a,
    // RdmaWriteOnlyWithImmediate = 0x0b,
    // RdmaReadRequest = 0x0c,
    // Acknowledge = 0x11,
    // AtomicAcknowledge = 0x12,
    // CompareSwap = 0x13,
    // FetchAdd = 0x14,
    // Resync = 0x15,
    // SendLastWithInvalidate = 0x16,
    // SendOnlyWithInvalidate = 0x17,
    RdmaWriteFirst = 0x06,
    RdmaWriteMiddle = 0x07,
    RdmaWriteLast = 0x08,
    RdmaWriteLastWithImmediate = 0x09,
    RdmaWriteOnly = 0x0a,
    RdmaWriteOnlyWithImmediate = 0x0b,
    RdmaReadResponseFirst = 0x0d,
    RdmaReadResponseMiddle = 0x0e,
    RdmaReadResponseLast = 0x0f,
    RdmaReadResponseOnly = 0x10,
    RdmaReadRequest = 0x0c,
    Acknowledge = 0x11,
}

impl ToHostWorkRbDescOpcode {
    pub(crate) fn is_first(&self) -> bool {
        match self {
            ToHostWorkRbDescOpcode::RdmaWriteFirst
            | ToHostWorkRbDescOpcode::RdmaReadResponseFirst => true,
            ToHostWorkRbDescOpcode::RdmaWriteMiddle
            | ToHostWorkRbDescOpcode::RdmaWriteLast
            | ToHostWorkRbDescOpcode::RdmaWriteLastWithImmediate
            | ToHostWorkRbDescOpcode::RdmaWriteOnly
            | ToHostWorkRbDescOpcode::RdmaWriteOnlyWithImmediate
            | ToHostWorkRbDescOpcode::RdmaReadResponseMiddle
            | ToHostWorkRbDescOpcode::RdmaReadResponseLast
            | ToHostWorkRbDescOpcode::RdmaReadResponseOnly
            | ToHostWorkRbDescOpcode::RdmaReadRequest
            | ToHostWorkRbDescOpcode::Acknowledge => false,
        }
    }

    pub(crate) fn is_resp(&self) -> bool {
        matches!(
            self,
            ToHostWorkRbDescOpcode::RdmaReadResponseFirst
                | ToHostWorkRbDescOpcode::RdmaReadResponseMiddle
                | ToHostWorkRbDescOpcode::RdmaReadResponseLast
                | ToHostWorkRbDescOpcode::RdmaReadResponseOnly
        )
    }

    pub(crate) fn is_read_resp(&self) -> bool {
        matches!(
            self,
            ToHostWorkRbDescOpcode::RdmaReadResponseFirst
                | ToHostWorkRbDescOpcode::RdmaReadResponseMiddle
                | ToHostWorkRbDescOpcode::RdmaReadResponseLast
                | ToHostWorkRbDescOpcode::RdmaReadResponseOnly
        )
    }

    pub(crate) fn write_type(&self) -> Option<ToHostWorkRbDescWriteType> {
        match self {
            ToHostWorkRbDescOpcode::RdmaWriteFirst
            | ToHostWorkRbDescOpcode::RdmaReadResponseFirst => {
                Some(ToHostWorkRbDescWriteType::First)
            }
            ToHostWorkRbDescOpcode::RdmaWriteMiddle
            | ToHostWorkRbDescOpcode::RdmaReadResponseMiddle => {
                Some(ToHostWorkRbDescWriteType::Middle)
            }
            ToHostWorkRbDescOpcode::RdmaWriteLast
            | ToHostWorkRbDescOpcode::RdmaWriteLastWithImmediate
            | ToHostWorkRbDescOpcode::RdmaReadResponseLast => Some(ToHostWorkRbDescWriteType::Last),
            ToHostWorkRbDescOpcode::RdmaWriteOnlyWithImmediate
            | ToHostWorkRbDescOpcode::RdmaWriteOnly
            | ToHostWorkRbDescOpcode::RdmaReadResponseOnly => Some(ToHostWorkRbDescWriteType::Only),
            ToHostWorkRbDescOpcode::RdmaReadRequest | ToHostWorkRbDescOpcode::Acknowledge => None,
        }
    }
}

#[derive(TryFromPrimitive, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub(crate) enum ToHostWorkRbDescAethCode {
    // AETH_CODE_ACK  = 2'b00,
    // AETH_CODE_RNR  = 2'b01,
    // AETH_CODE_RSVD = 2'b10,
    // AETH_CODE_NAK  = 2'b11
    Ack = 0b00,
    Rnr = 0b01,
    Rsvd = 0b10,
    Nak = 0b11,
}

impl Default for ToHostWorkRbDescAethCode {
    fn default() -> Self {
        Self::Rsvd
    }
}

impl ToCardCtrlRbDesc {
    pub(super) fn write(&self, dst: &mut [u8]) {
        fn write_common_header(dst: &mut [u8], opcode: CtrlRbDescOpcode, op_id: u32) {
            // typedef struct {
            //     Bit#(32)                userData;
            //     ReservedZero#(20)       reserved1;
            //     Bit#(4)                 extraSegmentCnt;
            //     Bit#(6)                 opCode;
            //     Bool                    isSuccessOrNeedSignalCplt;
            //     Bool                    valid;
            // } CmdQueueDescCommonHead deriving(Bits, FShow);

            let mut common = CmdQueueDescCommonHead(dst);
            common.set_valid(true);
            common.set_reserverd(0);
            common.set_is_success_or_need_signal_cplt(false);
            common.set_op_code(opcode as u32);
            common.set_extra_segment_cnt(0);
            common.set_user_data(op_id);
        }

        fn write_update_mr_table(dst: &mut [u8], desc: &ToCardCtrlRbDescUpdateMrTable) {
            // typedef struct {
            //     ReservedZero#(7)            reserved1;
            //     Bit#(17)                    pgtOffset;
            //     Bit#(8)                     accFlags;
            //     Bit#(32)                    pdHandler;
            //     Bit#(32)                    mrKey;
            //     Bit#(32)                    mrLength;
            //     Bit#(64)                    mrBaseVA;
            //     CmdQueueDescCommonHead      commonHeader;
            // } CmdQueueReqDescUpdateMrTable deriving(Bits, FShow);

            // bytes 0-7 are header bytes, ignore them

            let mut update_mr_table = CmdQueueReqDescUpdateMrTable(dst);
            update_mr_table.set_mr_base_va(desc.addr);
            update_mr_table.set_mr_length(desc.len.into());
            update_mr_table.set_mr_key(desc.key.get().into());
            update_mr_table.set_pd_handler(desc.pd_hdl.into());
            update_mr_table.set_acc_flags(desc.acc_flags.bits().into());
            update_mr_table.set_pgt_offset(desc.pgt_offset.into());
        }

        fn write_update_page_table(dst: &mut [u8], desc: &ToCardCtrlRbDescUpdatePageTable) {
            // typedef struct {
            //     ReservedZero#(64)               reserved1;
            //     Bit#(32)                        dmaReadLength;
            //     Bit#(32)                        startIndex;
            //     Bit#(64)                        dmaAddr;
            //     CmdQueueDescCommonHead          commonHeader;
            // } CmdQueueReqDescUpdatePGT deriving(Bits, FShow);

            // bits 0-7 are header bits
            let mut update_pgt = CmdQueueReqDescUpdatePGT(dst);
            update_pgt.set_dma_addr(desc.start_addr);
            update_pgt.set_start_index(desc.pgt_idx.into());
            #[allow(clippy::arithmetic_side_effects)]
            // this will less than MR_PGT_SIZE * 8, which will not overflow
            update_pgt.set_dma_read_length((desc.pgte_cnt * 8).into());
        }

        fn write_qp_management(dst: &mut [u8], desc: &ToCardCtrlRbDescQpManagement) {
            // typedef struct {
            //     ReservedZero#(80)              reserved1;       // 80  bits
            //     QPN                             qpn;            // 24  bits
            //     ReservedZero#(5)                reserved2;      // 5   bits
            //     PMTU                            pmtu;           // 3   bits
            //     FlagsType#(MemAccessTypeFlag)   rqAccessFlags;  // 8   bits
            //     ReservedZero#(4)                reserved3;      // 4   bits
            //     TypeQP                          qpType;         // 4   bits
            //     HandlerPD                       pdHandler;      // 32  bits
            //     QPN                             qpn;            // 24  bits
            //     ReservedZero#(6)                reserved4;      // 6   bits
            //     Bool                            isError;        // 1   bit
            //     Bool                            isValid;        // 1   bit
            //     CmdQueueDescCommonHead          commonHeader;   // 64  bits
            // } CmdQueueReqDescQpManagementSeg0 deriving(Bits, FShow);

            let mut seg0 = CmdQueueReqDescQpManagementSeg0(dst);
            seg0.set_is_valid(desc.is_valid);
            seg0.set_is_error(false);
            seg0.set_qpn(desc.qpn.get().into());
            seg0.set_pd_handler(desc.pd_hdl.into());
            seg0.set_qp_type(desc.qp_type as u64);
            seg0.set_rq_access_flags(desc.rq_acc_flags.bits().into());
            seg0.set_pmtu(desc.pmtu as u64);
            seg0.set_peer_qpn(desc.peer_qpn.get().into());
        }

        fn write_set_network_param(dst: &mut [u8], desc: &ToCardCtrlRbDescSetNetworkParam) {
            let mut network_params = CmdQueueReqDescSetNetworkParam(dst);
            network_params.set_eth_mac_addr(u8_slice_to_u64(desc.macaddr.as_bytes()));
            network_params.set_ip_addr(u8_slice_to_u64(&desc.ipaddr.octets()));
            network_params.set_ip_gateway(u8_slice_to_u64(&desc.gateway.octets()));
            network_params.set_ip_netmask(u8_slice_to_u64(&desc.netmask.octets()));
        }

        fn write_set_raw_packet_receive_meta(
            dst: &mut [u8],
            desc: &ToCardCtrlRbDescSetRawPacketReceiveMeta,
        ) {
            let mut raw_packet_recv_meta = CmdQueueReqDescSetRawPacketReceiveMeta(dst);
            raw_packet_recv_meta.set_write_base_addr(desc.base_write_addr);
            raw_packet_recv_meta.set_write_mr_key(u64::from(desc.key.get()));
        }

        fn write_update_err_psn_recover_point(
            dst: &mut [u8],
            desc: &ToCardCtrlRbDescUpdateErrPsnRecoverPoint,
        ) {
            let mut raw_packet_recv_meta = CmdQueueReqDescUpdateErrRecoverPoint(dst);
            raw_packet_recv_meta.set_qpn(desc.qpn.get());
            raw_packet_recv_meta.set_psn(desc.recover_psn.get());
        }

        match self {
            ToCardCtrlRbDesc::UpdateMrTable(desc) => {
                write_common_header(dst, CtrlRbDescOpcode::UpdateMrTable, desc.common.op_id);
                write_update_mr_table(dst, desc);
            }
            ToCardCtrlRbDesc::UpdatePageTable(desc) => {
                write_common_header(dst, CtrlRbDescOpcode::UpdatePageTable, desc.common.op_id);
                write_update_page_table(dst, desc);
            }
            ToCardCtrlRbDesc::QpManagement(desc) => {
                write_common_header(dst, CtrlRbDescOpcode::QpManagement, desc.common.op_id);
                write_qp_management(dst, desc);
            }
            ToCardCtrlRbDesc::SetNetworkParam(desc) => {
                write_common_header(dst, CtrlRbDescOpcode::SetNetworkParam, desc.common.op_id);
                write_set_network_param(dst, desc);
            }
            ToCardCtrlRbDesc::SetRawPacketReceiveMeta(desc) => {
                write_common_header(
                    dst,
                    CtrlRbDescOpcode::SetRawPacketReceiveMeta,
                    desc.common.op_id,
                );
                write_set_raw_packet_receive_meta(dst, desc);
            }
            ToCardCtrlRbDesc::UpdateErrorPsnRecoverPoint(desc) => {
                write_common_header(
                    dst,
                    CtrlRbDescOpcode::UpdateErrorPsnRecoverPoint,
                    desc.common.op_id,
                );
                write_update_err_psn_recover_point(dst, desc);
            }
        }
    }
}

impl ToHostCtrlRbDesc {
    pub(super) fn read(src: &[u8]) -> Result<ToHostCtrlRbDesc, DeviceError> {
        // typedef struct {
        //     Bit#(32)                userData;
        //     ReservedZero#(20)       reserved1;
        //     Bit#(4)                 extraSegmentCnt;
        //     Bit#(6)                 opCode;
        //     Bool                    isSuccessOrNeedSignalCplt;
        //     Bool                    valid;
        // } CmdQueueDescCommonHead deriving(Bits, FShow);
        let head = CmdQueueDescCommonHead(src);

        let valid = head.get_valid();
        assert!(valid, "Invalid CmdQueueDescCommonHead");

        let extra_segment_cnt = head.get_extra_segment_cnt();
        assert!(
            extra_segment_cnt == 0,
            "extra_segment_cnt: {extra_segment_cnt}"
        );

        let is_success = head.get_is_success_or_need_signal_cplt();
        // bitfield restricts the field is not longer than 8bits.
        // So we can safely cast it to u8.
        #[allow(clippy::cast_possible_truncation)]
        let opcode_raw = head.get_op_code() as u8;

        let opcode = CtrlRbDescOpcode::try_from(opcode_raw).map_err(|_| {
            DeviceError::ParseDesc(format!("CtrlRbDescOpcode = {opcode_raw} can not be parsed"))
        })?;
        let op_id = head.get_user_data().to_le();

        let common = ToHostCtrlRbDescCommon {
            op_id,
            opcode,
            is_success,
        };

        let desc = ToHostCtrlRbDesc { common };
        Ok(desc)
    }
}

impl ToCardWorkRbDesc {
    pub(super) fn write_0(&self, dst: &mut [u8]) {
        let (common, opcode, is_first, is_last) = match self {
            ToCardWorkRbDesc::Read(desc) => {
                (&desc.common, ToCardWorkRbDescOpcode::Read, true, true)
            }
            ToCardWorkRbDesc::Write(desc) => (
                &desc.common,
                ToCardWorkRbDescOpcode::Write,
                desc.is_first,
                desc.is_last,
            ),
            ToCardWorkRbDesc::WriteWithImm(desc) => (
                &desc.common,
                ToCardWorkRbDescOpcode::WriteWithImm,
                desc.is_first,
                desc.is_last,
            ),
            ToCardWorkRbDesc::ReadResp(desc) => (
                &desc.common,
                ToCardWorkRbDescOpcode::ReadResp,
                desc.is_first,
                desc.is_last,
            ),
        };

        let mut head = SendQueueDescCommonHead(dst);
        head.set_valid(true);
        head.set_is_success_or_need_signal_cplt(false);
        head.set_is_first(is_first);
        head.set_is_last(is_last);
        head.set_op_code(opcode as u32);

        #[allow(clippy::arithmetic_side_effects)]
        // self.serialized_desc_cnt() always greater than 1
        let extra_segment_cnt = self.serialized_desc_cnt() - 1;
        head.set_extra_segment_cnt(extra_segment_cnt);
        head.set_total_len(common.total_len);

        // typedef struct {
        //     ReservedZero#(64)           reserved1;        // 64 bits
        //     AddrIPv4                    dqpIP;            // 32 bits
        //     RKEY                        rkey;             // 32 bits
        //     ADDR                        raddr;            // 64 bits
        //     SendQueueDescCommonHead     commonHeader;     // 64 bits
        // } SendQueueReqDescSeg0 deriving(Bits, FShow);
        // let mut seg0 = SendQueueReqDescSeg0(&mut dst[8..]);
        let mut head = SendQueueReqDescSeg0(&mut head.0);
        head.set_raddr(common.raddr);
        head.set_rkey(common.rkey.get().into());
        head.set_dqp_ip(u8_slice_to_u64(&common.dqp_ip.octets()));
        // We use the pkey field to store the `MSN`.
        head.set_pkey(common.msn.get().into());
    }

    pub(super) fn write_1(&self, dst: &mut [u8]) {
        // typedef struct {
        //     ReservedZero#(64)       reserved1;          // 64 bits

        //     IMM                     imm;                // 32 bits

        //     ReservedZero#(8)        reserved2;          // 8  bits
        //     QPN                     dqpn;               // 24 bits

        //     ReservedZero#(16)       reserved3;          // 16 bits
        //     MAC                     macAddr;            // 48 bits

        //     ReservedZero#(8)        reserved4;          // 8  bits
        //     PSN                     psn;                // 24 bits

        //     ReservedZero#(5)        reserved5;          // 5  bits
        //     NumSGE                  sgeCnt;             // 3  bits

        //     ReservedZero#(4)        reserved6;          // 4  bits
        //     TypeQP                  qpType;             // 4  bits

        //     ReservedZero#(3)        reserved7;          // 3  bits
        //     WorkReqSendFlag         flags;              // 5  bits

        //     ReservedZero#(5)        reserved8;          // 5  bits
        //     PMTU                    pmtu;               // 3  bits
        // } SendQueueReqDescSeg1 deriving(Bits, FShow);

        #[allow(clippy::arithmetic_side_effects)]
        let (common, sge_cnt) = match self {
            ToCardWorkRbDesc::Read(desc) => (&desc.common, 1),
            ToCardWorkRbDesc::Write(desc) | ToCardWorkRbDesc::ReadResp(desc) => (
                &desc.common,
                1 + u8::from(desc.sge1.is_some())
                    + u8::from(desc.sge2.is_some())
                    + u8::from(desc.sge3.is_some()),
            ),
            ToCardWorkRbDesc::WriteWithImm(desc) => (
                &desc.common,
                1 + u8::from(desc.sge1.is_some())
                    + u8::from(desc.sge2.is_some())
                    + u8::from(desc.sge3.is_some()),
            ),
        };
        let mut desc_common = SendQueueReqDescSeg1(dst);
        desc_common.set_pmtu(common.pmtu as u64);
        desc_common.set_flags(u64::from(common.flags.bits()));
        desc_common.set_qp_type(common.qp_type as u64);
        desc_common.set_seg_cnt(sge_cnt.into());
        desc_common.set_psn(common.psn.get().into());
        desc_common.set_mac_addr(u8_slice_to_u64(common.mac_addr.as_bytes()));

        desc_common.set_dqpn(common.dqpn.get().into());

        if let ToCardWorkRbDesc::WriteWithImm(desc) = self {
            desc_common.set_imm(u64::from(desc.imm));
        } else {
            desc_common.set_imm(0);
        }
    }

    #[allow(clippy::indexing_slicing)]
    pub(super) fn write_2(&self, dst: &mut [u8]) {
        // typedef struct {
        //     ADDR   laddr;         // 64 bits
        //     Length len;           // 32 bits
        //     LKEY   lkey;          // 32 bits
        // } SendQueueReqDescFragSGE deriving(Bits, FShow);

        // typedef struct {
        //     SendQueueReqDescFragSGE     sge1;       // 128 bits
        //     SendQueueReqDescFragSGE     sge2;       // 128 bits
        // } SendQueueReqDescVariableLenSGE deriving(Bits, FShow);

        let (sge0, sge1) = match self {
            ToCardWorkRbDesc::Read(desc) => (&desc.sge, None),
            ToCardWorkRbDesc::Write(desc) | ToCardWorkRbDesc::ReadResp(desc) => {
                (&desc.sge0, desc.sge1.as_ref())
            }
            ToCardWorkRbDesc::WriteWithImm(desc) => (&desc.sge0, desc.sge1.as_ref()),
        };
        // Note that the order of the sges is reversed in the struct
        let mut frag_sge = SendQueueReqDescFragSGE(&mut dst[16..32]);
        frag_sge.set_laddr(sge0.addr);
        frag_sge.set_len(sge0.len.into());
        frag_sge.set_lkey(sge0.key.get().into());

        let mut frag_sge2 = SendQueueReqDescFragSGE(&mut dst[0..16]);
        if let Some(sge1) = sge1 {
            frag_sge2.set_laddr(sge1.addr);
            frag_sge2.set_len(sge1.len.into());
            frag_sge2.set_lkey(sge1.key.get().into());
        } else {
            dst[0..16].copy_from_slice(&[0; 16]);
        }
    }

    #[allow(clippy::indexing_slicing)]
    pub(super) fn write_3(&self, dst: &mut [u8]) {
        // typedef struct {
        //     ADDR   laddr;         // 64 bits
        //     Length len;           // 32 bits
        //     LKEY   lkey;          // 32 bits
        // } SendQueueReqDescFragSGE deriving(Bits, FShow);

        // typedef struct {
        //     SendQueueReqDescFragSGE     sge1;       // 128 bits
        //     SendQueueReqDescFragSGE     sge2;       // 128 bits
        // } SendQueueReqDescVariableLenSGE deriving(Bits, FShow);

        let (sge2, sge3) = match self {
            ToCardWorkRbDesc::Read(_) => (None, None),
            ToCardWorkRbDesc::Write(desc) | ToCardWorkRbDesc::ReadResp(desc) => {
                (desc.sge2.as_ref(), desc.sge3.as_ref())
            }
            ToCardWorkRbDesc::WriteWithImm(desc) => (desc.sge2.as_ref(), desc.sge3.as_ref()),
        };

        let mut frag_sge = SendQueueReqDescFragSGE(&mut dst[0..16]);
        if let Some(sge3) = sge3 {
            frag_sge.set_lkey(sge3.key.get().into());
            frag_sge.set_len(sge3.len.into());
            frag_sge.set_laddr(sge3.addr);
        } else {
            frag_sge.set_lkey(0);
            frag_sge.set_len(0);
            frag_sge.set_laddr(0);
        }

        let mut frag_sge2 = SendQueueReqDescFragSGE(&mut dst[16..32]);
        if let Some(sge2) = sge2 {
            frag_sge2.set_lkey(sge2.key.get().into());
            frag_sge2.set_len(sge2.len.into());
            frag_sge2.set_laddr(sge2.addr);
        } else {
            frag_sge2.set_lkey(0);
            frag_sge2.set_len(0);
            frag_sge2.set_laddr(0);
        }
    }

    #[allow(clippy::arithmetic_side_effects)]
    pub(super) fn serialized_desc_cnt(&self) -> u32 {
        let sge_desc_cnt = match self {
            ToCardWorkRbDesc::Read(_) => 1,
            ToCardWorkRbDesc::Write(desc) | ToCardWorkRbDesc::ReadResp(desc) => {
                1 + u32::from(desc.sge2.is_some())
            }
            ToCardWorkRbDesc::WriteWithImm(desc) => 1 + u32::from(desc.sge2.is_some()),
        };

        2 + sge_desc_cnt
    }
}

impl ToHostWorkRbDesc {
    /// (addr, key, len)
    fn read_reth(src: &[u8]) -> (u64, Key, u32) {
        // typedef struct {
        //     Length                  dlen;         // 32
        //     RKEY                    rkey;         // 32
        //     ADDR                    va;           // 64
        // } MetaReportQueueDescFragRETH deriving(Bits, FShow);

        // first 12 bytes are desc type, status and bth
        #[allow(clippy::indexing_slicing)]
        let frag_reth = MetaReportQueueDescFragRETH(&src[12..]);
        let addr = frag_reth.get_va();
        // bitfield restricts the field is not longer than 32 bits.
        #[allow(clippy::cast_possible_truncation)]
        let key = Key::new(frag_reth.get_rkey() as u32);
        #[allow(clippy::cast_possible_truncation)]
        let len = frag_reth.get_dlen() as u32;

        (addr, key, len)
    }

    // FIXME: imm will be in next desc
    fn read_imm(src: &[u8]) -> u32 {
        // typedef struct {
        //     IMM                             data;           // 32
        // } MetaReportQueueDescFragImmDT deriving(Bits, FShow);

        // first 28 bytes are desc type, status, bth and reth
        #[allow(clippy::indexing_slicing)]
        let imm = MetaReportQueueDescFragImmDT(&src[28..32]);
        // call the `to_be` to convert order
        imm.get_imm()
    }

    // (last_psn, msn, value, code)
    #[allow(clippy::cast_possible_truncation)]
    fn read_aeth(src: &[u8]) -> Result<(Psn, Msn, u8, ToHostWorkRbDescAethCode), DeviceError> {
        // typedef struct {
        //     ReservedZero#(9)        reserved1;    // 9
        //     AethCode                code;         // 2
        //     AethValue               value;        // 5
        //     MSN                     msn;          // 24
        //     PSN                     lastRetryPSN; // 24
        // } MetaReportQueueDescFragAETH deriving(Bits, FShow);

        // first 12 bytes are desc type, status and bth
        #[allow(clippy::indexing_slicing)]
        let frag_aeth = MetaReportQueueDescFragAETH(&src[12..]);
        let psn = Psn::new(frag_aeth.get_psn());
        let msg_seq_number = Msn::new(frag_aeth.get_msn() as u16);
        let value = frag_aeth.get_aeth_value() as u8;
        let code = frag_aeth.get_aeth_code() as u8;
        let code = ToHostWorkRbDescAethCode::try_from(code).map_err(|_| {
            DeviceError::ParseDesc(format!(
                "ToHostWorkRbDescAethCode = {code} can not be parsed"
            ))
        })?;

        Ok((psn, msg_seq_number, value, code))
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::indexing_slicing,
        clippy::too_many_lines
    )]
    pub(super) fn read(src: &mut [u8]) -> Result<ToHostWorkRbDesc, ToHostWorkRbDescError> {
        // typedef struct {
        //     ReservedZero#(8)                reserved1;      // 8
        //     MSN                             msn;            // 24
        //     MetaReportQueueDescFragRETH     reth;           // 128
        //     MetaReportQueueDescFragBTH      bth;            // 64
        //     RdmaReqStatus                   reqStatus;      // 8
        //     PSN                             expectedPSN;    // 24
        // } MetaReportQueueDescBthRethReth deriving(Bits, FShow);
        let mut desc_bth = MetaReportQueueDescBthReth(&mut src[0..32]);

        let expected_psn = Psn::new(desc_bth.get_expected_psn() as u32);
        let mut raw_status = desc_bth.get_req_status() as u8;
        let start_time = Instant::now();
        while raw_status == 0 {
            raw_status = desc_bth.get_req_status() as u8;
            if start_time.elapsed().as_millis() > 1000 {
                log::error!("timeout in parsing status");
                break;
            }
        }
        desc_bth.set_req_status(0);
        let status = ToHostWorkRbDescStatus::try_from(raw_status).map_err(|_| {
            ToHostWorkRbDescError::DeviceError(DeviceError::ParseDesc(format!(
                "ToHostWorkRbDescStatus = {} can not be parsed",
                desc_bth.get_req_status()
            )))
        })?;
        let msg_seq_number = Msn::new(desc_bth.get_msn() as u16);
        let can_auto_ack = desc_bth.get_can_auto_ack();

        // typedef struct {
        //     ReservedZero#(4)                reserved1;    // 4
        //     PAD                             padCnt;       // 2
        //     Bool                            ackReq;       // 1
        //     Bool                            solicited;    // 1
        //     PSN                             psn;          // 24
        //     QPN                             dqpn;         // 24
        //     RdmaOpCode                      opcode;       // 5
        //     TransType                       trans;        // 3
        // } MetaReportQueueDescFragBTH deriving(Bits, FShow);

        let desc_frag_bth = MetaReportQueueDescFragBTH(&src[4..12]);
        let trans = ToHostWorkRbDescTransType::try_from(desc_frag_bth.get_trans_type() as u8)
            .map_err(|_| {
                ToHostWorkRbDescError::DeviceError(DeviceError::ParseDesc(format!(
                    "ToHostWorkRbDescTransType = {} can not be parsed",
                    desc_frag_bth.get_trans_type()
                )))
            })?;
        let opcode =
            ToHostWorkRbDescOpcode::try_from(desc_frag_bth.get_opcode() as u8).map_err(|_| {
                ToHostWorkRbDescError::DeviceError(DeviceError::ParseDesc(format!(
                    "ToHostWorkRbDescOpcode = {} can not be parsed",
                    desc_frag_bth.get_opcode()
                )))
            })?;
        let dqpn = Qpn::new(desc_frag_bth.get_qpn());
        let psn = Psn::new(desc_frag_bth.get_psn());

        let common = ToHostWorkRbDescCommon {
            status,
            trans,
            dqpn,
            msn: msg_seq_number,
            expected_psn,
        };
        let is_read_resp = opcode.is_read_resp();
        // The default value will not be used since the `write_type` will only appear
        // in those write related opcodes.
        let write_type = opcode
            .write_type()
            .unwrap_or(ToHostWorkRbDescWriteType::Only);
        match opcode {
            ToHostWorkRbDescOpcode::RdmaWriteFirst
            | ToHostWorkRbDescOpcode::RdmaWriteMiddle
            | ToHostWorkRbDescOpcode::RdmaWriteLast
            | ToHostWorkRbDescOpcode::RdmaWriteOnly
            | ToHostWorkRbDescOpcode::RdmaReadResponseFirst
            | ToHostWorkRbDescOpcode::RdmaReadResponseMiddle
            | ToHostWorkRbDescOpcode::RdmaReadResponseLast
            | ToHostWorkRbDescOpcode::RdmaReadResponseOnly => {
                let (addr, _, len) = Self::read_reth(src);
                Ok(ToHostWorkRbDesc::WriteOrReadResp(
                    ToHostWorkRbDescWriteOrReadResp {
                        common,
                        is_read_resp,
                        write_type,
                        psn,
                        addr,
                        len,
                        can_auto_ack,
                    },
                ))
            }
            ToHostWorkRbDescOpcode::RdmaWriteLastWithImmediate
            | ToHostWorkRbDescOpcode::RdmaWriteOnlyWithImmediate => {
                let (addr, key, len) = Self::read_reth(src);
                if matches!(common.trans, ToHostWorkRbDescTransType::DtldExtended) {
                    Err(ToHostWorkRbDescError::Incomplete(
                        IncompleteToHostWorkRbDesc {
                            parsed: ToHostWorkRbDesc::Raw(ToHostWorkRbDescRaw {
                                common,
                                addr,
                                len,
                                key,
                            }),
                            parsed_cnt: 1,
                        },
                    ))
                } else {
                    let imm = Self::read_imm(src);
                    Ok(ToHostWorkRbDesc::WriteWithImm(
                        ToHostWorkRbDescWriteWithImm {
                            common,
                            write_type,
                            psn,
                            imm,
                            addr,
                            len,
                            key,
                        },
                    ))
                }
            }
            ToHostWorkRbDescOpcode::RdmaReadRequest => {
                let (addr, key, len) = Self::read_reth(src);

                Err(ToHostWorkRbDescError::Incomplete(
                    IncompleteToHostWorkRbDesc {
                        parsed: ToHostWorkRbDesc::Read(ToHostWorkRbDescRead {
                            common,
                            len,
                            laddr: addr,
                            lkey: key,
                            raddr: 0,
                            rkey: Key::default(),
                        }),
                        parsed_cnt: 1,
                    },
                ))
            }
            ToHostWorkRbDescOpcode::Acknowledge => {
                let (retry_psn, msn_in_ack, value, code) =
                    Self::read_aeth(src).map_err(ToHostWorkRbDescError::DeviceError)?;
                Ok(ToHostWorkRbDesc::Ack(ToHostWorkRbDescAck {
                    common,
                    msn: msn_in_ack,
                    value,
                    psn,
                    code,
                    retry_psn
                }))
            }
        }
    }
}

impl IncompleteToHostWorkRbDesc {
    #[allow(clippy::cast_possible_truncation)]
    pub(super) fn read(self, src: &[u8]) -> Result<ToHostWorkRbDesc, ToHostWorkRbDescError> {
        fn read_second_reth(src: &[u8]) -> (u64, Key) {
            // typedef struct {
            //     RKEY                            secondaryRkey;   // 32
            //     ADDR                            secondaryVa;     // 64
            // } MetaReportQueueDescFragSecondaryRETH deriving(Bits, FShow);
            let secondary_reth = MetaReportQueueDescFragSecondaryRETH(&src);
            let addr = secondary_reth.get_secondary_va();
            let key = Key::new(secondary_reth.get_secondary_rkey() as u32);

            (addr, key)
        }

        match self.parsed {
            ToHostWorkRbDesc::Read(mut desc) => {
                let (raddr, rkey) = read_second_reth(src);
                desc.raddr = raddr;
                desc.rkey = rkey;
                Ok(ToHostWorkRbDesc::Read(desc))
            }
            ToHostWorkRbDesc::Raw(desc) => Ok(ToHostWorkRbDesc::Raw(desc)), // ignore the redundant imm
            ToHostWorkRbDesc::WriteOrReadResp(_)
            | ToHostWorkRbDesc::WriteWithImm(_)
            | ToHostWorkRbDesc::Ack(_) => unreachable!(),
        }
    }
}

pub(crate) struct ToCardWorkRbDescBuilder {
    type_: ToCardWorkRbDescOpcode,
    common: Option<ToCardWorkRbDescCommon>,
    seg_list: Vec<Sge>,
    imm: Option<u32>,
}

impl ToCardWorkRbDescBuilder {
    pub(crate) fn new(type_: ToCardWorkRbDescOpcode) -> Self {
        Self {
            type_,
            common: None,
            seg_list: Vec::new(),
            imm: None,
        }
    }

    pub(crate) fn with_common(mut self, common: ToCardWorkRbDescCommon) -> Self {
        self.common = Some(common);
        self
    }

    pub(crate) fn with_sge(mut self, seg: Sge) -> Self {
        self.seg_list.push(seg);
        self
    }

    pub(crate) fn with_imm(mut self, imm: Imm) -> Self {
        self.imm = Some(imm.get());
        self
    }

    pub(crate) fn build(mut self) -> Result<Box<ToCardWorkRbDesc>, Error> {
        let common = self
            .common
            .ok_or_else(|| Error::BuildDescFailed("common"))?;
        let desc = match self.type_ {
            ToCardWorkRbDescOpcode::Write => {
                let sge0 = self
                    .seg_list
                    .pop()
                    .ok_or_else(|| Error::BuildDescFailed("sge"))?;
                let sge1 = self.seg_list.pop();
                let sge2 = self.seg_list.pop();
                let sge3 = self.seg_list.pop();
                ToCardWorkRbDesc::Write(ToCardWorkRbDescWrite {
                    common,
                    is_last: true,
                    is_first: true,
                    sge0: sge0.into(),
                    sge1: sge1.map(bitfield::Into::into),
                    sge2: sge2.map(bitfield::Into::into),
                    sge3: sge3.map(bitfield::Into::into),
                })
            }
            ToCardWorkRbDescOpcode::WriteWithImm => {
                let sge0 = self
                    .seg_list
                    .pop()
                    .ok_or_else(|| Error::BuildDescFailed("sge"))?;
                let sge1 = self.seg_list.pop();
                let sge2 = self.seg_list.pop();
                let sge3 = self.seg_list.pop();
                let imm = self.imm.ok_or_else(|| Error::BuildDescFailed("imm"))?;
                ToCardWorkRbDesc::WriteWithImm(ToCardWorkRbDescWriteWithImm {
                    common,
                    is_last: true,
                    is_first: true,
                    imm,
                    sge0: sge0.into(),
                    sge1: sge1.map(bitfield::Into::into),
                    sge2: sge2.map(bitfield::Into::into),
                    sge3: sge3.map(bitfield::Into::into),
                })
            }
            ToCardWorkRbDescOpcode::Read => {
                let sge0 = self
                    .seg_list
                    .pop()
                    .ok_or_else(|| Error::BuildDescFailed("sge"))?;
                ToCardWorkRbDesc::Read(ToCardWorkRbDescRead {
                    common,
                    sge: sge0.into(),
                })
            }
            ToCardWorkRbDescOpcode::ReadResp => {
                let sge0 = self
                    .seg_list
                    .pop()
                    .ok_or_else(|| Error::BuildDescFailed("sge"))?;
                let sge1 = self.seg_list.pop();
                let sge2 = self.seg_list.pop();
                let sge3 = self.seg_list.pop();
                ToCardWorkRbDesc::ReadResp(ToCardWorkRbDescWrite {
                    common,
                    is_last: true,
                    is_first: true,
                    sge0: sge0.into(),
                    sge1: sge1.map(bitfield::Into::into),
                    sge2: sge2.map(bitfield::Into::into),
                    sge3: sge3.map(bitfield::Into::into),
                })
            }
        };
        Ok(Box::new(desc))
    }
}

#[derive(Debug, Error)]
pub(crate) enum DeviceError {
    #[error("device error : {0}")]
    Device(String),
    #[error("Scheduler : {0}")]
    Scheduler(String),
    #[error("Parse descriptor error : {0}")]
    ParseDesc(String),
    #[error("Operation timeout")]
    Timeout,
}
