#![allow(unused)]

use std::net::Ipv4Addr;

/// A descriptor for the to-card control ring buffer.
pub(crate) enum ToCardCtrlRbDesc {
    UpdateMrTable(ToCardCtrlRbDescUpdateMrTable),
    UpdatePageTable(ToCardCtrlRbDescUpdatePageTable),
    QpManagement(ToCardCtrlRbDescQpManagement),
}

/// A descriptor for the to-host control ring buffer.
pub(crate) enum ToHostCtrlRbDesc {
    UpdateMrTable(ToHostCtrlRbDescUpdateMrTable),
    UpdatePageTable(ToHostCtrlRbDescUpdatePageTable),
    QpManagement(ToHostCtrlRbDescQpManagement),
}

/// A descriptor for the to-card work ring buffer.
pub(crate) enum ToCardWorkRbDesc {
    Request(ToCardWorkRbDescRequest),
}

/// A descriptor for the to-host work ring buffer.
pub(crate) enum ToHostWorkRbDesc {
    SendQueueReport(ToHostWorkRbDescSendQueueReport),
    Bth(ToHostWorkRbDescBth),
    BthRethImmDt(ToHostWorkRbDescBthRethImmDt),
    BthAeth(ToHostWorkRbDescBthAeth),
    SecondaryReth(ToHostWorkRbDescSecondaryReth),
}

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
pub(crate) struct ToCardCtrlRbDescUpdateMrTable {
    pub(crate) common_header: CtrlRbDescCommonHeader,
    pub(crate) base_va: u64,
    pub(crate) mr_length: u32,
    pub(crate) mr_key: u32,
    pub(crate) pd_handler: u32,
    pub(crate) acc_flags: u8,
    pub(crate) pgt_offset: u32,
}

// typedef struct {
//     ReservedZero#(64)               reserved1;
//     Bit#(32)                        dmaReadLength;
//     Bit#(32)                        startIndex;
//     Bit#(64)                        dmaAddr;
//     CmdQueueDescCommonHead          commonHeader;
// } CmdQueueReqDescUpdatePGT deriving(Bits, FShow);
pub(crate) struct ToCardCtrlRbDescUpdatePageTable {
    pub(crate) common_header: CtrlRbDescCommonHeader,
    pub(crate) dma_addr: u64,
    pub(crate) start_index: u32,
    pub(crate) dma_read_length: u32,
}

// typedef struct {
//     ReservedZero#(104)              reserved1;      // 104 bits
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
pub(crate) struct ToCardCtrlRbDescQpManagement {
    pub(crate) common_header: CtrlRbDescCommonHeader,
    pub(crate) is_valid: bool,
    pub(crate) is_error: bool,
    pub(crate) qpn: u32,
    pub(crate) pd_handler: u32,
    pub(crate) qp_type: QpType,
    pub(crate) rq_access_flags: u8,
    pub(crate) pmtu: Pmtu,
}

// TODO: no corresponding struct
pub(crate) struct ToHostCtrlRbDescUpdateMrTable {
    pub(crate) common_header: CtrlRbDescCommonHeader,
}

// typedef struct {
//     ReservedZero#(64)               reserved1;
//     ReservedZero#(64)               reserved2;
//     ReservedZero#(64)               reserved3;
//     CmdQueueDescCommonHead          commonHeader;
// } CmdQueueRespDescUpdatePGT deriving(Bits, FShow);
pub(crate) struct ToHostCtrlRbDescUpdatePageTable {
    pub(crate) common_header: CtrlRbDescCommonHeader,
}

// TODO: no corresponding struct
pub(crate) struct ToHostCtrlRbDescQpManagement {
    pub(crate) common_header: CtrlRbDescCommonHeader,
}

// typedef struct {
//     ReservedZero#(64)           reserved1;        // 64 bits
//     AddrIPv4                    dqpIP;            // 32 bits
//     RKEY                        rkey;             // 32 bits
//     ADDR                        raddr;            // 64 bits
//     SendQueueDescCommonHead     commonHeader;     // 64 bits
// } SendQueueReqDescSeg0 deriving(Bits, FShow);
// typedef struct {
//     ReservedZero#(64)       reserved1;          // 64 bits
//     IMM                     imm;                // 32 bits
//     ReservedZero#(8)        reserved2;          // 8  bits
//     QPN                     dqpn;               // 24 bits
//     MAC                     macAddr;            // 48 bits
//     ReservedZero#(16)       reserved3;          // 16 bits
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
pub(crate) struct ToCardWorkRbDescRequest {
    pub(crate) common_header: ToCardWorkRbDescCommonHeader,
    pub(crate) raddr: u64,
    pub(crate) rkey: [u8; 4],
    pub(crate) dqp_ip: Ipv4Addr, // using Ipv4Addr temporarily for convenience
    pub(crate) pmtu: Pmtu,
    pub(crate) flags: u8,
    pub(crate) qp_type: QpType,
    pub(crate) sge_cnt: u8,
    pub(crate) psn: u32,
    pub(crate) mac_addr: [u8; 6],
    pub(crate) dqpn: u32,
    pub(crate) imm: [u8; 4],
    pub(crate) sgl: ScatterGatherList,
}

// typedef struct {
//     ReservedZero#(231)              reserved1;      // 231
//     Bool                            hasDmaRespErr;  // 1
//     ReservedZero#(23)               reserved2;      // 23
//     MeatReportQueueDescType         descType;       // 1
// } MeatReportQueueDescSendQueueReport deriving(Bits, FShow);
pub(crate) struct ToHostWorkRbDescSendQueueReport {
    pub(crate) desc_type: ToHostWorkRbDescType,
    pub(crate) has_dma_resp_err: bool,
}

// typedef struct {
//     ReservedZero#(160)              reserved1;      // 160
//     MeatReportQueueDescFragBTH      bth;            // 64
//     RdmaReqStatus                   reqStatus;      // 8
//     ReservedZero#(23)               reserved2;      // 23
//     MeatReportQueueDescType         descType;       // 1
// } MeatReportQueueDescBth deriving(Bits, FShow);
pub(crate) struct ToHostWorkRbDescBth {
    pub(crate) desc_type: ToHostWorkRbDescType,
    pub(crate) req_status: RdmaReqStatus,
    pub(crate) bth: ToHostWorkRbDescFragBth,
}

// typedef struct {
//     MeatReportQueueDescFragImmDT    immDt;          // 32
//     MeatReportQueueDescFragRETH     reth;           // 128
//     MeatReportQueueDescFragBTH      bth;            // 64
//     RdmaReqStatus                   reqStatus;      // 8
//     ReservedZero#(23)               reserved1;      // 23
//     MeatReportQueueDescType         descType;       // 1
// } MeatReportQueueDescBthRethImmDT deriving(Bits, FShow);
pub(crate) struct ToHostWorkRbDescBthRethImmDt {
    pub(crate) desc_type: ToHostWorkRbDescType,
    pub(crate) req_status: RdmaReqStatus,
    pub(crate) bth: ToHostWorkRbDescFragBth,
    pub(crate) reth: ToHostWorkRbDescFragReth,
    pub(crate) imm_dt: ToHostWorkRbDescFragImmDt,
}

// typedef struct {
//     ReservedZero#(105)              reserved1;      // 105
//     MeatReportQueueDescFragAETH     aeth;           // 55
//     MeatReportQueueDescFragBTH      bth;            // 64
//     RdmaReqStatus                   reqStatus;      // 8
//     ReservedZero#(23)               reserved2;      // 23
//     MeatReportQueueDescType         descType;       // 1
// } MeatReportQueueDescBthAeth deriving(Bits, FShow);
pub(crate) struct ToHostWorkRbDescBthAeth {
    pub(crate) desc_type: ToHostWorkRbDescType,
    pub(crate) req_status: RdmaReqStatus,
    pub(crate) bth: ToHostWorkRbDescFragBth,
    pub(crate) aeth: ToHostWorkRbDescFragAeth,
}

// typedef struct {
//     ReservedZero#(160)                          reserved1;       // 160
//     MeatReportQueueDescFragSecondaryRETH        secReth;         // 96
// } MeatReportQueueDescSecondaryReth deriving(Bits, FShow);
pub(crate) struct ToHostWorkRbDescSecondaryReth {
    pub(crate) sec_reth: ToHostWorkRbDescFragSecondaryReth,
}

// typedef struct {
//     Bit#(32)                userData;
//     ReservedZero#(20)       reserved1;
//     Bool                    isSuccessOrNeedSignalCplt;
//     Bit#(4)                 extraSegmentCnt;
//     Bit#(6)                 opCode;
//     Bool                    valid;
// } CmdQueueDescCommonHead deriving(Bits, FShow);
pub(crate) struct CtrlRbDescCommonHeader {
    pub(crate) valid: bool,
    pub(crate) opcode: CtrlRbDescOpcode,
    pub(crate) extra_segment_cnt: u8,
    pub(crate) is_success_or_need_signal_cplt: bool,
    pub(crate) user_data: [u8; 4],
}

// typedef enum {
//     CmdQueueOpcodeUpdateMrTable = 'h0,
//     CmdQueueOpcodeUpdatePGT = 'h1,
//     CmdQueueOpcodeQpManagement = 'h2
// } CommandQueueOpcode deriving(Bits, Eq);
pub(crate) enum CtrlRbDescOpcode {
    UpdateMrTable = 0x00,
    UpdatePageTable = 0x01,
    QpManagement = 0x02,
}

// typedef struct {
//     Length                  totalLen;
//     ReservedZero#(20)       reserved1;
//     Bool                    isSuccessOrNeedSignalCplt;
//     Bit#(4)                 extraSegmentCnt;
//     Bool                    isFirst;
//     Bool                    isLast;
//     WorkReqOpCode           opCode;
//     Bool                    valid;
// } SendQueueDescCommonHead deriving(Bits, FShow);
pub(crate) struct ToCardWorkRbDescCommonHeader {
    pub(crate) valid: bool,
    pub(crate) opcode: ToCardWorkRbDescOpcode,
    pub(crate) is_last: bool,
    pub(crate) is_first: bool,
    pub(crate) extra_segment_cnt: u8,
    pub(crate) is_success_or_need_signal_cplt: bool,
    pub(crate) total_len: u32,
}

// typedef enum {
//     IBV_WR_RDMA_WRITE           =  0,
//     IBV_WR_RDMA_WRITE_WITH_IMM  =  1,
//     IBV_WR_SEND                 =  2,
//     IBV_WR_SEND_WITH_IMM        =  3,
//     IBV_WR_RDMA_READ            =  4,
//     IBV_WR_ATOMIC_CMP_AND_SWP   =  5,
//     IBV_WR_ATOMIC_FETCH_AND_ADD =  6,
//     IBV_WR_LOCAL_INV            =  7,
//     IBV_WR_BIND_MW              =  8,
//     IBV_WR_SEND_WITH_INV        =  9,
//     IBV_WR_TSO                  = 10,
//     IBV_WR_DRIVER1              = 11
// } WorkReqOpCode deriving(Bits, Eq, FShow);
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ToCardWorkRbDescOpcode {
    RdmaWrite = 0,
    RdmaWriteWithImm = 1,
    Send = 2,
    SendWithImm = 3,
    RdmaRead = 4,
    AtomicCmpAndSwp = 5,
    AtomicFetchAndAdd = 6,
    LocalInv = 7,
    BindMw = 8,
    SendWithInv = 9,
    Tso = 10,
    Driver1 = 11,
}

// typedef struct {
//     ReservedZero#(6)                reserved1;    // 6
//     Bool                            ackReq;       // 1
//     Bool                            solicited;    // 1
//     PSN                             psn;          // 24
//     QPN                             dqpn;         // 24
//     RdmaOpCode                      opcode;       // 5
//     TransType                       trans;        // 3
// } MeatReportQueueDescFragBTH deriving(Bits, FShow);
pub(crate) struct ToHostWorkRbDescFragBth {
    pub(crate) trans: TransType,
    pub(crate) opcode: RdmaOpcode,
    pub(crate) dqpn: u32,
    pub(crate) psn: u32,
    pub(crate) solicited: bool,
    pub(crate) ack_req: bool,
}

// typedef struct {
//     Length                  dlen;         // 32
//     RKEY                    rkey;         // 32
//     ADDR                    va;           // 64
// } MeatReportQueueDescFragRETH deriving(Bits, FShow);
pub(crate) struct ToHostWorkRbDescFragReth {
    pub(crate) va: u64,
    pub(crate) rkey: [u8; 4],
    pub(crate) dlen: u32,
}

// typedef struct {
//     AethCode                code;         // 2
//     AethValue               value;        // 5
//     MSN                     msn;          // 24
//     PSN                     lastRetryPSN; // 24
// } MeatReportQueueDescFragAETH deriving(Bits, FShow);
pub(crate) struct ToHostWorkRbDescFragAeth {
    pub(crate) last_retry_psn: u32,
    pub(crate) msn: u32,
    pub(crate) value: u8,
    pub(crate) code: AethCode,
}

// typedef struct {
//     RKEY                            secondaryRkey;   // 32
//     ADDR                            secondaryVa;     // 64
// } MeatReportQueueDescFragSecondaryRETH deriving(Bits, FShow);
pub(crate) struct ToHostWorkRbDescFragSecondaryReth {
    pub(crate) secondary_va: u64,
    pub(crate) secondary_rkey: [u8; 4],
}

// typedef struct {
//     IMM                             data;           // 32
// } MeatReportQueueDescFragImmDT deriving(Bits, FShow);
pub(crate) struct ToHostWorkRbDescFragImmDt {
    pub(crate) data: [u8; 4],
}

// typedef enum {
//     MeatReportQueueDescTypeRecvPacketMeta = 0,
//     MeatReportQueueDescTypeSendFinished   = 1
// } MeatReportQueueDescType deriving(Bits, FShow);
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ToHostWorkRbDescType {
    RecvPacketMeta = 0,
    SendFinished = 1,
}

// typedef enum {
//     IBV_QPT_RC         = 2,
//     IBV_QPT_UC         = 3,
//     IBV_QPT_UD         = 4,
//     IBV_QPT_RAW_PACKET = 8,
//     IBV_QPT_XRC_SEND   = 9,
//     IBV_QPT_XRC_RECV   = 10
//     // IBV_QPT_DRIVER = 0xff
// } TypeQP deriving(Bits, Eq, FShow);
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum QpType {
    Rc = 2,
    Uc = 3,
    Ud = 4,
    RawPacket = 8,
    XrcSend = 9,
    XrcRecv = 10,
}

// typedef enum {
//     IBV_MTU_256  = 1,
//     IBV_MTU_512  = 2,
//     IBV_MTU_1024 = 3,
//     IBV_MTU_2048 = 4,
//     IBV_MTU_4096 = 5
// } PMTU deriving(Bits, Eq, FShow);
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Pmtu {
    Mtu256 = 1,
    Mtu512 = 2,
    Mtu1024 = 3,
    Mtu2048 = 4,
    Mtu4096 = 5,
}

// typedef enum {
//     TRANS_TYPE_RC  = 3'h0, // 3'b000
//     TRANS_TYPE_UC  = 3'h1, // 3'b001
//     TRANS_TYPE_RD  = 3'h2, // 3'b010
//     TRANS_TYPE_UD  = 3'h3, // 3'b011
//     TRANS_TYPE_CNP = 3'h4, // 3'b100
//     TRANS_TYPE_XRC = 3'h5  // 3'b101
// } TransType deriving(Bits, Bounded, Eq, FShow);
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TransType {
    Rc = 0x00,
    Uc = 0x01,
    Rd = 0x02,
    Ud = 0x03,
    Cnp = 0x04,
    Xrc = 0x05,
}

// typedef enum {
//     SEND_FIRST                     = 5'h00,
//     SEND_MIDDLE                    = 5'h01,
//     SEND_LAST                      = 5'h02,
//     SEND_LAST_WITH_IMMEDIATE       = 5'h03,
//     SEND_ONLY                      = 5'h04,
//     SEND_ONLY_WITH_IMMEDIATE       = 5'h05,
//     RDMA_WRITE_FIRST               = 5'h06,
//     RDMA_WRITE_MIDDLE              = 5'h07,
//     RDMA_WRITE_LAST                = 5'h08,
//     RDMA_WRITE_LAST_WITH_IMMEDIATE = 5'h09,
//     RDMA_WRITE_ONLY                = 5'h0a,
//     RDMA_WRITE_ONLY_WITH_IMMEDIATE = 5'h0b,
//     RDMA_READ_REQUEST              = 5'h0c,
//     RDMA_READ_RESPONSE_FIRST       = 5'h0d,
//     RDMA_READ_RESPONSE_MIDDLE      = 5'h0e,
//     RDMA_READ_RESPONSE_LAST        = 5'h0f,
//     RDMA_READ_RESPONSE_ONLY        = 5'h10,
//     ACKNOWLEDGE                    = 5'h11,
//     ATOMIC_ACKNOWLEDGE             = 5'h12,
//     COMPARE_SWAP                   = 5'h13,
//     FETCH_ADD                      = 5'h14,
//     RESYNC                         = 5'h15,
//     SEND_LAST_WITH_INVALIDATE      = 5'h16,
//     SEND_ONLY_WITH_INVALIDATE      = 5'h17
// } RdmaOpCode deriving(Bits, Bounded, Eq, FShow);
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RdmaOpcode {
    SendFirst = 0x00,
    SendMiddle = 0x01,
    SendLast = 0x02,
    SendLastWithImmediate = 0x03,
    SendOnly = 0x04,
    SendOnlyWithImmediate = 0x05,
    RdmaWriteFirst = 0x06,
    RdmaWriteMiddle = 0x07,
    RdmaWriteLast = 0x08,
    RdmaWriteLastWithImmediate = 0x09,
    RdmaWriteOnly = 0x0a,
    RdmaWriteOnlyWithImmediate = 0x0b,
    RdmaReadRequest = 0x0c,
    RdmaReadResponseFirst = 0x0d,
    RdmaReadResponseMiddle = 0x0e,
    RdmaReadResponseLast = 0x0f,
    RdmaReadResponseOnly = 0x10,
    Acknowledge = 0x11,
    AtomicAcknowledge = 0x12,
    CompareSwap = 0x13,
    FetchAdd = 0x14,
    Resync = 0x15,
    SendLastWithInvalidate = 0x16,
    SendOnlyWithInvalidate = 0x17,
}

// TODO: temporary struct
pub(crate) struct ScatterGatherList {
    pub(crate) data: [ScatterGatherElement; 1],
    pub(crate) len: u32,
}

// typedef struct {
//     ADDR   laddr;         // 64 bits
//     Length len;           // 32 bits
//     LKEY   lkey;          // 32 bits
// } SendQueueReqDescFragSGE deriving(Bits, FShow);
pub(crate) struct ScatterGatherElement {
    pub(crate) laddr: u64,
    pub(crate) lkey: [u8; 4],
    pub(crate) len: u32,
}

// typedef enum {
//     RDMA_REQ_ST_NORMAL              = 1,
//     RDMA_REQ_ST_INV_ACC_FLAG        = 2,
//     RDMA_REQ_ST_INV_OPCODE          = 3,
//     RDMA_REQ_ST_INV_MR_KEY          = 4,
//     RDMA_REQ_ST_INV_MR_REGION       = 5,
//     RDMA_REQ_ST_UNKNOWN             = 6,
//     RDMA_REQ_ST_MAX_GUARD           = 255
// } RdmaReqStatus deriving(Bits, Eq, FShow);
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RdmaReqStatus {
    Normal = 1,
    InvAccFlag = 2,
    InvOpcode = 3,
    InvMrKey = 4,
    InvMrRegion = 5,
    Unknown = 6,
    MaxGuard = 255,
}

// typedef enum {
//     AETH_CODE_ACK  = 2'b00,
//     AETH_CODE_RNR  = 2'b01,
//     AETH_CODE_RSVD = 2'b10,
//     AETH_CODE_NAK  = 2'b11
// } AethCode deriving(Bits, Bounded, Eq, FShow);
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AethCode {
    Ack = 0b00,
    Rnr = 0b01,
    Rsvd = 0b10,
    Nak = 0b11,
}