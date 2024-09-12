/*
Base and extended transport header
*/

use std::{
    mem::{size_of, transmute},
    net::Ipv4Addr,
};

use thiserror::Error;

use crate::{
    device::{ToHostWorkRbDescAethCode, ToHostWorkRbDescOpcode, ToHostWorkRbDescTransType},
    types::QpType,
};

use super::types::{
    AethHeader, Metadata, PayloadInfo, RdmaGeneralMeta, RdmaMessage, RdmaMessageMetaCommon,
    RethHeader,
};

pub(crate) const ICRC_SIZE: usize = 4;
pub(crate) const IPV4_DEFAULT_VERSION_AND_HEADER_LENGTH: u8 = 0x45;
pub(crate) const IPV4_DEFAULT_DSCP_AND_ECN: u8 = 0;
pub(crate) const IPV4_PROTOCOL_UDP: u8 = 0x11;
pub(crate) const IPV4_DEFAULT_TTL: u8 = 64;
pub(crate) const RDMA_PAYLOAD_ALIGNMENT: usize = 4;

const BTH_OPCODE_MASK: u8 = 0x1F;
const BTH_TRANSACTION_TYPE_MASK: u8 = 0xE0;
const BTH_TRANSACTION_TYPE_SHIFT: usize = 5;
const BTH_DESTINATION_QPN_MASK: u32 = 0x00FF_FFFF;
const BTH_FLAGS_SOLICITED_MASK: u8 = 0x80;
const BTH_FLAGS_PAD_CNT_MASK: u8 = 0x60;
const BTH_FLAGS_PAD_CNT_SHIFT: usize = 5;
const BTH_ACK_REQ_MASK: u8 = 0x80;
const BTH_PSN_MASK: u32 = 0x00FF_FFFF;
const MAX_AETH_CODE: u8 = 4;
const AETH_CODE_MASK: u8 = 0x60;
const AETH_CODE_SHIFT: usize = 5;
const AETH_VALUE_MASK: u8 = 0x1F;
const AETH_MSN_MASK: u32 = 0x00FF_FFFF;

/// Base Transport Header of RDMA over Ethernet
#[derive(Clone, Copy)]
#[repr(C, packed)]
#[allow(clippy::upper_case_acronyms)]
pub(crate) struct BTH {
    tran_type_and_opcode: u8, // 1 byte: (3bit)transaction type+(5bit)opcode
    flags: u8,                // 1 byte, include solicited, padCnt
    pkey: [u8; 2],            // 2 bytes
    destination_qpn: [u8; 4], // 4 bytes.The higher 1 byte is not used.
    psn: [u8; 4],             // ack_req(1bit) + psn = 32 bits
}

impl BTH {
    /// SAFETY: we assmue the buffer is a valid BTH
    #[allow(clippy::transmute_ptr_to_ref)]
    pub(crate) fn from_bytes(bytes: &[u8]) -> &'static mut Self {
        unsafe { transmute(bytes.as_ptr()) }
    }

    pub(crate) fn get_transaction_type(&self) -> u8 {
        (self.tran_type_and_opcode & BTH_TRANSACTION_TYPE_MASK) >> BTH_TRANSACTION_TYPE_SHIFT
    }

    pub(crate) fn get_opcode(&self) -> u8 {
        self.tran_type_and_opcode & BTH_OPCODE_MASK
    }

    pub(crate) fn get_solicited(&self) -> bool {
        self.flags & BTH_FLAGS_SOLICITED_MASK != 0
    }

    pub(crate) fn get_pad_cnt(&self) -> u8 {
        (self.flags & BTH_FLAGS_PAD_CNT_MASK) >> BTH_FLAGS_PAD_CNT_SHIFT
    }

    #[allow(clippy::arithmetic_side_effects)]// pad_cnt is derived from payload_length,and will always be less than payload_length
    pub(crate) fn get_packet_real_length(&self, payload_length: usize) -> usize {
        let pad_cnt: usize = self.get_pad_cnt().into();
        payload_length - pad_cnt
    }

    pub(crate) fn get_pkey(&self) -> u16 {
        u16::from_be_bytes(self.pkey)
    }

    pub(crate) fn get_destination_qpn(&self) -> u32 {
        u32::from_be_bytes([
            0,
            self.destination_qpn[1],
            self.destination_qpn[2],
            self.destination_qpn[3],
        ])
    }

    pub(crate) fn get_ack_req(&self) -> bool {
        (self.psn[0] & BTH_ACK_REQ_MASK) != 0
    }

    pub(crate) fn get_psn(&self) -> u32 {
        u32::from_be_bytes([0, self.psn[1], self.psn[2], self.psn[3]])
    }

    pub(crate) fn set_opcode_and_type(
        &mut self,
        opcode: ToHostWorkRbDescOpcode,
        tran_type: ToHostWorkRbDescTransType,
    ) {
        self.tran_type_and_opcode =
            (tran_type as u8) << BTH_TRANSACTION_TYPE_SHIFT | (opcode as u8);
    }

    pub(crate) fn set_flags_solicited(&mut self, is_solicited: bool) {
        if is_solicited {
            self.flags |= BTH_FLAGS_SOLICITED_MASK;
        } else {
            self.flags &= !BTH_FLAGS_SOLICITED_MASK;
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn set_pad_cnt(&mut self, pad_cnt: usize) {
        self.flags =
            (self.flags & !BTH_FLAGS_PAD_CNT_MASK) | ((pad_cnt as u8) << BTH_FLAGS_PAD_CNT_SHIFT);
    }

    pub(crate) fn set_pkey(&mut self, pkey: u16) {
        self.pkey = pkey.to_be_bytes();
    }

    pub(crate) fn set_destination_qpn(&mut self, qpn: u32) {
        self.destination_qpn = (qpn & BTH_DESTINATION_QPN_MASK).to_be_bytes();
    }

    pub(crate) fn set_ack_req(&mut self, ack_req: bool) {
        if ack_req {
            self.psn[0] |= BTH_ACK_REQ_MASK;
        } else {
            self.psn[0] &= !BTH_ACK_REQ_MASK;
        }
    }

    pub(crate) fn set_psn(&mut self, psn: u32) {
        let ack_req = self.psn[0];
        self.psn = (psn & BTH_PSN_MASK).to_be_bytes();
        self.psn[0] = ack_req;
    }

    /// used for icrc check
    pub(crate) fn fill_ecn_and_resv6(&mut self) {
        self.destination_qpn[0] = 0xff;
    }

    /// convert the &`RdmaMessageMetaCommon` to `BTH`
    pub(crate) fn set_from_common_meta(
        &mut self,
        common_meta: &RdmaMessageMetaCommon,
        pad_cnt: usize,
    ) {
        self.set_opcode_and_type(common_meta.opcode.clone(), common_meta.tran_type);
        self.set_flags_solicited(common_meta.solicited);
        self.set_pad_cnt(pad_cnt);
        self.set_destination_qpn(common_meta.dqpn.get());
        self.set_ack_req(common_meta.ack_req);
        self.set_psn(common_meta.psn.get());
        self.set_pkey(common_meta.pkey.get());
    }
}

/// RDMA Extended Transport Header
#[repr(C, packed)]
#[allow(clippy::upper_case_acronyms)]
pub(crate) struct RETH {
    va: [u8; 8],
    rkey: [u8; 4],
    dlen: [u8; 4],
}

impl RETH {
    /// SAFETY: we assmue the buffer is a valid RETH
    #[cfg(test)]
    #[allow(clippy::transmute_ptr_to_ref)]
    pub(crate) fn from_bytes(bytes: &[u8]) -> &'static mut Self {
        unsafe { transmute(bytes.as_ptr()) }
    }

    pub(crate) fn get_va(&self) -> u64 {
        u64::from_be_bytes(self.va)
    }

    pub(crate) fn get_rkey(&self) -> u32 {
        u32::from_be_bytes(self.rkey)
    }

    pub(crate) fn get_dlen(&self) -> u32 {
        u32::from_be_bytes(self.dlen)
    }

    pub(crate) fn set_va(&mut self, va: u64) {
        self.va = u64::to_be_bytes(va);
    }

    pub(crate) fn set_rkey(&mut self, rkey: u32) {
        self.rkey = u32::to_be_bytes(rkey);
    }

    pub(crate) fn set_dlen(&mut self, dlen: u32) {
        self.dlen = u32::to_be_bytes(dlen);
    }

    pub(crate) fn set_from_reth_header(&mut self, reth: &RethHeader) {
        self.set_va(reth.va);
        self.set_rkey(reth.rkey.get());
        self.set_dlen(reth.len);
    }
}

/// ACK Extended Transport Header
/// ┌──┬───────┬──────────────┬──────────────────────────────────┐
/// │  │code(2)│ value(5)     │ MSN(24)                          │
/// └──┴───────┴──────────────┴──────────────────────────────────┘
#[repr(C, packed)]
#[allow(clippy::upper_case_acronyms)]
pub(crate) struct AETH {
    value: [u8; 4], // 1 bit for res
}

impl AETH {
    /// SAFETY: we assmue the buffer is a valid AETH
    #[cfg(test)]
    #[allow(clippy::transmute_ptr_to_ref)]
    pub(crate) fn from_bytes(bytes: &[u8]) -> &'static mut Self {
        unsafe { transmute(bytes.as_ptr()) }
    }

    pub(crate) fn get_aeth_code(&self) -> u8 {
        (self.value[0] & AETH_CODE_MASK) >> AETH_CODE_SHIFT
    }

    pub(crate) fn get_aeth_value(&self) -> u8 {
        self.value[0] & AETH_VALUE_MASK
    }

    pub(crate) fn get_msn(&self) -> u32 {
        u32::from_be_bytes([0, self.value[1], self.value[2], self.value[3]])
    }

    pub(crate) fn set_aeth_code_and_value(&mut self, code: u8, value: u8) {
        self.value[0] = (code % MAX_AETH_CODE) << AETH_CODE_SHIFT | value;
    }

    pub(crate) fn set_msn(&mut self, msn: u32) {
        let mut new_value = (msn & AETH_MSN_MASK).to_be_bytes();
        new_value[0] = self.value[0];
        self.value = new_value;
    }
}

/// The `imm` of RDMA protocol
pub(crate) struct Immediate([u8; 4]);

impl Immediate {
    pub(crate) fn get(&self) -> u32 {
        u32::from_be_bytes(self.0)
    }

    pub(crate) fn set(&mut self, imm: u32) {
        self.0 = imm.to_be_bytes();
    }
}

/// Rdma packet header trait.
///
/// We use trait instead of enum because the `enum` requires additional space to store the variant.
pub(crate) trait RdmaPacketHeader: Sized {
    /// Get the pointer to the payload data
    ///
    /// The payload is just behind the header, so we can get the pointer to the payload data by adding 1 to the header pointer.
    /// SAFETY: User should ensure the buffer is large enough to hold the packet header
    fn get_data_ptr(&self) -> *const u8 {
        unsafe { (self as *const Self).offset(1).cast::<u8>() }
    }

    /// Get a reference to the packet header
    ///
    /// SAFETY: User should ensure the buffer is large enough to hold the packet header
    #[allow(clippy::transmute_ptr_to_ref)]
    fn from_bytes(bytes: &[u8]) -> &'static mut Self {
        unsafe { transmute(bytes.as_ptr()) }
    }

    /// Convert the packet header to `RdmaMessage`
    fn to_rdma_message(&self, buf_size: usize) -> Result<RdmaMessage, PacketError>;

    /// Convert the `RdmaMessage` to packet header
    fn set_from_rdma_message(&mut self, message: &RdmaMessage) -> Result<usize, PacketError>;
}

/// A composite packet header layout that contains the BTH and RETH
#[repr(C, packed)]
pub(crate) struct RdmaHeaderReqBthReth {
    pub(crate) bth: BTH,
    pub(crate) reth: RETH,
}

impl RdmaPacketHeader for RdmaHeaderReqBthReth {
    fn to_rdma_message(&self, buf_size: usize) -> Result<RdmaMessage, PacketError> {
        let payload_length = self
            .bth
            .get_packet_real_length(buf_size.wrapping_sub(size_of::<Self>()));
        Ok(RdmaMessage {
            meta_data: Metadata::General(RdmaGeneralMeta::new_from_packet(
                &self.bth, &self.reth, None, None,
            )?),
            payload: PayloadInfo::new_with_data(self.get_data_ptr(), payload_length),
        })
    }

    fn set_from_rdma_message(&mut self, message: &RdmaMessage) -> Result<usize, PacketError> {
        match &message.meta_data {
            Metadata::General(header) => {
                self.bth
                    .set_from_common_meta(&header.common_meta, message.payload.get_pad_cnt());
                self.reth.set_from_reth_header(&header.reth);
                Ok(size_of::<Self>())
            }
            Metadata::Acknowledge(_) => Err(PacketError::InvalidMetadataType),
        }
    }
}

/// A composite packet header layout that contains the BTH and two RETHs
#[repr(C, packed)]
pub(crate) struct RdmaHeaderReqBthDoubleReth {
    pub(crate) bth: BTH,
    pub(crate) reth: RETH,
    pub(crate) secondary_reth: RETH,
}

impl RdmaPacketHeader for RdmaHeaderReqBthDoubleReth {
    fn to_rdma_message(&self, buf_size: usize) -> Result<RdmaMessage, PacketError> {
        let payload_length = self
            .bth
            .get_packet_real_length(buf_size.wrapping_sub(size_of::<Self>()));
        Ok(RdmaMessage {
            meta_data: Metadata::General(RdmaGeneralMeta::new_from_packet(
                &self.bth,
                &self.reth,
                None,
                Some(&self.secondary_reth),
            )?),
            payload: PayloadInfo::new_with_data(self.get_data_ptr(), payload_length),
        })
    }

    fn set_from_rdma_message(&mut self, message: &RdmaMessage) -> Result<usize, PacketError> {
        match &message.meta_data {
            Metadata::General(header) => {
                self.bth
                    .set_from_common_meta(&header.common_meta, message.payload.get_pad_cnt());
                self.reth.set_from_reth_header(&header.reth);
                let sec_reth = header
                    .secondary_reth
                    .as_ref()
                    .ok_or(PacketError::InvalidMetadataType)?;
                self.secondary_reth.set_from_reth_header(sec_reth);
                Ok(size_of::<Self>())
            }
            Metadata::Acknowledge(_) => Err(PacketError::InvalidMetadataType),
        }
    }
}

/// A composite packet header layout that contains the BTH, the RETH and the Immediate.
#[repr(C)]
pub(crate) struct RdmaHeaderReqBthRethImm {
    pub(crate) bth: BTH,
    pub(crate) reth: RETH,
    pub(crate) imm: Immediate,
}

impl RdmaPacketHeader for RdmaHeaderReqBthRethImm {
    fn to_rdma_message(&self, buf_size: usize) -> Result<RdmaMessage, PacketError> {
        let payload_length = self
            .bth
            .get_packet_real_length(buf_size.wrapping_sub(size_of::<Self>()));
        Ok(RdmaMessage {
            meta_data: Metadata::General(RdmaGeneralMeta::new_from_packet(
                &self.bth,
                &self.reth,
                Some(&self.imm),
                None,
            )?),
            payload: PayloadInfo::new_with_data(self.get_data_ptr(), payload_length),
        })
    }

    fn set_from_rdma_message(&mut self, message: &RdmaMessage) -> Result<usize, PacketError> {
        match &message.meta_data {
            Metadata::General(header) => {
                self.bth
                    .set_from_common_meta(&header.common_meta, message.payload.get_pad_cnt());
                self.reth.set_from_reth_header(&header.reth);
                self.imm
                    .set(header.imm.ok_or(PacketError::InvalidMetadataType)?);
                Ok(size_of::<Self>())
            }
            Metadata::Acknowledge(_) => Err(PacketError::InvalidMetadataType),
        }
    }
}

/// A composite packet header layout that contains the BTH and the AETH.
#[repr(C, packed)]
pub(crate) struct RdmaHeaderRespBthAeth {
    pub(crate) bth: BTH,
    pub(crate) aeth: AETH,
}

impl RdmaPacketHeader for RdmaHeaderRespBthAeth {
    fn to_rdma_message(&self, buf_size: usize) -> Result<RdmaMessage, PacketError> {
        let payload_length = self
            .bth
            .get_packet_real_length(buf_size.wrapping_sub(size_of::<Self>()));
        Ok(RdmaMessage {
            meta_data: Metadata::Acknowledge(AethHeader::new_from_packet(&self.bth, &self.aeth)?),
            payload: PayloadInfo::new_with_data(self.get_data_ptr(), payload_length),
        })
    }

    fn set_from_rdma_message(&mut self, message: &RdmaMessage) -> Result<usize, PacketError> {
        match &message.meta_data {
            Metadata::Acknowledge(header) => {
                self.bth
                    .set_from_common_meta(&header.common_meta, message.payload.get_pad_cnt());
                self.aeth
                    .set_aeth_code_and_value(header.aeth_code.clone() as u8, header.aeth_value);
                self.aeth.set_msn(header.msn);
                Ok(size_of::<Self>())
            }
            Metadata::General(_) => Err(PacketError::InvalidMetadataType),
        }
    }
}

pub(crate) type RdmaWriteFirstHeader = RdmaHeaderReqBthReth;
pub(crate) type RdmaWriteMiddleHeader = RdmaHeaderReqBthReth;
pub(crate) type RdmaWriteLastHeader = RdmaHeaderReqBthReth;
pub(crate) type RdmaWriteLastWithImmediateHeader = RdmaHeaderReqBthRethImm;
pub(crate) type RdmaWriteOnlyHeader = RdmaHeaderReqBthReth;
pub(crate) type RdmaWriteOnlyWithImmediateHeader = RdmaHeaderReqBthRethImm;
pub(crate) type RdmaReadRequestHeader = RdmaHeaderReqBthDoubleReth;
pub(crate) type RdmaReadResponseFirstHeader = RdmaHeaderReqBthReth;
pub(crate) type RdmaReadResponseMiddleHeader = RdmaHeaderReqBthReth;
pub(crate) type RdmaReadResponseLastHeader = RdmaHeaderReqBthReth;
pub(crate) type RdmaReadResponseOnlyHeader = RdmaHeaderReqBthReth;
pub(crate) type RdmaAcknowledgeHeader = RdmaHeaderRespBthAeth;

/// The IPv4 header
#[derive(Clone, Copy)]
#[repr(C, packed)]
pub(crate) struct Ipv4Header {
    pub(crate) version_header_len: u8, // version and header length
    pub(crate) dscp_ecn: u8,           // dscp and ecn
    total_length: [u8; 2],
    identification: [u8; 2],
    flags_fragment_offset: [u8; 2],
    pub(crate) ttl: u8,
    pub(crate) protocol: u8,
    checksum: [u8; 2],
    source: [u8; 4],
    destination: [u8; 4],
}

impl Ipv4Header {
    /// set default `version_header_len`,`dscp_ecn`,`ttl` and `protocol`.
    pub(crate) fn set_default_header(&mut self) {
        self.version_header_len = IPV4_DEFAULT_VERSION_AND_HEADER_LENGTH;
        self.dscp_ecn = IPV4_DEFAULT_DSCP_AND_ECN;
        self.ttl = IPV4_DEFAULT_TTL;
        self.protocol = IPV4_PROTOCOL_UDP;
    }

    pub(crate) fn set_total_length(&mut self, length: u16) {
        self.total_length = length.to_be_bytes();
    }
    pub(crate) fn set_identification(&mut self, id: u16) {
        self.identification = id.to_be_bytes();
    }
    pub(crate) fn set_flags_fragment_offset(&mut self, flags: u16) {
        self.flags_fragment_offset = flags.to_be_bytes();
    }
    pub(crate) fn set_checksum(&mut self, checksum: u16) {
        self.checksum = checksum.to_be_bytes();
    }
    pub(crate) fn set_source(&mut self, source: Ipv4Addr) {
        self.source = source.octets();
    }
    pub(crate) fn set_destination(&mut self, destination: Ipv4Addr) {
        self.destination = destination.octets();
    }
}

/// The UDP Header
#[derive(Clone, Copy)]
#[repr(C, packed)]
pub(crate) struct UdpHeader {
    source_port: [u8; 2],
    dest_port: [u8; 2],
    length: [u8; 2],
    checksum: [u8; 2],
}

impl UdpHeader {
    pub(crate) fn set_source_port(&mut self, port: u16) {
        self.source_port = port.to_be_bytes();
    }

    pub(crate) fn set_dest_port(&mut self, port: u16) {
        self.dest_port = port.to_be_bytes();
    }

    pub(crate) fn set_length(&mut self, length: u16) {
        self.length = length.to_be_bytes();
    }

    pub(crate) fn set_checksum(&mut self, checksum: u16) {
        self.checksum = checksum.to_be_bytes();
    }
}

/// A composite packet header layout that contains the Ipv4 header and the Udp header.
#[derive(Clone, Copy)]
#[repr(C, packed)]
pub(crate) struct IpUdpHeaders {
    pub(crate) ip_header: Ipv4Header,
    pub(crate) udp_header: UdpHeader,
}

impl IpUdpHeaders {
    #[allow(clippy::transmute_ptr_to_ref)]
    pub(crate) fn from_bytes(bytes: &[u8]) -> &'static mut Self {
        unsafe { transmute(bytes.as_ptr()) }
    }
}

/// A composite packet layout that contains the Ipv4 header, the Udp header and the BTH.
/// The packet may contains the RETH or the AETH, but for ICRC computation, we don't need to include them.
#[derive(Clone, Copy)]
#[repr(C, packed)]
pub(crate) struct CommonPacketHeader {
    pub(crate) net_header: IpUdpHeaders,
    pub(crate) bth_header: BTH,
}

impl CommonPacketHeader {
    #[allow(clippy::transmute_ptr_to_ref)]
    pub(crate) fn from_bytes(bytes: &[u8]) -> &'static mut Self {
        unsafe { transmute(bytes.as_ptr()) }
    }
}

#[allow(clippy::module_name_repetitions)]
#[derive(Error, Debug)]
pub(crate) enum PacketError {
    #[error("Header gets an invalid opcode")]
    InvalidOpcode,
    #[error("Convert ToHostWorkRbDescTransType failed")]
    FailedToConvertTransType,
    #[error("Convert ToHostWorkRbDescOpcode failed")]
    FailedToConvertRdmaOpcode(#[from] num_enum::TryFromPrimitiveError<ToHostWorkRbDescOpcode>),
    #[error("Convert ToHostWorkRbDescAethCode failed")]
    FailedToConvertAethCode(#[from] num_enum::TryFromPrimitiveError<ToHostWorkRbDescAethCode>),
    #[error("Invalid Metadata type")]
    InvalidMetadataType,
}

impl From<QpType> for ToHostWorkRbDescTransType {
    fn from(ty: QpType) -> ToHostWorkRbDescTransType {
        match ty {
            QpType::Uc => ToHostWorkRbDescTransType::Uc,
            QpType::Ud => ToHostWorkRbDescTransType::Ud,
            QpType::RawPacket | QpType::Rc => ToHostWorkRbDescTransType::Rc,
            QpType::XrcSend | QpType::XrcRecv => ToHostWorkRbDescTransType::Xrc,
        }
    }
}
