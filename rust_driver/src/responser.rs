use std::net::Ipv4Addr;

use crate::buf::{Slot, RDMA_ACK_BUFFER_SLOT_SIZE};
use crate::device::layout::{Aeth, Bth, Ipv4, Mac, NReth, Udp};
use crate::qp::QpContext;
use crate::types::{Imm, Key, Msn, Psn, QpType, Qpn, WorkReqSendFlag};
use eui48::MacAddress;

use crate::device::{
    ToCardWorkRbDesc, ToCardWorkRbDescBuilder, ToCardWorkRbDescCommon, ToCardWorkRbDescOpcode,
    ToHostWorkRbDescAethCode, ToHostWorkRbDescOpcode, ToHostWorkRbDescRead,
};
use crate::utils::calculate_packet_cnt;
use crate::{Error, Sge, ThreadSafeHashmap};

/// make an ack packet in the buffer, and return a work descriptor
///
/// The slot can be allocated by `PacketBuf::recycle_buf`
pub(crate) fn make_ack(
    ack_buf: Slot<RDMA_ACK_BUFFER_SLOT_SIZE>,
    qp_table: &ThreadSafeHashmap<Qpn, QpContext>,
    qpn: Qpn,
    msn: Msn,
    psn: Psn,
) -> Result<Box<ToCardWorkRbDesc>, Error> {
    make_ack_or_nack(ack_buf, qp_table, qpn, msn, psn, None)
}

/// make a nack packet in the buffer, and return a work descriptor
///
/// The slot can be allocated by `PacketBuf::recycle_buf`
pub(crate) fn make_nack(
    ack_buf: Slot<RDMA_ACK_BUFFER_SLOT_SIZE>,
    qp_table: &ThreadSafeHashmap<Qpn, QpContext>,
    qpn: Qpn,
    msn: Msn,
    start_psn: Psn,
    end_psn: Psn,
) -> Result<Box<ToCardWorkRbDesc>, Error> {
    make_ack_or_nack(ack_buf, qp_table, qpn, msn, start_psn, Some(end_psn))
}

fn make_ack_or_nack(
    mut ack_buf: Slot<RDMA_ACK_BUFFER_SLOT_SIZE>,
    qp_table: &ThreadSafeHashmap<Qpn, QpContext>,
    qpn: Qpn,
    msn: Msn,
    psn: Psn,
    expected_psn: Option<Psn>,
) -> Result<Box<ToCardWorkRbDesc>, Error> {
    #[allow(clippy::unwrap_used)]
    let (src_mac, src_ip, dst_mac, dst_ip, common) = {
        let table = qp_table.read();
        if let Some(qp) = table.get(&qpn) {
            let dst_ip = qp.dqp_ip;
            let dst_mac = qp.dqp_mac_addr;
            let src_mac = qp.local_mac;
            let src_ip = qp.local_ip;
            #[allow(clippy::cast_possible_truncation)]
            let common = ToCardWorkRbDescCommon {
                total_len: ACKPACKET_SIZE as u32,
                rkey: Key::default(),
                raddr: 0,
                dqp_ip: dst_ip,
                dqpn: qpn,
                mac_addr: qp.dqp_mac_addr,
                pmtu: qp.pmtu,
                flags: WorkReqSendFlag::empty(),
                qp_type: QpType::RawPacket,
                psn: Psn::default(),
                msn,
            };
            (src_mac, src_ip, dst_mac, dst_ip, common)
        } else {
            return Err(Error::Invalid(format!("QP {qpn:?}")));
        }
    };
    write_packet(
        ack_buf.as_mut_slice(),
        (src_mac, src_ip),
        (dst_mac, dst_ip),
        qpn,
        msn,
        psn,
        expected_psn,
    );
    #[allow(clippy::cast_possible_truncation)]
    let sge = ack_buf.into_sge(ACKPACKET_SIZE as u32);
    ToCardWorkRbDescBuilder::new(ToCardWorkRbDescOpcode::WriteWithImm)
        .with_common(common)
        .with_sge(sge)
        .with_imm(Imm::default())
        .build()
}

/// make a read response work descriptor
///
/// read_req is a reference to the read request descriptor
pub(crate) fn make_read_resp(
    qp_table: &ThreadSafeHashmap<Qpn, QpContext>,
    read_req: &ToHostWorkRbDescRead,
) -> Result<Box<ToCardWorkRbDesc>, Error> {
    let dqpn = read_req.common.dqpn;
    let msn = read_req.common.msn;
    let raddr = read_req.raddr;
    let rkey = read_req.rkey;
    let len = read_req.len;
    let common = if let Some(qp) = qp_table.read().get(&dqpn) {
        let mut common = ToCardWorkRbDescCommon {
            total_len: len,
            rkey,
            raddr,
            dqp_ip: qp.dqp_ip,
            dqpn,
            mac_addr: qp.dqp_mac_addr,
            pmtu: qp.pmtu,
            flags: Default::default(),
            qp_type: qp.qp_type,
            psn: Psn::default(),
            msn,
        };
        let packet_cnt = calculate_packet_cnt(qp.pmtu, raddr, len);
        let first_pkt_psn = {
            let mut send_psn = qp.sending_psn.lock();
            let first_pkt_psn = *send_psn;
            *send_psn = send_psn.wrapping_add(packet_cnt);
            first_pkt_psn
        };
        common.psn = first_pkt_psn;
        common
    } else {
        return Err(Error::Invalid(format!("QP {dqpn:?}")));
    };

    let sge = Sge {
        addr: read_req.laddr,
        len: read_req.len,
        key: read_req.lkey,
    };
    ToCardWorkRbDescBuilder::new(ToCardWorkRbDescOpcode::ReadResp)
        .with_common(common)
        .with_sge(sge)
        .build()
}

/// Covert the MAC address to a u64 in big endian
#[allow(clippy::indexing_slicing)]
fn mac_to_be64(mac: MacAddress) -> u64 {
    let mac = mac.as_bytes();
    u64::from_le_bytes([mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], 0, 0])
}

/// Write the IP header and UDP header
///
/// If the
/// # Panic
/// We assume the `buf` is large enough to hold the packet, in other words, the length should
/// at least be `ACKPACKET_SIZE`. If the length is less than `ACKPACKET_SIZE`, it will panic.
#[allow(clippy::indexing_slicing)]
fn write_packet(
    buf: &mut [u8],
    src: (MacAddress, Ipv4Addr),
    dst: (MacAddress, Ipv4Addr),
    dpqn: Qpn,
    msg_seq_num: Msn,
    psn: Psn,
    expected_psn: Option<Psn>,
) {
    let buf = &mut buf[..ACKPACKET_SIZE];
    let (src_mac, src_ip) = src;
    let (dst_mac, dst_ip) = dst;

    // write the mac header
    let mut mac_header = Mac(buf);
    // note that we write in big endian directly
    mac_header.set_src_mac_addr(mac_to_be64(src_mac));
    mac_header.set_dst_mac_addr(mac_to_be64(dst_mac));
    mac_header.set_network_layer_type(MAC_SERVICE_LAYER_IPV4.into());

    // write a ip header
    let mut ip_header = Ipv4(&mut mac_header.0[MAC_HEADER_SIZE..]);
    ip_header.set_version_and_len(u32::from(IP_DEFAULT_VERSION_AND_LEN));
    ip_header.set_dscp_ecn(0);

    // The `total_length` take a 16 bits **big-endian** number as input.
    // Only the third and forth bytes are used, so the we put the `ACKPACKET_SIZE` into the third byte.
    #[allow(clippy::cast_possible_truncation)]
    ip_header.set_total_length(u32::from_be_bytes([
        0,
        0,
        ACKPACKET_SIZE_WITHOUT_MAC as u8,
        0,
    ]));
    ip_header.set_identification(0x27);
    ip_header.set_fragment_offset(0);
    ip_header.set_ttl(u32::from(IP_DEFAULT_TTL));
    ip_header.set_protocol(u32::from(IP_DEFAULT_PROTOCOL));
    let src_addr: u32 = src_ip.into();
    ip_header.set_source(src_addr.to_be());
    let dst_addr: u32 = dst_ip.into();
    ip_header.set_destination(dst_addr.to_be());
    // Set the checksum to 0, and calculate the checksum later
    ip_header.set_checksum(0);
    let checksum = calculate_ipv4_checksum(ip_header.0).to_be();
    ip_header.set_checksum(checksum.into());

    let udp_buf = &mut mac_header.0[MAC_HEADER_SIZE + IPV4_HEADER_SIZE..];
    let mut udp_header = Udp(udp_buf);
    udp_header.set_src_port(RDMA_DEFAULT_PORT.to_be());
    udp_header.set_dst_port(RDMA_DEFAULT_PORT.to_be());
    #[allow(clippy::cast_possible_truncation)]
    udp_header.set_length((ACKPACKET_SIZE_WITHOUT_MAC_AND_IPV4 as u16).to_be());
    // It might redundant to calculate checksum, as the ICRC will calculate the another checksum
    udp_header.set_checksum(0);

    let bth_hdr_buf = &mut mac_header.0[MAC_HEADER_SIZE + IPV4_HEADER_SIZE + UDP_HEADER_SIZE..];
    let mut bth_header = Bth(bth_hdr_buf);
    bth_header.set_opcode(ToHostWorkRbDescOpcode::Acknowledge as u32);
    bth_header.set_pad_count(0);
    bth_header.set_pkey(0);
    bth_header.set_ecn_and_resv6(0);

    bth_header.set_dqpn(dpqn.into_be());
    bth_header.set_psn(psn.into_be());

    let is_nak = expected_psn.is_some();
    let aeth_hdr_buf =
        &mut mac_header.0[MAC_HEADER_SIZE + IPV4_HEADER_SIZE + UDP_HEADER_SIZE + BTH_HEADER_SIZE..];
    let mut aeth_header = Aeth(aeth_hdr_buf);
    if is_nak {
        aeth_header.set_aeth_code(ToHostWorkRbDescAethCode::Nak as u32);
    } else {
        aeth_header.set_aeth_code(ToHostWorkRbDescAethCode::Ack as u32);
    }
    aeth_header.set_aeth_value(0);
    aeth_header.set_msn(msg_seq_num.into_be().into());

    let mut nreth_header = NReth(
        &mut mac_header.0[MAC_HEADER_SIZE
            + IPV4_HEADER_SIZE
            + UDP_HEADER_SIZE
            + BTH_HEADER_SIZE
            + AETH_HEADER_SIZE..],
    );
    if is_nak {
        // we have checked the option before.
        #[allow(clippy::unwrap_used)]
        let expected_psn = expected_psn.unwrap().into_be();
        nreth_header.set_last_retry_psn(expected_psn);
    } else {
        nreth_header.set_last_retry_psn(0);
    }
    // calculate the ICRC
    let total_buf = &mut mac_header.0[..ACKPACKET_SIZE];
    let icrc = calculate_icrc(&total_buf[MAC_HEADER_SIZE..]);
    total_buf[ACKPACKET_SIZE - ICRC_SIZE..].copy_from_slice(&icrc.to_le_bytes());
}

const MAC_HEADER_SIZE: usize = 14;
const IPV4_HEADER_SIZE: usize = 20;
const UDP_HEADER_SIZE: usize = 8;
const BTH_HEADER_SIZE: usize = 12;
const IPV4_UDP_BTH_HEADER_SIZE: usize = IPV4_HEADER_SIZE + UDP_HEADER_SIZE + BTH_HEADER_SIZE;
const AETH_HEADER_SIZE: usize = 4;
const NRETH_HEADER_SIZE: usize = 4;
const ICRC_SIZE: usize = 4;
const ACKPACKET_SIZE_WITHOUT_MAC_AND_IPV4: usize =
    UDP_HEADER_SIZE + BTH_HEADER_SIZE + AETH_HEADER_SIZE + NRETH_HEADER_SIZE + ICRC_SIZE;

const ACKPACKET_SIZE_WITHOUT_MAC: usize = IPV4_HEADER_SIZE + ACKPACKET_SIZE_WITHOUT_MAC_AND_IPV4;

pub(crate)const ACKPACKET_SIZE: usize = MAC_HEADER_SIZE + ACKPACKET_SIZE_WITHOUT_MAC;
#[allow(clippy::assertions_on_constants)]
const _: () = assert!(
    RDMA_ACK_BUFFER_SLOT_SIZE >= ACKPACKET_SIZE,
    "RDMA_ACK_BUFFER_SLOT_SIZE too small"
);

const MAC_SERVICE_LAYER_IPV4: u16 = 8;
const IP_DEFAULT_VERSION_AND_LEN: u8 = 0x45;
const IP_DEFAULT_TTL: u8 = 64;
const IP_DEFAULT_PROTOCOL: u8 = 17;
const RDMA_DEFAULT_PORT: u16 = 4791;

/// Calculate the RDMA packet ICRC.
///
/// the `data` passing in should include the space for the ICRC(4 bytes).
///
/// # Panic
/// We assume the `data` is large enough to hold the RDMA packet, in other words, the length should
/// contain the space for IP header, UDP header, BTH header and the ICRC.
#[allow(clippy::indexing_slicing)]
fn calculate_icrc(data: &[u8]) -> u32 {
    let mut hasher = crc32fast::Hasher::new();
    let prefix = [0xffu8; 8];
    let mut buf = [0; IPV4_UDP_BTH_HEADER_SIZE];
    hasher.update(&prefix);

    buf.copy_from_slice(data[..IPV4_UDP_BTH_HEADER_SIZE].as_ref());
    let mut ip_header = Ipv4(&mut buf);
    ip_header.set_dscp_ecn(0xff);
    ip_header.set_ttl(0xff);
    ip_header.set_checksum(0xffff);

    let mut udp_header = Udp(&mut buf[IPV4_HEADER_SIZE..]);
    udp_header.set_checksum(0xffff);

    let mut bth_header = Bth(&mut buf[IPV4_HEADER_SIZE + UDP_HEADER_SIZE..]);
    bth_header.set_ecn_and_resv6(0xff);

    hasher.update(&buf);
    // the rest of header and payload
    #[allow(clippy::arithmetic_side_effects)]
    hasher.update(&data[IPV4_UDP_BTH_HEADER_SIZE..data.len() - ICRC_SIZE]);
    hasher.finalize()
}

/// Calculate the checksum of the IPv4 header
///
/// The `header` should be a valid IPv4 header, and the checksum field is set to 0.
///
/// # Panic
/// The header is considered as a standard IPv4 header, so the length should be 20 bytes.
/// Otherwise it will panic.
#[allow(
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
fn calculate_ipv4_checksum(header: &[u8]) -> u16 {
    let mut sum = 0u32;

    for i in (0..IPV4_HEADER_SIZE).step_by(2) {
        let word = if i + 1 < header.len() {
            (u16::from(header[i]) << 8_i32) | u16::from(header[i + 1])
        } else {
            u16::from(header[i]) << 8_i32
        };
        sum += u32::from(word);
    }

    while sum >> 16_i32 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16_i32);
    }

    !sum as u16
}

#[cfg(test)]
mod tests {

    use crate::responser::calculate_ipv4_checksum;

    use super::calculate_icrc;

    #[test]
    fn test_icrc_computing() {
        let packet = [
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
        let icrc = calculate_icrc(&packet);
        assert_eq!(icrc, u32::from_le_bytes([0xc6, 0x87, 0x22, 0x98]));
        let packet = [
            69, 0, 0, 0, 0, 0, 0, 0, 64, 17, 124, 232, 127, 0, 0, 3, 127, 0, 0, 2, 18, 183, 18,
            183, 0, 32, 0, 0, 17, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0,
        ];
        let icrc = calculate_icrc(&packet);
        assert_eq!(icrc, u32::from_le_bytes([64, 33, 163, 207]));
    }
    #[test]
    fn test_calculate_ipv4_checksum() {
        // capture from a real packet
        let ref_header: [u8; 20] = [
            0x45, 0x00, 0x00, 0x34, 0xb4, 0xef, 0x40, 0x00, 0x25, 0x06, 0x00, 0x00, 0x14, 0x59,
            0xed, 0x08, 0xac, 0x1b, 0xe8, 0xda,
        ];
        let expected_checksum: u16 = u16::from_be_bytes([0x0a, 0x7d]);
        let checksum = calculate_ipv4_checksum(&ref_header);
        assert_eq!(checksum, expected_checksum);
        let ref_header = [
            0x45, 0x00, 0x02, 0x0c, 0x3f, 0x3b, 0x40, 0x00, 0x29, 0x06, 0x00, 0x00, 0x8c, 0x52,
            0x72, 0x15, 0xac, 0x1b, 0xe8, 0xda,
        ];
        let expected_checksum: u16 = u16::from_be_bytes([0x7d, 0x53]);
        let checksum = calculate_ipv4_checksum(&ref_header);
        assert_eq!(checksum, expected_checksum);
        let ref_header = [
            0x45, 0x0, 0x0, 0xbc, 0x0, 0x1, 0x0, 0x0, 0x40, 0x11, 0x0, 0x0, 0xc0, 0xa8, 0x0, 0x2,
            0xc0, 0xa8, 0x0, 0x3,
        ];
        let checksum = calculate_ipv4_checksum(&ref_header);
        let expected_checksum: u16 = u16::from_be_bytes([0xf8, 0xda]);
        assert_eq!(checksum, expected_checksum);
    }

}
