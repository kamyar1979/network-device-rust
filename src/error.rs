//! Vendor-neutral classification of driver errors.

use std::convert::Infallible;
use std::error::Error;
use std::fmt;

/// What a failed operation means for its caller, whatever the vendor.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ErrorKind {
    /// The driver or device cannot perform this operation.
    Unsupported,
    /// The driver or device refused the input; retrying will not help.
    Rejected,
    /// Retrying is safe: nothing was written, or the operation converges.
    Transient,
    /// A write may or may not have been applied, for example after a lost response.
    UnknownOutcome,
    /// Credentials were refused or the session expired.
    Authentication,
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unsupported => "unsupported",
            Self::Rejected => "rejected",
            Self::Transient => "transient",
            Self::UnknownOutcome => "unknown outcome",
            Self::Authentication => "authentication",
        })
    }
}

/// An error a driver returns, classified for vendor-neutral handling.
///
/// Every [`crate::Driver::Error`] implements this, so callers decide retries
/// and outcomes from [`DeviceError::kind`] without matching vendor types.
pub trait DeviceError: Error + Send + Sync + 'static {
    /// How the caller should treat this failure.
    fn kind(&self) -> ErrorKind;

    /// Device writes acknowledged by the failed operation before it failed.
    fn completed_writes(&self) -> usize {
        0
    }
}

impl DeviceError for Infallible {
    fn kind(&self) -> ErrorKind {
        match *self {}
    }
}

/// A driver's answer to an operation it does not implement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Unsupported {
    operation: &'static str,
}

impl Unsupported {
    /// Creates the error for one operation, named as in [`crate::DeviceOperation::name`].
    pub const fn new(operation: &'static str) -> Self {
        Self { operation }
    }

    /// Returns the unsupported operation's name.
    pub const fn operation(&self) -> &'static str {
        self.operation
    }
}

impl fmt::Display for Unsupported {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "operation '{}' is not supported", self.operation)
    }
}

impl Error for Unsupported {}

impl DeviceError for Unsupported {
    fn kind(&self) -> ErrorKind {
        ErrorKind::Unsupported
    }
}
