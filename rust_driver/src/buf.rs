use std::{
    slice::from_raw_parts_mut,
    sync::atomic::{AtomicU16, Ordering},
};

use crate::types::{Key, Sge};

pub(crate) const RDMA_ACK_BUFFER_SLOT_SIZE: usize = 128;
pub(crate) const NIC_PACKET_BUFFER_SLOT_SIZE: usize = 4096;

pub(crate) struct Slot<const SLOT_SIZE: usize>(*mut u8, Key);

impl<const SLOT_SIZE: usize> Slot<SLOT_SIZE> {
    pub(crate) fn as_mut_slice(&mut self) -> &'static mut [u8] {
        unsafe { from_raw_parts_mut(self.0, SLOT_SIZE) }
    }

    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn into_sge(self, real_size: u32) -> Sge {
        assert!(
            real_size <= SLOT_SIZE as u32,
            "The real size should be less than the slot size"
        );
        Sge {
            addr: self.0 as u64,
            len: real_size, // safe to cast here
            key: self.1,
        }
    }

    pub(crate) unsafe fn from_raw_parts_mut(ptr: *mut u8, key: Key) -> Self {
        Self(ptr, key)
    }
}

/// A structure to hold the acknowledge and basic nic packet buffer
///
/// The element is `Option<Slot>` because the `Queue` need to initialize some nodes as Sentinel
/// while the reference can not be initialized as `None`.
#[derive(Debug)]
pub(crate) struct PacketBuf<const SLOT_SIZE: usize> {
    head: AtomicU16,
    start_va: usize,
    slot_length: u16,
    lkey: Key,
}

impl<const SLOT_SIZE: usize> PacketBuf<SLOT_SIZE> {
    /// Create a new acknowledge buffer
    #[allow(clippy::arithmetic_side_effects, clippy::cast_possible_truncation)]
    pub(crate) fn new(start_va: usize, length: usize, lkey: Key) -> Self {
        assert!(
            length % SLOT_SIZE == 0,
            "The length should be multiple of SLOT_SIZE"
        );
        let slot_length = (length / SLOT_SIZE) as u16;
        Self {
            head: AtomicU16::default(),
            start_va,
            lkey,
            slot_length,
        }
    }

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

    pub(crate) fn get_register_params(&self) -> (usize, Key) {
        (self.start_va, self.lkey)
    }
}

#[cfg(test)]
mod tests {
    use crate::types::Key;

    use super::{PacketBuf, RDMA_ACK_BUFFER_SLOT_SIZE};

    #[test]
    fn test_buffer() {
        let mem = Box::leak(Box::new([0u8; 1024 * RDMA_ACK_BUFFER_SLOT_SIZE]));
        let base_va = mem.as_ptr() as usize;
        let buffer: PacketBuf<RDMA_ACK_BUFFER_SLOT_SIZE> =
            PacketBuf::new(base_va, 1024 * RDMA_ACK_BUFFER_SLOT_SIZE, Key::new(0x1000));
        for i in 0..2048 {
            let slot = buffer.recycle_buf();
            assert_eq!(
                slot.0 as usize,
                mem.as_ptr() as usize + (i % 1024) * RDMA_ACK_BUFFER_SLOT_SIZE
            );
        }
    }
}
