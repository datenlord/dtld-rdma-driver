use crate::{
    device::{error::DeviceResult, DeviceError},
    types::{Key, MemAccessTypeFlag, Pmtu, Psn, QpType, Qpn}, utils::u8_slice_to_u64,
};
use eui48::MacAddress;
use num_enum::TryFromPrimitive;
use std::net::Ipv4Addr;

use super::layout::{
    CmdQueueDescCommonHead, CmdQueueReqDescQpManagementSeg0, CmdQueueReqDescSetNetworkParam,
    CmdQueueReqDescSetRawPacketReceiveMeta, CmdQueueReqDescUpdateErrRecoverPoint,
    CmdQueueReqDescUpdateMrTable, CmdQueueReqDescUpdatePGT,
};

#[derive(Debug)]
/// Host to card cmdq descriptor
pub(crate) enum ToCardCtrlRbDesc {
    /// Update memory region table
    UpdateMrTable(ToCardCtrlRbDescUpdateMrTable),

    /// Update page table
    UpdatePageTable(ToCardCtrlRbDescUpdatePageTable),

    /// QP management
    QpManagement(ToCardCtrlRbDescQpManagement),

    /// Set network param
    SetNetworkParam(ToCardCtrlRbDescSetNetworkParam),

    /// Set raw packet receive meta
    SetRawPacketReceiveMeta(ToCardCtrlRbDescSetRawPacketReceiveMeta),

    /// Update error psn recover point
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

/// cmdq response descriptor
#[derive(Debug)]
pub(crate) struct ToHostCtrlRbDesc {
    pub(crate) common: ToHostCtrlRbDescCommon,
}

/// cmdq response descriptor common header
#[derive(Debug)]
pub(crate) struct ToHostCtrlRbDescCommon {
    /// The operation id,
    pub(crate) op_id: u32, 

    /// The opcode of the descriptor
    pub(crate) opcode: CtrlRbDescOpcode,

    /// The result of the operation
    pub(crate) is_success: bool,
}

/// common header for host to card cmdq descriptor
#[derive(Debug, Default)]
pub(crate) struct ToCardCtrlRbDescCommon {
    pub(crate) op_id: u32, // user_data
}

/// cmdq update memory region table descriptor
#[derive(Debug)]
pub(crate) struct ToCardCtrlRbDescUpdateMrTable {
    /// common header
    pub(crate) common: ToCardCtrlRbDescCommon,

    /// The base virtual address of the memory region
    pub(crate) addr: u64,

    /// The length of the memory region
    pub(crate) len: u32,

    /// The lkey of the memory region
    pub(crate) key: Key,

    /// The pd handler of the memory region
    pub(crate) pd_hdl: u32,

    /// The access flags of the memory region
    pub(crate) acc_flags: MemAccessTypeFlag,

    /// The offset of in the page table
    pub(crate) pgt_offset: u32,
}

/// cmdq update page table descriptor
#[derive(Debug)]
pub(crate) struct ToCardCtrlRbDescUpdatePageTable {
    /// common header
    pub(crate) common: ToCardCtrlRbDescCommon,

    /// The start address of the page table
    pub(crate) start_addr: u64,

    /// The index of the page table
    pub(crate) pgt_idx: u32,  //offset

    /// The count of page table entries
    pub(crate) pgte_cnt: u32, //bytes
}

/// cmdq qp management descriptor
#[derive(Debug)]
pub(crate) struct ToCardCtrlRbDescQpManagement {
    /// common header
    pub(crate) common: ToCardCtrlRbDescCommon,

    /// is this qp valid
    pub(crate) is_valid: bool,

    /// The QP number
    pub(crate) qpn: Qpn,

    /// The PD handle
    pub(crate) pd_hdl: u32,

    /// The type of the QP
    pub(crate) qp_type: QpType,

    /// The access flags of the receive queue
    pub(crate) rq_acc_flags: MemAccessTypeFlag,

    /// The pmtu of the QP
    pub(crate) pmtu: Pmtu,

    /// The peer QP number
    pub(crate) peer_qpn: Qpn,
}

/// cmdq set network param descriptor
#[derive(Debug)]
pub(crate) struct ToCardCtrlRbDescSetNetworkParam {
    /// common header
    pub(crate) common: ToCardCtrlRbDescCommon,

    /// The gateway of the network
    pub(crate) gateway: Ipv4Addr,

    /// The netmask of the network
    pub(crate) netmask: Ipv4Addr,

    /// The ip address of the network
    pub(crate) ipaddr: Ipv4Addr,

    /// The mac address of the network
    pub(crate) macaddr: MacAddress,
}

/// cmdq set raw packet receive meta descriptor
#[derive(Debug)]
pub(crate) struct ToCardCtrlRbDescSetRawPacketReceiveMeta {
    /// common header
    pub(crate) common: ToCardCtrlRbDescCommon,

    /// The base write address of the raw packet receive meta
    pub(crate) base_write_addr: u64,

    /// The key of the memory region
    pub(crate) key: Key,
}

/// cmdq update error psn recover point descriptor
#[derive(Debug)]
pub(crate) struct ToCardCtrlRbDescUpdateErrPsnRecoverPoint {
    /// common header
    pub(crate) common: ToCardCtrlRbDescCommon,

    /// The QP number
    pub(crate) qpn: Qpn,

    /// The PSN to recover this qp
    pub(crate) recover_psn: Psn,
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
    pub(super) fn read(src: &[u8]) -> DeviceResult<ToHostCtrlRbDesc> {
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
