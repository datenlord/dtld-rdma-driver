use std::{fmt::Debug, sync::Arc};

use thiserror::Error;

mod constants;
mod emulated;
mod hardware;
mod ringbuf;
mod software;
mod types;
pub(crate) mod layout;

pub(crate) mod scheduler;
pub(crate) use types::ToCardWorkRbDesc;


pub(crate) use self::{
    emulated::EmulatedDevice, hardware::HardwareDevice, software::SoftwareDevice, types::*,
};

/// Public interface for a device. Can be a real hardware device or a software emulation.
pub(crate) trait DeviceAdaptor: Send + Sync {
    fn to_card_ctrl_rb(&self) -> Arc<dyn ToCardRb<ToCardCtrlRbDesc>>;
    fn to_host_ctrl_rb(&self) -> Arc<dyn ToHostRb<ToHostCtrlRbDesc>>;

    fn to_card_work_rb(&self) -> Arc<dyn ToCardRb<Box<ToCardWorkRbDesc>>>;
    fn to_host_work_rb(&self) -> Arc<dyn ToHostRb<ToHostWorkRbDesc>>;

    fn read_csr(&self, addr: usize) -> Result<u32, DeviceError>;
    fn write_csr(&self, addr: usize, data: u32) -> Result<(), DeviceError>;

    fn get_phys_addr(&self, virt_addr: usize) -> Result<usize, DeviceError>;

    fn use_hugepage(&self) -> bool;
}

/// Generic interface for a to-card ring buffer.
pub(crate) trait ToCardRb<D> {
    fn push(&self, desc: D) -> Result<(), DeviceError>;
}

/// Generic interface for a to-host ring buffer.
pub(crate) trait ToHostRb<D> {
    fn pop(&self) -> Result<D, DeviceError>;
}

/// An error indicating that a ring buffer overflowed.
#[derive(Debug, Error)]
#[error("ring buffer overflowed")]
pub(crate) struct Overflowed;

#[derive(Debug, Error)]
#[error("net socket failed to bind the port")]
pub(crate) struct PortBindFailed;
