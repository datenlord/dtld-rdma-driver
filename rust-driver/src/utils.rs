use crate::types::Pmtu;

/// Get the length of the first packet.
///
/// A buffer will be divided into multiple packets if any slice is crossed the boundary of pmtu
/// For example, if pmtu = 256 and va = 254, then the first packet can be at most 2 bytes.
/// If pmtu = 256 and va = 256, then the first packet can be at most 256 bytes.
#[inline]
pub(crate) fn get_first_packet_max_length(va: u64, pmtu: u32) -> u32 {
    // The offset is smaller than pmtu, which is smaller than 4096 currently.
    #[allow(clippy::cast_possible_truncation, clippy::arithmetic_side_effects)]
    let offset = (va.wrapping_rem(u64::from(pmtu))) as u32;

    pmtu.wrapping_sub(offset)
}

/// Calculate that how many packets needs
pub(crate) fn calculate_packet_cnt(pmtu: Pmtu, raddr: u64, total_len: u32) -> u32 {
    let first_pkt_max_len = get_first_packet_max_length(raddr, u32::from(pmtu));
    let first_pkt_len = total_len.min(first_pkt_max_len);

    1u32.wrapping_add((total_len.wrapping_sub(first_pkt_len)).div_ceil(u32::from(pmtu)))
}

/// convert an u8 slice to a u64
pub(crate) fn u8_slice_to_u64(slice: &[u8]) -> u64 {
    // this operation convert a [u8;8] to a u64. So it's safe to left shift
    slice
        .iter()
        .take(8)
        .fold(0, |a, b| (a << 8_i32).wrapping_add(u64::from(*b)))
}

#[cfg(test)]
mod tests {
    use crate::types::Pmtu;

    use super::{calculate_packet_cnt, get_first_packet_max_length, u8_slice_to_u64};

    #[test]
    fn test_calculate_packet_cnt() {
        let raddr = 0;
        let total_len = 4096;
        let packet_cnt = calculate_packet_cnt(Pmtu::Mtu1024, raddr, total_len);
        assert_eq!(packet_cnt, 4);

        for raddr in 1..1023 {
            let packet_cnt = calculate_packet_cnt(Pmtu::Mtu1024, raddr, total_len);
            assert_eq!(packet_cnt, 5);
        }
    }

    #[test]
    fn test_get_first_packet_max_length() {
        for i in 0..4096 {
            assert_eq!(get_first_packet_max_length(i, 4096), (4096 - i) as u32);
        }
    }

    #[test]
    fn test_u8_slice_to_u64() {
        let input = [0x2, 0x3, 0x4, 0x5];
        assert_eq!(u8_slice_to_u64(&input), 0x02030405);
        let input = [0x2, 0x3, 0x4, 0x5, 0x06, 0x07];
        assert_eq!(u8_slice_to_u64(&input), 0x020304050607);
        let input = [0x2, 0x3, 0x4, 0x5, 0x06, 0x07, 0x08, 0x09];
        assert_eq!(u8_slice_to_u64(&input), 0x0203040506070809);
        let input = [0x2, 0x3, 0x4, 0x5, 0x06, 0x07, 0x08, 0x09, 0x10];
        assert_eq!(u8_slice_to_u64(&input), 0x0203040506070809);
    }
}
