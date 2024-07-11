use std::{io, net::Ipv4Addr};

use super::{
    layout::{
        MetaReportQueueDescBthReth, MetaReportQueueDescFragAETH, MetaReportQueueDescFragBTH,
        MetaReportQueueDescFragImmDT, MetaReportQueueDescFragRETH,
        MetaReportQueueDescFragSecondaryRETH, SendQueueDescCommonHead, SendQueueReqDescFragSGE,
        SendQueueReqDescSeg0, SendQueueReqDescSeg1,
        OFFSET_OF_AETH_IN_META_REPORT_QUEUE_DESC_FRAG_IMM_DT_IN_BYTES,
        OFFSET_OF_BTH_IN_META_REPORT_QUEUE_DESC_BTH_RETH_IN_BYTES,
        OFFSET_OF_IMM_IN_META_REPORT_QUEUE_DESC_FRAG_IMM_DT,
        OFFSET_OF_RETH_IN_META_REPORT_QUEUE_DESC_BTH_RETH_IN_BYTES, SIZE_OF_BTH_IN_BYTES,
        SIZE_OF_IMM_IN_BYTES,
    },
};
use crate::{
    device::{error::DeviceResult, DeviceError},
    types::{Imm, Key, Msn, Pmtu, Psn, QpType, Qpn, Sge, WorkReqSendFlag}, utils::u8_slice_to_u64,
};
use eui48::MacAddress;
use num_enum::TryFromPrimitive;

/// Card to host work ring buffer descriptor
#[derive(Clone, Debug)]
pub(crate) enum ToCardWorkRbDesc {
    Read(ToCardWorkRbDescRead),
    Write(ToCardWorkRbDescWrite),
    WriteWithImm(ToCardWorkRbDescWriteWithImm),
    ReadResp(ToCardWorkRbDescWrite),
}

/// Host to card work ring buffer descriptor
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

/// Common fields for host to card work ring buffer descriptor
#[derive(Clone, Debug)]
pub(crate) struct ToCardWorkRbDescCommon {
    /// Total length of the payload
    pub(crate) total_len: u32,

    /// Remote virtual address
    pub(crate) raddr: u64,

    /// `rkey` for raddr
    pub(crate) rkey: Key,

    /// destination qp ip
    pub(crate) dqp_ip: Ipv4Addr,

    /// destination qp number
    pub(crate) dqpn: Qpn,

    /// destination mac address
    pub(crate) mac_addr: MacAddress,

    /// PMTU
    pub(crate) pmtu: Pmtu,

    /// Send flags
    pub(crate) flags: WorkReqSendFlag,

    /// QP type
    pub(crate) qp_type: QpType,

    /// PSN
    pub(crate) psn: Psn,

    /// MSN
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

/// Host to card workq read request descriptor
#[derive(Default, Clone, Debug)]
pub(crate) struct ToCardWorkRbDescRead {
    /// Common fields
    pub(crate) common: ToCardWorkRbDescCommon,

    /// Scatter gather element
    pub(crate) sge: DescSge,
}

/// Host to card workq write descriptor
#[derive(Default, Clone, Debug)]
pub(crate) struct ToCardWorkRbDescWrite {
    /// Common fields
    pub(crate) common: ToCardWorkRbDescCommon,

    /// Is last descriptor of this transaction
    pub(crate) is_last: bool,

    /// Is the first descriptor of this transaction
    pub(crate) is_first: bool,

    /// Scatter gather element
    pub(crate) sge0: DescSge,
}

/// Host to card workq write with immediate descriptor
#[derive(Clone, Debug, Default)]
pub(crate) struct ToCardWorkRbDescWriteWithImm {
    /// Common fields
    pub(crate) common: ToCardWorkRbDescCommon,

    /// Is last descriptor of this transaction
    pub(crate) is_last: bool,

    /// Is the first descriptor of this transaction
    pub(crate) is_first: bool,

    /// Immediate data
    pub(crate) imm: u32,

    /// Scatter gather element
    pub(crate) sge0: DescSge,
}

/// Host to card workq response common fields
#[derive(Debug, Default, Clone)]
pub(crate) struct ToHostWorkRbDescCommon {
    /// status of the descriptor
    pub(crate) status: ToHostWorkRbDescStatus,

    /// transport type
    pub(crate) trans: ToHostWorkRbDescTransType,

    /// destination qp number
    pub(crate) dqpn: Qpn,

    /// MSN
    pub(crate) msn: Msn,

    /// expected PSN
    pub(crate) expected_psn: Psn,
}

/// Host to card workq read request descriptor
#[derive(Debug, Default, Clone)]
pub(crate) struct ToHostWorkRbDescRead {
    /// Common fields
    pub(crate) common: ToHostWorkRbDescCommon,

    /// length of the payload
    pub(crate) len: u32,

    /// local address
    pub(crate) laddr: u64,

    /// local key
    pub(crate) lkey: Key,

    /// remote address
    pub(crate) raddr: u64,

    /// remote key
    pub(crate) rkey: Key,
}

/// Host to workq work write or read response descriptor
#[derive(Debug, Clone)]
pub(crate) struct ToHostWorkRbDescWriteOrReadResp {
    /// Common fields
    pub(crate) common: ToHostWorkRbDescCommon,

    /// Is this a read response?
    pub(crate) is_read_resp: bool,

    /// Write type
    pub(crate) write_type: ToHostWorkRbDescWriteType,

    /// PSN
    pub(crate) psn: Psn,

    /// Write address
    pub(crate) addr: u64,

    /// Write length
    pub(crate) len: u32,

    /// Can hardware auto ack
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

/// Host to card work write with immediate descriptor
#[allow(unused)] // Currently we don't have write imm descriptor
#[derive(Debug)]
pub(crate) struct ToHostWorkRbDescWriteWithImm {
    /// Common fields
    pub(crate) common: ToHostWorkRbDescCommon,

    /// Write type
    pub(crate) write_type: ToHostWorkRbDescWriteType,

    /// PSN
    pub(crate) psn: Psn,

    /// Immediate data
    pub(crate) imm: u32,

    /// Write address
    pub(crate) addr: u64,

    /// Write length
    pub(crate) len: u32,

    /// Can hardware auto ack
    pub(crate) key: Key,
}

/// Host to card work acknowledge descriptor
#[derive(Debug, Default, Clone)]
pub(crate) struct ToHostWorkRbDescAck {
    /// Common fields
    pub(crate) common: ToHostWorkRbDescCommon,

    /// MSN
    pub(crate) msn: Msn,

    /// PSN
    pub(crate) psn: Psn,

    /// AETH code
    pub(crate) code: ToHostWorkRbDescAethCode,
    #[allow(unused)] // used in nack checking
    pub(crate) value: u8,
}

/// Host to card work raw descriptor
#[derive(Debug, Default)]
pub(crate) struct ToHostWorkRbDescRaw {
    /// Common fields
    pub(crate) common: ToHostWorkRbDescCommon,

    /// Raw data address
    pub(crate) addr: u64,

    /// Raw data length
    pub(crate) len: u32,

    /// Raw data lkey
    pub(crate) key: Key,
}

#[derive(Default, Debug, Clone, Copy)]
pub(crate) struct DescSge {
    addr: u64,
    len: u32,
    key: Key,
}

impl From<Sge> for DescSge {
    fn from(sge: Sge) -> Self {
        Self {
            addr: sge.phy_addr,
            len: sge.len,
            key: sge.key,
        }
    }
}

#[derive(TryFromPrimitive, Debug, Clone)]
#[repr(u8)]
pub(crate) enum ToHostWorkRbDescStatus {
    /// Normal status
    Normal = 1,

    /// Invalid access flag
    InvAccFlag = 2,

    /// Invalid opcode
    InvOpcode = 3,

    /// Invalid MR key
    InvMrKey = 4,

    /// Invalid MR region
    InvMrRegion = 5,

    /// Unknown error
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

/// Incomplete `ToHostWorkRbDesc`
///
/// Most descriptor take only one slot, but some descriptors like read request
/// need more than one slot to store the whole descriptor.
pub(crate) struct IncompleteToHostWorkRbDesc {
    parsed: ToHostWorkRbDesc,
}

/// Card to host work ring buffer descriptor error
pub(crate) enum ToHostWorkRbDescError {
    Incomplete(IncompleteToHostWorkRbDesc),
    DeviceError(DeviceError),
}

/// To host opcode
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

/// Aeth code
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

        let (common, sge_cnt) = match self {
            ToCardWorkRbDesc::Read(desc) => (&desc.common, 1u64),
            ToCardWorkRbDesc::Write(desc) | ToCardWorkRbDesc::ReadResp(desc) => {
                (&desc.common, 1u64)
            }
            ToCardWorkRbDesc::WriteWithImm(desc) => (&desc.common, 1u64),
        };
        let mut desc_common = SendQueueReqDescSeg1(dst);
        desc_common.set_pmtu(common.pmtu as u64);
        desc_common.set_flags(u64::from(common.flags.bits()));
        desc_common.set_qp_type(common.qp_type as u64);
        desc_common.set_seg_cnt(sge_cnt);
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

        let sge0 = match self {
            ToCardWorkRbDesc::Read(desc) => &desc.sge,
            ToCardWorkRbDesc::Write(desc) | ToCardWorkRbDesc::ReadResp(desc) => &desc.sge0,
            ToCardWorkRbDesc::WriteWithImm(desc) => &desc.sge0,
        };
        // Note that the order of the sges is reversed in the struct
        let mut frag_sge = SendQueueReqDescFragSGE(&mut dst[16..32]);
        frag_sge.set_laddr(sge0.addr);
        frag_sge.set_len(sge0.len.into());
        frag_sge.set_lkey(sge0.key.get().into());
    }

    #[allow(clippy::unused_self)] // we might need more than 3 descriptors later
    pub(super) fn serialized_desc_cnt(&self) -> u32 {
        3
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
        let frag_reth = MetaReportQueueDescFragRETH(
            &src[OFFSET_OF_RETH_IN_META_REPORT_QUEUE_DESC_BTH_RETH_IN_BYTES..],
        );
        let addr = frag_reth.get_va();
        // bitfield restricts the field is not longer than 32 bits.
        #[allow(clippy::cast_possible_truncation)]
        let key = Key::new_unchecked(frag_reth.get_rkey() as u32);
        #[allow(clippy::cast_possible_truncation)]
        let len = frag_reth.get_dlen() as u32;

        (addr, key, len)
    }

    fn read_imm(src: &[u8]) -> u32 {
        // typedef struct {
        //     IMM                             data;           // 32
        // } MetaReportQueueDescFragImmDT deriving(Bits, FShow);

        // first 28 bytes are desc type, status, bth and reth
        #[allow(clippy::indexing_slicing)]
        let imm = MetaReportQueueDescFragImmDT(
            &src[OFFSET_OF_IMM_IN_META_REPORT_QUEUE_DESC_FRAG_IMM_DT
                ..OFFSET_OF_IMM_IN_META_REPORT_QUEUE_DESC_FRAG_IMM_DT + SIZE_OF_IMM_IN_BYTES],
        );
        // call the `to_be` to convert order
        imm.get_imm()
    }

    // (last_psn, msn, value, code)
    #[allow(clippy::cast_possible_truncation)]
    fn read_aeth(src: &[u8]) -> DeviceResult<(Psn, Msn, u8, ToHostWorkRbDescAethCode)> {
        // typedef struct {
        //     AethCode                code;         // 3
        //     AethValue               value;        // 5
        //     MSN                     msn;          // 24
        //     PSN                     lastRetryPSN; // 24
        // } MetaReportQueueDescFragAETH deriving(Bits, FShow);

        // first 12 bytes are desc type, status and bth
        #[allow(clippy::indexing_slicing)]
        let frag_aeth = MetaReportQueueDescFragAETH(
            &src[OFFSET_OF_AETH_IN_META_REPORT_QUEUE_DESC_FRAG_IMM_DT_IN_BYTES..],
        );
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
    pub(super) fn read(src: &[u8]) -> Result<ToHostWorkRbDesc, ToHostWorkRbDescError> {
        // typedef struct {
        //     ReservedZero#(8)                reserved1;      // 8
        //     MSN                             msn;            // 24
        //     MetaReportQueueDescFragRETH     reth;           // 128
        //     MetaReportQueueDescFragBTH      bth;            // 64
        //     RdmaReqStatus                   reqStatus;      // 8
        //     PSN                             expectedPSN;    // 24
        // } MetaReportQueueDescBthRethReth deriving(Bits, FShow);
        let desc_bth = MetaReportQueueDescBthReth(&src);

        let expected_psn = Psn::new(desc_bth.get_expected_psn() as u32);
        let mut status = desc_bth.get_req_status() as u8;
        if status == 0 {
            status = desc_bth.get_req_status() as u8;
        }
        let status = ToHostWorkRbDescStatus::try_from(status).map_err(|_| {
            ToHostWorkRbDescError::DeviceError(DeviceError::ParseDesc(format!(
                "ToHostWorkRbDescStatus = {status} {desc_bth:?} can not be parsed"
            )))
        })?;

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

        let desc_frag_bth = MetaReportQueueDescFragBTH(
            &src[OFFSET_OF_BTH_IN_META_REPORT_QUEUE_DESC_BTH_RETH_IN_BYTES
                ..OFFSET_OF_BTH_IN_META_REPORT_QUEUE_DESC_BTH_RETH_IN_BYTES + SIZE_OF_BTH_IN_BYTES],
        );
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
        let msg_seq_number = Msn::new(desc_bth.get_msn() as u16);

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
                let can_auto_ack = desc_bth.get_can_auto_ack();
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
                    },
                ))
            }
            ToHostWorkRbDescOpcode::Acknowledge => {
                let (_last_psn, msn_in_ack, value, code) =
                    Self::read_aeth(src).map_err(ToHostWorkRbDescError::DeviceError)?;
                Ok(ToHostWorkRbDesc::Ack(ToHostWorkRbDescAck {
                    common,
                    msn: msn_in_ack,
                    value,
                    psn,
                    code,
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
            let key = Key::new_unchecked(secondary_reth.get_secondary_rkey() as u32);

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
    sge: Option<Sge>,
    imm: Option<u32>,
}

impl ToCardWorkRbDescBuilder {
    /// Create a new `ToCardWorkRbDescBuilder`
    pub(crate) fn new(type_: ToCardWorkRbDescOpcode) -> Self {
        Self {
            type_,
            common: None,
            sge: None,
            imm: None,
        }
    }

    /// with `ToCardWorkRbDescCommon`
    pub(crate) fn with_common(mut self, common: ToCardWorkRbDescCommon) -> Self {
        self.common = Some(common);
        self
    }

    /// with scatter gather element
    pub(crate) fn with_sge(mut self, sge: Sge) -> Self {
        self.sge = Some(sge);
        self
    }

    /// with immediate
    pub(crate) fn with_imm(mut self, imm: Imm) -> Self {
        self.imm = Some(imm.get());
        self
    }

    /// build a to card work ringbuf descriptor
    pub(crate) fn build(self) -> Result<Box<ToCardWorkRbDesc>, io::Error> {
        let common = self
            .common
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing common"))?;
        let desc = match self.type_ {
            ToCardWorkRbDescOpcode::Write => {
                if let Some(sge) = self.sge {
                    ToCardWorkRbDesc::Write(ToCardWorkRbDescWrite {
                        common,
                        is_last: true,
                        is_first: true,
                        sge0: sge.into(),
                    })
                } else {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput, "missing sge"));
                }
            }
            ToCardWorkRbDescOpcode::WriteWithImm => {
                let Some(sge) = self.sge else {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput, "missing sge"));
                };
                let Some(imm) = self.imm else {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput, "missing imm"));
                };
                ToCardWorkRbDesc::WriteWithImm(ToCardWorkRbDescWriteWithImm {
                    common,
                    is_last: true,
                    is_first: true,
                    imm,
                    sge0: sge.into(),
                })
            }
            ToCardWorkRbDescOpcode::Read => {
                if let Some(sge) = self.sge {
                    ToCardWorkRbDesc::Read(ToCardWorkRbDescRead {
                        common,
                        sge: sge.into(),
                    })
                } else {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput, "missing sge"));
                }
            }
            ToCardWorkRbDescOpcode::ReadResp => {
                if let Some(sge) = self.sge {
                    ToCardWorkRbDesc::ReadResp(ToCardWorkRbDescWrite {
                        common,
                        is_last: true,
                        is_first: true,
                        sge0: sge.into(),
                    })
                } else {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput, "missing sge"));
                }
            }
        };
        Ok(Box::new(desc))
    }
}
