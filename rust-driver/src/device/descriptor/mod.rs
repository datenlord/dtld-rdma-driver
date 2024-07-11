use std::error::Error;

use thiserror::Error;

/// ctrl descriptor
pub(crate) mod ctrl;

/// work descriptor
pub(crate) mod work;

/// layout of a descriptor
mod layout;