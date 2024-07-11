


/// The layout of a desciptor in ringbuf
mod descriptor;

/// Some constants of the device
mod constant;

/// A ring buffer for interaction between the driver and the hardware device
mod ringbuf;

/// Error type for device adatpor and related modules
mod error;

use error::{DeviceError,DeviceResult};