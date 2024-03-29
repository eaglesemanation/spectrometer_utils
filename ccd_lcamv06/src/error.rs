use thiserror::Error;
use core::result::Result as CoreResult;

pub type Result<T> = CoreResult<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    // TODO: Figure out a way to assemble list of baud rates at compile time
    #[error("Baud rate is not in range of accepted values: 115200, 384000, 921600")]
    InvalidBaudRate,
    #[error("Could not parse recieved data correctly")]
    InvalidData,
    #[error("Unexpected end of package")]
    UnexpectedEop,
    #[error("{0} is longer than expected")]
    VersionDetailTooLong(&'static str),
    #[error("Recieved an unexpected type of response: {0}")]
    UnexpectedResponse(&'static str),

    #[cfg(feature = "std")]
    #[error("{0}")]
    StdIoError(#[from] std::io::Error),

    // TODO: Include contents of original error
    #[cfg(feature = "embedded-hal-nb")]
    #[error("Serial communication failed")]
    EmbeddedHalNbError,
}
