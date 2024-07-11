use std::{mem, net::Ipv4Addr};

use bitflags::bitflags;
use derive_builder::Builder;
use eui48::MacAddress;
use num_enum::TryFromPrimitive;
use thiserror::Error;

macro_rules! reverse_byte_order_helper {
    ($val:expr) => {
        if cfg!(target_endian = "little") {
            $val.to_be()
        } else {
            $val.to_le()
        }
    };
}

/// Protection Domain
#[derive(Debug, Clone, Copy, Default)]
pub struct Pd {
    handle: u32,
}

/// Type for `Imm`
#[derive(Debug, Clone, Copy, Hash, Default)]
pub struct Imm(u32);
impl Imm {
    /// Create a new `Imm` with the given value.
    #[must_use]
    pub fn new(imm: u32) -> Self {
        Self(imm)
    }

    /// Get the value of `Imm`.
    #[must_use]
    pub fn get(&self) -> u32 {
        self.0
    }

    pub(crate) fn into_ne(self) -> u32 {
        reverse_byte_order_helper!(self.0)
    }
}

impl From<u32> for Imm {
    fn from(imm: u32) -> Self {
        Self::new(imm)
    }
}

/// Message Sequence Number
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct Msn(u16);
impl Msn {
    /// Create a new `Msn` with the given value.
    pub(crate) fn new(msn: u16) -> Self {
        Self(msn)
    }

    /// Get the value of `Msn`.
    #[must_use]
    pub fn get(&self) -> u16 {
        self.0
    }

    /// Convert the value of `Msn` to big endian.
    fn into_ne(self) -> u16 {
        reverse_byte_order_helper!(self.0)
    }
}

impl From<u16> for Msn {
    fn from(msn: u16) -> Self {
        Self::new(msn)
    }
}

impl Default for Msn {
    fn default() -> Self {
        Self::new(0)
    }
}

/// `RKey` and `LKey`
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Default)]
pub struct Key(u32);
impl Key {
    const MR_KEY_IDX_BIT_CNT: usize = 8;

    /// Create a new `Key` with the given value.
    pub(crate) fn new(mr_idx: u32, key_secret: u32) -> Self {
        let key_idx = mr_idx << ((u32::BITS as usize).wrapping_sub(Self::MR_KEY_IDX_BIT_CNT));
        let key_secret = key_secret >> Self::MR_KEY_IDX_BIT_CNT;
        Self(key_idx | key_secret)
    }

    pub(crate) fn new_unchecked(key: u32) -> Self {
        Self(key)
    }

    /// Get the value of `Key`.
    #[must_use]
    pub fn get(&self) -> u32 {
        self.0
    }

    /// Convert the value of `Key` to network endian.
    pub(crate) fn into_ne(self) -> u32 {
        reverse_byte_order_helper!(self.0)
    }
}

/// Queue Pair Number
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Default)]
pub struct Qpn(u32);

impl Qpn {
    const WIDTH_IN_BITS: usize = 24;
    const MASK: u32 = u32::MAX >> (32 - Self::WIDTH_IN_BITS);

    /// The `QPN` value should be less than 2^24;
    pub(crate) fn new(qpn: u32) -> Self {
        assert!(qpn <= Self::MASK, "QPN should not exceed 24 bits");
        Self(qpn)
    }

    /// Get the value of `Qpn`.
    #[must_use]
    pub fn get(&self) -> u32 {
        self.0
    }

    /// Convert the value of `qpn` to net endian.
    pub(crate) fn into_ne(self) -> u32 {
        let key = self.0.to_ne_bytes();
        u32::from_le_bytes([key[2], key[1], key[0], 0])
    }
}

/// In RDMA spec, some structs are defined as 24 bits.
/// For example : `PSN`, `QPN` etc.
///
/// This struct is used to represent these 24 bits.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Default)]
pub struct Psn(u32);

impl Psn {
    const WIDTH_IN_BITS: usize = 24;
    const MASK: u32 = u32::MAX >> (32 - Self::WIDTH_IN_BITS);
    const MAX_PSN_RANGE: u32 = 1 << 23_i32;

    /// Create a new `Psn` with the given value.
    ///
    /// # Panics
    /// If the value is greater than 24 bits, it will panic.
    #[must_use]
    pub fn new(psn: u32) -> Self {
        assert!(psn <= Self::MASK, "PSN should not exceed 24 bits");
        Self(psn)
    }

    /// Get the value of `psn`.
    #[must_use]
    pub fn get(&self) -> u32 {
        self.0
    }

    /// Convert the value of `psn` to net endian.
    pub(crate) fn into_ne(self) -> u32 {
        let key = self.0.to_ne_bytes();
        u32::from_le_bytes([key[2], key[1], key[0], 0])
    }

    /// wrapping add the current value with rhs
    pub(crate) fn wrapping_add(self, rhs: Self) -> Self {
        // since (a+b) mod p  = (a + (b mod p)) mod p, we don't have to let rhs= rhs%p here
        Self(self.0.wrapping_add(rhs.0) & Self::MASK)
    }

    /// Get the difference between two PSN
    pub(crate) fn wrapping_sub(self, rhs: Self) -> u32 {
        self.0.wrapping_sub(rhs.0) & Self::MASK
    }
}

impl From<u32> for Psn {
    fn from(key: u32) -> Self {
        Self::new(key)
    }
}

bitflags! {
    /// Memory access bit flags
    #[derive(Debug, Clone, Copy)]
    pub struct MemAccessTypeFlag: u8 {
        /// No access flag
        const IbvAccessNoFlags = 0;      // Not defined in rdma-core

        /// Local write
        const IbvAccessLocalWrite = 1;   // (1 << 0)

        /// Remote write
        const IbvAccessRemoteWrite = 2;  // (1 << 1)

        /// Remote read
        const IbvAccessRemoteRead = 4;   // (1 << 2)

        /// Remote atomic
        const IbvAccessRemoteAtomic = 8; // (1 << 3)

        /// Mw bind
        const IbvAccessMwBind = 16;      // (1 << 4)

        /// Zero based
        const IbvAccessZeroBased = 32;   // (1 << 5)

        /// On demand
        const IbvAccessOnDemand = 64;    // (1 << 6)

        /// Hugetlb
        const IbvAccessHugetlb = 128;    // (1 << 7)

        // IbvAccessRelaxedOrdering   = IBV_ACCESS_OPTIONAL_FIRST,
    }
}

bitflags! {
    /// Work Request Send Flag
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct WorkReqSendFlag: u8 {
        /// No flags
        const IbvSendNoFlags  = 0; // Not defined in rdma-core
        /// Send fence
        const IbvSendFence     = 1;
        /// Send signaled
        const IbvSendSignaled  = 2;
        /// Send solicited
        const IbvSendSolicited = 4;
        /// Send inline
        const IbvSendInline    = 8;
        /// Send IP checksum
        const IbvSendChecksum   = 16;
    }
}

/// Queue Pair Type for software/hardware
#[non_exhaustive]
#[derive(TryFromPrimitive, Debug, Clone, Copy)]
#[repr(u8)]
pub enum QpType {
    /// Reliable Connection
    Rc = 2,

    /// Unreliable Connection
    Uc = 3,

    /// Unreliable Datagram
    Ud = 4,

    /// Raw Packet
    RawPacket = 8,

    /// XRC Send
    XrcSend = 9,

    /// XRC Receive
    XrcRecv = 10,
}

/// Packet MTU
#[non_exhaustive]
#[derive(Default, Debug, Clone, Copy)]
#[repr(u8)]
pub enum Pmtu {
    /// 256 bytes
    #[default]
    Mtu256 = 1,

    /// 512 bytes
    Mtu512 = 2,

    /// 1024 bytes
    Mtu1024 = 3,

    /// 2048 bytes
    Mtu2048 = 4,

    /// 4096 bytes
    Mtu4096 = 5,
}

impl From<Pmtu> for u32 {
    fn from(pmtu: Pmtu) -> u32 {
        const BASE_VALUE: u32 = 128;
        BASE_VALUE << (pmtu as usize)
    }
}

/// Scatter Gather Element
#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub struct Sge {
    /// physical address
    pub phy_addr: u64,

    /// Length
    pub len: u32,

    /// LKey
    pub key: Key,
}

impl Sge {
    /// Create a new `Sge`
    #[must_use]
    pub fn new(phy_addr: u64, len: u32, key: Key) -> Self {
        Self { phy_addr, len, key }
    }
}

/// RDMA network param
#[derive(Debug, Builder, Clone, Copy)]
#[non_exhaustive]
pub struct RdmaDeviceNetworkParam {
    /// Network gateway
    gateway: Ipv4Addr,

    /// Network netmask
    netmask: Ipv4Addr,

    /// IP address
    ipaddr: Ipv4Addr,

    /// MAC address
    macaddr: MacAddress,
}

/// Queue Pair imuutable context
#[non_exhaustive]
#[derive(Builder, Debug, Clone, Copy)]
pub struct Qp {
    /// Protection Domain
    pd: Pd,

    /// Queue Pair Number
    qpn: Qpn,

    /// Peer Queue Pair Number
    peer_qpn: Qpn,

    /// Queue Pair Type
    qp_type: QpType,

    /// Receive Queue Access Flags
    rq_acc_flags: MemAccessTypeFlag,

    /// Packet MTU
    pmtu: Pmtu,

    /// Destination IP
    dqp_ip: Ipv4Addr,

    /// Destination MAC
    dqp_mac: MacAddress,
}

/// Error type for user
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum Error {
    /// Invalid user input
    #[error("Invalid: {0}")]
    Invalid(Box<String>),
}

#[cfg(test)]
mod tests {
    use crate::types::Psn;
    use std::slice::from_raw_parts;

    #[test]
    fn test_wrapping_add() {
        let psn = Psn::new(0xffffff);
        let ret = psn.wrapping_add(1.into());
        assert_eq!(0, ret.get());

        let ret = psn.wrapping_add(2.into());
        assert_eq!(ret.get(), 1);

        let ret = psn.wrapping_add(0xffffff.into());
        assert_eq!(ret.get(), 0xffffff - 1);
    }

    #[test]
    fn test_to_ne() {
        let psn = Psn::new(0x123456);
        let mem = psn.into_ne();
        let buf = unsafe { from_raw_parts(&mem as *const _ as *const u8, 4) };
        assert_eq!(buf, &[0x12, 0x34, 0x56, 0]);

        let key = crate::types::Key::new_unchecked(0x12345678);
        let mem = key.into_ne();
        let buf = unsafe { from_raw_parts(&mem as *const _ as *const u8, 4) };
        assert_eq!(buf, &[0x12, 0x34, 0x56, 0x78]);
    }

    #[test]
    fn test_wrapping_sub() {
        let psn = Psn::new(0);
        let psn2 = psn.wrapping_sub(1.into());
        assert_eq!(psn2, 0xffffff);

        let psn = Psn::new(0x800001);
        let diff = psn.wrapping_sub(0.into());
        assert_eq!(diff, 0x800001);

        let psn = Psn::new(0);
        let diff = psn.wrapping_sub(0x800001.into());
        assert_eq!(diff, 0x7fffff);
    }

    #[test]
    fn test_pmtu() {
        let pmtu = crate::types::Pmtu::Mtu256;
        assert_eq!(u32::from(pmtu), 256);
        let pmtu = crate::types::Pmtu::Mtu512;
        assert_eq!(u32::from(pmtu), 512);
        let pmtu = crate::types::Pmtu::Mtu1024;
        assert_eq!(u32::from(pmtu), 1024);
        let pmtu = crate::types::Pmtu::Mtu2048;
        assert_eq!(u32::from(pmtu), 2048);
        let pmtu = crate::types::Pmtu::Mtu4096;
        assert_eq!(u32::from(pmtu), 4096);
    }
}
