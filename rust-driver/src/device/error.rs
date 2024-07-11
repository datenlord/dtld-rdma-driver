use std::error::Error;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum DeviceError {
    /// Device related error
    #[error("device error : {0}")]
    Device(Box<dyn Error>),

    /// Scheduler 
    #[error("scheduler : {0}")]
    Scheduler(String),

    /// Failed to parse the descriptor
    #[error("parse descriptor error : {0}")]
    ParseDesc(String),

    /// Polling timeout
    #[error("Operation timeout")]
    Timeout,
}

pub(crate)type DeviceResult<T> = Result<T, DeviceError>;