use std::{
    alloc::{alloc, dealloc, Layout},
    fs::{File, OpenOptions},
    io,
    ops::{Deref, DerefMut, Index, IndexMut},
    os::fd::AsRawFd,
    path::Path,
    slice::from_raw_parts_mut,
};

use log::error;

use crate::types::{Pmtu, PAGE_SIZE};

/// Get the length of the first packet.
///
/// A buffer will be divided into multiple packets if any slice is crossed the boundary of pmtu
/// For example, if pmtu = 256 and va = 254, then the first packet can be at most 2 bytes.
/// If pmtu = 256 and va = 256, then the first packet can be at most 256 bytes.
#[inline]
#[allow(clippy::arithmetic_side_effects)]
pub(crate) fn get_first_packet_max_length(va: u64, pmtu: u32) -> u32 {
    // The offset is smaller than pmtu, which is smaller than 4096 currently.
    #[allow(clippy::cast_possible_truncation)]
    let offset = (va.wrapping_rem(u64::from(pmtu))) as u32;

    pmtu - offset
}

#[allow(clippy::arithmetic_side_effects)] // total_len must be greater or equal than first_pkt_len
pub(crate) fn calculate_packet_cnt(pmtu: Pmtu, raddr: u64, total_len: u32) -> u32 {
    let first_pkt_max_len = get_first_packet_max_length(raddr, u32::from(&pmtu));
    let first_pkt_len = total_len.min(first_pkt_max_len);

    1 + (total_len - first_pkt_len).div_ceil(u32::from(&pmtu))
}

#[allow(clippy::arithmetic_side_effects)]
pub(crate) fn u8_slice_to_u64(slice: &[u8]) -> u64 {
    // this operation convert a [u8;8] to a u64. So it's safe to left shift
    slice.iter().fold(0, |a, b| (a << 8_i32) + u64::from(*b))
}

#[allow(clippy::arithmetic_side_effects)]
pub(crate) fn align_up<const PAGE: usize>(addr: usize) -> usize {
    (((addr) + ((PAGE) - 1)) / PAGE) * PAGE
}

/// A struct to manage hugepage memory
#[derive(Debug)]
pub struct MmapMemory {
    size: usize,
    addr: usize,
}

impl MmapMemory {
    /// size of the huge page
    pub const HUGE_PAGE_SIZE: usize = 1024 * 1024 * 2;

    pub(crate) fn new_ringbuf<const RINGBUF_SIZE: usize>(
        device_file: &File,
        ringbuf_slot_offset: i64,
    ) -> io::Result<Self> {
        let device_file_fd = device_file.as_raw_fd();

        let ptr = unsafe {
            libc::mmap(
                core::ptr::null_mut(),
                RINGBUF_SIZE,
                libc::PROT_WRITE | libc::PROT_READ,
                libc::MAP_SHARED,
                device_file_fd,
                ringbuf_slot_offset,
            )
        };

        if ptr == libc::MAP_FAILED {
            log::error!("failed to allocate here {}", ringbuf_slot_offset);
            return Err(io::Error::last_os_error());
        }

        Ok(Self {
            size: RINGBUF_SIZE,
            addr: ptr as usize,
        })
    }
    /// # Errors
    ///
    pub fn new(size: usize) -> io::Result<Self> {
        let size = align_up::<{ Self::HUGE_PAGE_SIZE }>(size);
        let buffer = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_ANONYMOUS | libc::MAP_PRIVATE | libc::MAP_HUGETLB,
                -1,
                0,
            )
        };
        if buffer == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }

        let ret = unsafe { libc::mlock(buffer, size) };
        if ret != 0_i32 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            size,
            addr: buffer as usize,
        })
    }

    /// get raw pointer of huge page buffer
    #[must_use]
    pub fn as_ptr(&self) -> *const u8 {
        self.addr as *const u8
    }

    /// get size of the huge page buffer
    #[must_use]
    pub fn size(&self) -> usize {
        self.size
    }

    pub(crate) fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { from_raw_parts_mut(self.addr as *mut u8, self.size) }
    }

    pub(crate) fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.addr as *const u8, self.size) }
    }
}

impl AsRef<[u8]> for MmapMemory {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AsMut<[u8]> for MmapMemory {
    fn as_mut(&mut self) -> &mut [u8] {
        self.as_mut_slice()
    }
}

impl Drop for MmapMemory {
    fn drop(&mut self) {
        let result = unsafe { libc::munmap(self.addr as *mut libc::c_void, self.size) };
        if result != 0_i32 {
            error!("drop huge page failed: {result}");
        }
    }
}

fn allocate_aligned_memory(size: usize) -> io::Result<&'static mut [u8]> {
    let layout =
        Layout::from_size_align(size, PAGE_SIZE).map_err(|_| io::Error::last_os_error())?;
    let ptr = unsafe { alloc(layout) };
    Ok(unsafe { from_raw_parts_mut(ptr, size) })
}

#[allow(clippy::unwrap_used)]
fn deallocate_aligned_memory(buf: &mut [u8], size: usize) {
    let layout = Layout::from_size_align(size, PAGE_SIZE).unwrap();
    unsafe {
        dealloc(buf.as_mut_ptr(), layout);
    }
}

/// An aligned memory buffer.
#[derive(Debug)]
pub struct AlignedMemory(&'static mut [u8]);

impl AlignedMemory {
    /// # Errors
    /// Return an error if the size is too large.
    pub fn new(size: usize) -> io::Result<Self> {
        Ok(AlignedMemory(allocate_aligned_memory(size)?))
    }

    /// Length of the buffer
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize{
        self.0.len()
    }
}

impl AsRef<[u8]> for AlignedMemory {
    fn as_ref(&self) -> &[u8] {
        self.0
    }
}

impl AsMut<[u8]> for AlignedMemory {
    fn as_mut(&mut self) -> &mut [u8] {
        self.0
    }
}


impl Drop for AlignedMemory {
    fn drop(&mut self) {
        deallocate_aligned_memory(self.0, self.0.len());
    }
}

#[derive(Debug)]
pub(crate) enum Buffer {
    DmaBuffer(MmapMemory),
    AlignedMemory(AlignedMemory),
}

impl Buffer {
    pub(crate) fn new(size: usize, is_huge_page: bool) -> io::Result<Self> {
        if is_huge_page {
            MmapMemory::new(size).map(Buffer::DmaBuffer)
        } else {
            AlignedMemory::new(size).map(Buffer::AlignedMemory)
        }
    }

    pub(crate) fn size(&self) -> usize {
        match self {
            Buffer::DmaBuffer(huge_page) => huge_page.size(),
            Buffer::AlignedMemory(aligned_memory) => aligned_memory.0.len(),
        }
    }
}

impl Index<usize> for Buffer {
    type Output = u64;
    #[allow(
        clippy::ptr_as_ptr,
        clippy::cast_ptr_alignment,
        clippy::arithmetic_side_effects
    )]
    fn index(&self, index: usize) -> &u64 {
        match self {
            Buffer::DmaBuffer(buffer) => {
                let ptr = unsafe {
                    buffer.as_ptr().add(index * std::mem::size_of::<u64>()) as *const u64
                };
                unsafe { &*ptr }
            }
            Buffer::AlignedMemory(aligned_memory) => {
                let ptr = unsafe {
                    aligned_memory
                        .as_ref()
                        .as_ptr()
                        .add(index * std::mem::size_of::<u64>()) as *const u64
                };
                unsafe { &*ptr }
            }
        }
    }
}

impl IndexMut<usize> for Buffer {
    #[allow(
        clippy::ptr_as_ptr,
        clippy::cast_ptr_alignment,
        clippy::arithmetic_side_effects
    )]
    fn index_mut(&mut self, index: usize) -> &mut u64 {
        match self {
            Buffer::DmaBuffer(buffer) => {
                let ptr =
                    unsafe { buffer.as_ptr().add(index * std::mem::size_of::<u64>()) as *mut u64 };
                unsafe { &mut *ptr }
            }
            Buffer::AlignedMemory(aligned_memory) => {
                let ptr = unsafe {
                    aligned_memory
                        .as_mut()
                        .as_mut_ptr()
                        .add(index * std::mem::size_of::<u64>()) as *mut u64
                };
                unsafe { &mut *ptr }
            }
        }
    }
}

impl AsRef<[u8]> for Buffer {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::AlignedMemory(a) => a.as_ref(),
            Self::DmaBuffer(b) => b.as_ref(),
        }
    }
}

impl AsMut<[u8]> for Buffer {
    fn as_mut(&mut self) -> &mut [u8] {
        match self {
            Self::AlignedMemory(a) => a.as_mut(),
            Self::DmaBuffer(b) => b.as_mut(),
        }
    }
}

// pub(crate) struct Threadsafe

#[cfg(test)]
mod tests {
    use crate::types::Pmtu;

    use super::align_up;

    #[test]
    fn test_calculate_packet_cnt() {
        let raddr = 0;
        let total_len = 4096;
        let packet_cnt = super::calculate_packet_cnt(Pmtu::Mtu1024, raddr, total_len);
        assert_eq!(packet_cnt, 4);

        for raddr in 1..1023 {
            let packet_cnt = super::calculate_packet_cnt(Pmtu::Mtu1024, raddr, total_len);
            assert_eq!(packet_cnt, 5);
        }
    }

    #[test]
    fn align_up_test() {
        let a = align_up::<{ 1024 * 1024 * 2 }>(1024);
        let b = align_up::<{ 1024 * 1024 * 2 }>(1024 * 1024 * 2 + 1);

        assert_eq!(a, 1024 * 1024 * 2);
        assert_eq!(b, 1024 * 1024 * 4);
    }
}
