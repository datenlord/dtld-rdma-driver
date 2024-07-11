use std::{
    slice::{from_raw_parts, from_raw_parts_mut},
    sync::atomic::{AtomicU16, Ordering},
};

use crate::types::{Key, Sge};

/// A slot buffer allocated by `PacketBufAllocator`
pub(crate) struct Slot<const SLOT_SIZE: usize>(*mut u8, Key);

impl<const SLOT_SIZE: usize> Slot<SLOT_SIZE> {
    const _ASSERT_SLOT_SIZE : () = assert!(SLOT_SIZE < u32::MAX as usize, "SLOT SIZE too large");
    /// Convert a slot into a sge so that it can be integrated into RDMA wqe
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn into_sge(self, real_size: u32) -> Sge {
        assert!(
            real_size <= SLOT_SIZE as u32,// assert in _ASSERT_SLOT_SIZE
            "The real size should be less than the slot size"
        );
        Sge {
            phy_addr: self.0 as u64,
            len: real_size, // safe to cast here
            key: self.1,
        }
    }
}

impl<const SLOT_SIZE: usize> AsRef<[u8]> for Slot<SLOT_SIZE> {
    fn as_ref(&self) -> &[u8] {
        // SAFETY: There is no way to create Slot without PacketBufAllocator outside the mod.
        // And the buffer allocated by `PacketBufAllocator` is fixed size.
        #[allow(unsafe_code)]
        unsafe {
            from_raw_parts(self.0, SLOT_SIZE)
        }
    }
}

impl<const SLOT_SIZE: usize> AsMut<[u8]> for Slot<SLOT_SIZE> {
    fn as_mut(&mut self) -> &mut [u8] {
        // SAFETY: There is no way to create Slot without PacketBufAllocator outside the mod.
        // And the buffer allocated by `PacketBufAllocator` is fixed size.
        #[allow(unsafe_code)]
        unsafe {
            from_raw_parts_mut(self.0, SLOT_SIZE)
        }
    }
}

/// A structure to hold the acknowledge and basic nic packet buffer
///
/// The element is `Option<Slot>` because the `Queue` need to initialize some nodes as Sentinel
/// while the reference can not be initialized as `None`.
#[derive(Debug)]
pub(crate) struct PacketBufAllocator<const SLOT_SIZE: usize> {
    head: AtomicU16,
    start_va: usize,
    slot_length: u16,
    lkey: Key,
}

impl<const SLOT_SIZE: usize> PacketBufAllocator<SLOT_SIZE> {
    /// Create a new `PacketBufAllocator`
    #[allow(clippy::arithmetic_side_effects, clippy::cast_possible_truncation)]
    pub(crate) fn new(start_va: usize, length: usize, lkey: Key) -> Self {
        assert!(
            length % SLOT_SIZE == 0,
            "The length should be multiple of SLOT_SIZE"
        ); // the allocator is used internally.
        let slot_length = (length / SLOT_SIZE) as u16;
        Self {
            head: AtomicU16::default(),
            start_va,
            lkey,
            slot_length,
        }
    }

    /// Take a buffer from current ring allocator.
    #[allow(clippy::arithmetic_side_effects, clippy::cast_possible_truncation)]
    pub(crate) fn recycle_buf(&self) -> Slot<SLOT_SIZE> {
        let mut prev = self.head.load(Ordering::Relaxed);
        loop {
            let next = (prev + 1) % self.slot_length;
            match self
                .head
                .compare_exchange_weak(prev, next, Ordering::SeqCst, Ordering::Relaxed)
            {
                Ok(_) => break,
                Err(x) => prev = x,
            }
        }
        Slot(
            (self.start_va + prev as usize * SLOT_SIZE) as *mut u8,
            self.lkey,
        )
    }

    /// Start address
    pub(crate) fn start_va(&self) -> usize {
        self.start_va
    }

    /// The lkey of current buffer
    pub(crate) fn lkey(&self) -> Key {
        self.lkey
    }
}

#[cfg(test)]
mod tests {
    use crate::types::Key;

    use super::PacketBufAllocator;

    #[test]
    fn test_recycle_buffer() {
        // test if the buffer can be recycled
        const RDMA_ACK_BUFFER_SLOT_SIZE: usize = 128;
        let mem = Box::leak(Box::new([0u8; 1024 * RDMA_ACK_BUFFER_SLOT_SIZE]));
        let base_va = mem.as_ptr() as usize;
        let buffer: PacketBufAllocator<RDMA_ACK_BUFFER_SLOT_SIZE> =
            PacketBufAllocator::new(base_va, 1024 * RDMA_ACK_BUFFER_SLOT_SIZE, Key::new_unchecked(0x1000));
        for i in 0..2048 {
            let slot = buffer.recycle_buf();
            assert_eq!(
                slot.0 as usize,
                mem.as_ptr() as usize + (i % 1024) * RDMA_ACK_BUFFER_SLOT_SIZE
            );
        }
        assert_eq!(buffer.lkey(), buffer.lkey);
        assert_eq!(buffer.start_va(), buffer.start_va);
    }
}
