use std::{mem::size_of, net::Ipv4Addr};

use thiserror::Error;

use crate::device::ToHostWorkRbDescOpcode;

use super::{
    packet::{
        CommonPacketHeader, IpUdpHeaders, Ipv4Header, PacketError, RdmaAcknowledgeHeader,
        RdmaPacketHeader, RdmaReadRequestHeader, RdmaReadResponseFirstHeader,
        RdmaReadResponseLastHeader, RdmaReadResponseMiddleHeader, RdmaReadResponseOnlyHeader,
        RdmaWriteFirstHeader, RdmaWriteLastHeader, RdmaWriteLastWithImmediateHeader,
        RdmaWriteMiddleHeader, RdmaWriteOnlyHeader, RdmaWriteOnlyWithImmediateHeader, BTH,
        ICRC_SIZE,
    },
    types::RdmaMessage,
};

pub(crate) struct PacketProcessor;

impl PacketProcessor {
    pub(crate) fn to_rdma_message(buf: &[u8]) -> Result<RdmaMessage, PacketError> {
        let opcode = ToHostWorkRbDescOpcode::try_from(BTH::from_bytes(buf).get_opcode());
        match opcode {
            Ok(ToHostWorkRbDescOpcode::RdmaWriteFirst) => {
                let header = RdmaWriteFirstHeader::from_bytes(buf);
                Ok(header.to_rdma_message(buf.len())?)
            }
            Ok(ToHostWorkRbDescOpcode::RdmaWriteMiddle) => {
                let header = RdmaWriteMiddleHeader::from_bytes(buf);
                Ok(header.to_rdma_message(buf.len())?)
            }
            Ok(ToHostWorkRbDescOpcode::RdmaWriteLast) => {
                let header = RdmaWriteLastHeader::from_bytes(buf);
                Ok(header.to_rdma_message(buf.len())?)
            }
            Ok(ToHostWorkRbDescOpcode::RdmaWriteLastWithImmediate) => {
                let header = RdmaWriteLastWithImmediateHeader::from_bytes(buf);
                Ok(header.to_rdma_message(buf.len())?)
            }
            Ok(ToHostWorkRbDescOpcode::RdmaWriteOnly) => {
                let header = RdmaWriteOnlyHeader::from_bytes(buf);
                Ok(header.to_rdma_message(buf.len())?)
            }
            Ok(ToHostWorkRbDescOpcode::RdmaWriteOnlyWithImmediate) => {
                let header = RdmaWriteOnlyWithImmediateHeader::from_bytes(buf);
                Ok(header.to_rdma_message(buf.len())?)
            }
            Ok(ToHostWorkRbDescOpcode::RdmaReadRequest) => {
                let header = RdmaReadRequestHeader::from_bytes(buf);
                Ok(header.to_rdma_message(buf.len())?)
            }
            Ok(ToHostWorkRbDescOpcode::RdmaReadResponseFirst) => {
                let header = RdmaReadResponseFirstHeader::from_bytes(buf);
                Ok(header.to_rdma_message(buf.len())?)
            }
            Ok(ToHostWorkRbDescOpcode::RdmaReadResponseMiddle) => {
                let header = RdmaReadResponseMiddleHeader::from_bytes(buf);
                Ok(header.to_rdma_message(buf.len())?)
            }
            Ok(ToHostWorkRbDescOpcode::RdmaReadResponseLast) => {
                let header = RdmaReadResponseLastHeader::from_bytes(buf);
                Ok(header.to_rdma_message(buf.len())?)
            }
            Ok(ToHostWorkRbDescOpcode::RdmaReadResponseOnly) => {
                let header = RdmaReadResponseOnlyHeader::from_bytes(buf);
                Ok(header.to_rdma_message(buf.len())?)
            }
            Ok(ToHostWorkRbDescOpcode::Acknowledge) => {
                let header = RdmaAcknowledgeHeader::from_bytes(buf);
                Ok(header.to_rdma_message(buf.len())?)
            }
            Err(_) => Err(PacketError::InvalidOpcode),
        }
    }

    pub(crate) fn set_from_rdma_message(
        buf: &mut [u8],
        message: &RdmaMessage,
    ) -> Result<usize, PacketError> {
        match message.meta_data.get_opcode() {
            ToHostWorkRbDescOpcode::RdmaWriteFirst => {
                let header = RdmaWriteFirstHeader::from_bytes(buf);
                Ok(header.set_from_rdma_message(message)?)
            }
            ToHostWorkRbDescOpcode::RdmaWriteMiddle => {
                let header = RdmaWriteMiddleHeader::from_bytes(buf);
                Ok(header.set_from_rdma_message(message)?)
            }
            ToHostWorkRbDescOpcode::RdmaWriteLast => {
                let header = RdmaWriteLastHeader::from_bytes(buf);
                Ok(header.set_from_rdma_message(message)?)
            }
            ToHostWorkRbDescOpcode::RdmaWriteLastWithImmediate => {
                let header = RdmaWriteLastWithImmediateHeader::from_bytes(buf);
                Ok(header.set_from_rdma_message(message)?)
            }
            ToHostWorkRbDescOpcode::RdmaWriteOnly => {
                let header = RdmaWriteOnlyHeader::from_bytes(buf);
                Ok(header.set_from_rdma_message(message)?)
            }
            ToHostWorkRbDescOpcode::RdmaWriteOnlyWithImmediate => {
                let header = RdmaWriteOnlyWithImmediateHeader::from_bytes(buf);
                Ok(header.set_from_rdma_message(message)?)
            }
            ToHostWorkRbDescOpcode::RdmaReadRequest => {
                let header = RdmaReadRequestHeader::from_bytes(buf);
                Ok(header.set_from_rdma_message(message)?)
            }
            ToHostWorkRbDescOpcode::RdmaReadResponseFirst => {
                let header = RdmaReadResponseFirstHeader::from_bytes(buf);
                Ok(header.set_from_rdma_message(message)?)
            }
            ToHostWorkRbDescOpcode::RdmaReadResponseMiddle => {
                let header = RdmaReadResponseMiddleHeader::from_bytes(buf);
                Ok(header.set_from_rdma_message(message)?)
            }
            ToHostWorkRbDescOpcode::RdmaReadResponseLast => {
                let header = RdmaReadResponseLastHeader::from_bytes(buf);
                Ok(header.set_from_rdma_message(message)?)
            }
            ToHostWorkRbDescOpcode::RdmaReadResponseOnly => {
                let header = RdmaReadResponseOnlyHeader::from_bytes(buf);
                Ok(header.set_from_rdma_message(message)?)
            }
            ToHostWorkRbDescOpcode::Acknowledge => {
                let header = RdmaAcknowledgeHeader::from_bytes(buf);
                Ok(header.set_from_rdma_message(message)?)
            }
        }
    }
}

#[allow(variant_size_differences)]
#[derive(Error, Debug)]
pub(crate) enum PacketProcessorError {
    #[error("missing src_addr")]
    MissingSrcAddr,
    #[error("missing src_port")]
    MissingSrcPort,
    #[error("missing dest_addr")]
    MissingDestAddr,
    #[error("missing dest_port")]
    MissingDestPort,
    #[error("missing message")]
    MissingMessage,
    #[error("missing ip identification")]
    MissingIpId,
    #[error("Needs a buffer of at least {0} bytes")]
    BufferNotLargeEnough(usize),
    #[error("packet error")]
    PacketError(#[from] PacketError),
    #[error("Length too long :{0}")]
    LengthTooLong(usize),
}

/// A builder for writing a packet
pub(crate) struct PacketWriter<'buf, 'message> {
    buf: &'buf mut [u8],
    src_addr: Option<Ipv4Addr>,
    src_port: Option<u16>,
    dest_addr: Option<Ipv4Addr>,
    dest_port: Option<u16>,
    message: Option<&'message RdmaMessage>,
    ip_id: Option<u16>,
}

impl<'buf, 'message> PacketWriter<'buf, 'message> {
    pub(crate) fn new(buf: &'buf mut [u8]) -> Self {
        Self {
            buf,
            src_addr: None,
            src_port: None,
            dest_addr: None,
            dest_port: None,
            message: None,
            ip_id: None,
        }
    }

    pub(crate) fn src_addr(&mut self, addr: Ipv4Addr) -> &mut Self {
        let new = self;
        new.src_addr = Some(addr);
        new
    }

    pub(crate) fn src_port(&mut self, port: u16) -> &mut Self {
        let new = self;
        new.src_port = Some(port);
        new
    }

    pub(crate) fn dest_addr(&mut self, addr: Ipv4Addr) -> &mut Self {
        let new = self;
        new.dest_addr = Some(addr);
        new
    }

    pub(crate) fn dest_port(&mut self, port: u16) -> &mut Self {
        let new = self;
        new.dest_port = Some(port);
        new
    }

    pub(crate) fn ip_id(&mut self, id: u16) -> &mut Self {
        let new = self;
        new.ip_id = Some(id);
        new
    }

    pub(crate) fn message(&mut self, message: &'message RdmaMessage) -> &mut Self {
        let new = self;
        new.message = Some(message);
        new
    }

    pub(crate) fn write(&mut self) -> Result<usize, PacketProcessorError> {
        // advance `size_of::<IpUdpHeaders>()` to write the rdma header
        let net_packet_offset = size_of::<IpUdpHeaders>();
        let message = self.message.ok_or(PacketProcessorError::MissingMessage)?;
        // write the rdma header
        let rdma_header_buf = self.buf.get_mut(net_packet_offset..).ok_or(
            PacketProcessorError::BufferNotLargeEnough(net_packet_offset),
        )?;
        let rdma_header_length = PacketProcessor::set_from_rdma_message(rdma_header_buf, message)?;

        // get the total length(include the ip,udp header and the icrc)
        let total_length = size_of::<IpUdpHeaders>()
            .wrapping_add(rdma_header_length)
            .wrapping_add(message.payload.with_pad_length())
            .wrapping_add(ICRC_SIZE);
        let total_length_in_u16 = u16::try_from(total_length)
            .map_err(|_| PacketProcessorError::LengthTooLong(total_length))?;

        // write the payload
        let header_offset = size_of::<IpUdpHeaders>().wrapping_add(rdma_header_length);
        let header_buf =
            self.buf
                .get_mut(header_offset..)
                .ok_or(PacketProcessorError::BufferNotLargeEnough(
                    net_packet_offset,
                ))?;
        message.payload.copy_to(header_buf.as_mut_ptr());

        // write the ip,udp header
        let ip_id = self.ip_id.ok_or(PacketProcessorError::MissingIpId)?;
        let src_addr = self.src_addr.ok_or(PacketProcessorError::MissingSrcAddr)?;
        let src_port = self.src_port.ok_or(PacketProcessorError::MissingSrcPort)?;
        let dest_addr = self
            .dest_addr
            .ok_or(PacketProcessorError::MissingDestAddr)?;
        let dest_port = self
            .dest_port
            .ok_or(PacketProcessorError::MissingDestPort)?;
        write_ip_udp_header(
            self.buf,
            src_addr,
            src_port,
            dest_addr,
            dest_port,
            total_length_in_u16,
            ip_id,
        );
        // compute icrc
        let icrc_buf = self
            .buf
            .get(0..total_length)
            .ok_or(PacketProcessorError::BufferNotLargeEnough(total_length))?;
        if total_length < ICRC_SIZE {
            return Err(PacketProcessorError::BufferNotLargeEnough(ICRC_SIZE));
        }
        let icrc = compute_icrc(icrc_buf).to_le_bytes();
        #[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
        // we have checked it is large enough
        self.buf[total_length - ICRC_SIZE..total_length].copy_from_slice(&icrc);
        Ok(total_length)
    }
}

/// Assume the buffer is a packet, compute the icrc
/// Return a u32 of the icrc
///
/// # Panic
/// The function made an assumption that the buffer is a valid RDMA packet, in other words,
/// it should at least contain the common header, ip header, udp header, bth header and the icrc.
#[allow(clippy::indexing_slicing)]
pub(crate) fn compute_icrc(data: &[u8]) -> u32 {
    let mut hasher = crc32fast::Hasher::new();
    let prefix = [0xffu8; 8];
    hasher.update(&prefix);

    let mut common_hdr = *CommonPacketHeader::from_bytes(data);

    common_hdr.net_header.ip_header.dscp_ecn = 0xff;
    common_hdr.net_header.ip_header.ttl = 0xff;
    common_hdr.net_header.ip_header.set_checksum(0xffff);
    common_hdr.net_header.udp_header.set_checksum(0xffff);
    common_hdr.bth_header.fill_ecn_and_resv6();

    // convert common_hdr to bytes
    // SAFETY: the length is ensured
    let common_hdr_bytes = unsafe {
        std::slice::from_raw_parts(
            std::ptr::addr_of!(common_hdr).cast::<u8>(),
            size_of::<CommonPacketHeader>(),
        )
    };
    hasher.update(common_hdr_bytes);
    // the rest of header and payload
    hasher.update(&data[size_of::<CommonPacketHeader>()..data.len().wrapping_sub(ICRC_SIZE)]);

    hasher.finalize()
}

/// Write the ip and udp header to the buffer
///
/// # Panic
/// the buffer should be large enough to hold the ip and udp header
pub(crate) fn write_ip_udp_header(
    buf: &mut [u8],
    src_addr: Ipv4Addr,
    src_port: u16,
    dest_addr: Ipv4Addr,
    dest_port: u16,
    total_length: u16,
    ip_identification: u16,
) {
    let common_hdr = IpUdpHeaders::from_bytes(buf);
    common_hdr.ip_header.set_default_header();
    common_hdr.ip_header.set_source(src_addr);
    common_hdr.ip_header.set_destination(dest_addr);
    common_hdr.ip_header.set_total_length(total_length);
    common_hdr.ip_header.set_flags_fragment_offset(0);
    common_hdr.ip_header.set_identification(ip_identification);
    common_hdr.ip_header.set_checksum(0);

    common_hdr.udp_header.set_source_port(src_port);
    common_hdr.udp_header.set_dest_port(dest_port);
    #[allow(clippy::cast_possible_truncation)]
    common_hdr
        .udp_header
        .set_length(total_length.wrapping_sub(size_of::<Ipv4Header>() as u16));
    common_hdr.udp_header.set_checksum(0);
}

/// Assume the buffer is a packet, check if the icrc is valid
/// Return a bool if the icrc is valid
///
/// # Panic
/// The function made an assumption that the buffer is a valid RDMA packet, in other words,
/// it should at least contain the common header, ip header, udp header, bth header and the icrc.
#[allow(clippy::indexing_slicing)]
pub(crate) fn is_icrc_valid(received_data: &mut [u8]) -> Result<bool, PacketProcessorError> {
    let length = received_data.len();
    // chcek the icrc
    let icrc_array: [u8; 4] = match received_data[length.wrapping_sub(ICRC_SIZE)..length].try_into() {
        Ok(arr) => arr,
        #[allow(clippy::cast_possible_truncation)]
        Err(_) => return Err(PacketProcessorError::BufferNotLargeEnough(ICRC_SIZE)),
    };
    let origin_icrc = u32::from_le_bytes(icrc_array);
    received_data[length.wrapping_sub(ICRC_SIZE)..length].copy_from_slice(&[0u8; 4]);
    let our_icrc = compute_icrc(received_data);
    Ok(our_icrc == origin_icrc)
}

#[cfg(test)]
mod tests {
    use crate::device::software::packet_processor::compute_icrc;

    #[test]
    fn test_computing_icrc() {
        // The buffer is a packet in hex format:
        // IP(id=54321, frag=0,protocol= \
        //     ttl=128, dst="127.0.0.1", src="127.0.0.1", len=108)/ \
        //     UDP(sport=49152, dport=4791, len=88)/ \
        //     BTH(opcode='RC_RDMA_WRITE_MIDDLE',pkey=0x1, dqpn=3, psn=0)/ \
        //     Raw(bytes([0]*64))
        let buf = [
            0x45, 0x00, 0x00, 0xbc, 0x00, 0x01, 0x00, 0x00, 0x40, 0x11, 0xf8, 0xda, 0xc0, 0xa8,
            0x00, 0x02, 0xc0, 0xa8, 0x00, 0x03, 0x12, 0xb7, 0x12, 0xb7, 0x00, 0xa8, 0x00, 0x00,
            0x0a, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x7f, 0x7e, 0x91, 0x00, 0x00, 0x00, 0x01, 0x70, 0x9a, 0x33, 0x00, 0x00, 0x00, 0x80,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xc6, 0x87, 0x22, 0x98,
        ];
        let icrc = compute_icrc(&buf);
        assert!(
            icrc == u32::from_le_bytes([0xc6, 0x87, 0x22, 0x98]),
            "icrc: {:x}",
            icrc
        );

        let buf = [
            69, 0, 0, 0, 0, 0, 0, 0, 64, 17, 124, 232, 127, 0, 0, 3, 127, 0, 0, 2, 18, 183, 18,
            183, 0, 32, 0, 0, 17, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0,
        ];
        let icrc = compute_icrc(&buf);
        assert_eq!(icrc, u32::from_le_bytes([64, 33, 163, 207]));
    }
}
