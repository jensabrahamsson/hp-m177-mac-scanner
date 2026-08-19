use std::io;
use std::path::PathBuf;

/// Recoverable product error. Display strings are intended for CLI/GUI users.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("HTTP {status} from {url}: {detail}")]
    Http {
        status: u16,
        url: String,
        detail: String,
    },

    #[error("transport error talking to {url}: {detail}")]
    Transport { url: String, detail: String },

    #[error("the scanner response could not be parsed: {0}")]
    Protocol(String),

    #[error("no usable scan protocol on {host} (tried eSCL ScanJobs and HP SOAP on port {soap_port})")]
    NoScanProtocol { host: String, soap_port: u16 },

    #[error("device '{0}' is not in the local device list; run `hp-m177 add` or `hp-m177 discover`")]
    UnknownDevice(String),

    #[error("scan job timed out after {0:?}")]
    Timeout(std::time::Duration),

    #[error("ADF is empty")]
    AdfEmpty,

    #[error("could not write {path}: {detail}")]
    Output { path: PathBuf, detail: String },

    #[error("invalid scan request: {0}")]
    InvalidRequest(String),

    #[error("{0}")]
    Json(#[from] serde_json::Error),
}

impl Error {
    pub fn msg(text: impl Into<String>) -> Self {
        Self::Message(text.into())
    }

    pub fn protocol(text: impl Into<String>) -> Self {
        Self::Protocol(text.into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
