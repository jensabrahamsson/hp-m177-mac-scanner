//! Scan client, local eSCL/AirScan facade, and shared add/scan API for the
//! HP Color LaserJet Pro MFP M177fw.
//!
//! Printing stays on the existing AirPrint/CUPS queue. This crate talks the
//! firmware's HP SOAP/DIME job cycle (port 8289) and, when a device actually
//! implements them, eSCL ScanJobs.

pub mod advertise;
pub mod cli;
pub mod dime;
pub mod discovery;
pub mod error;
pub mod escl;
pub mod facade;
pub mod fake;
pub mod gui;
pub mod imagefmt;
pub mod model;
pub mod printadd;
pub mod probe;
pub mod scan;
pub mod soap;
pub mod store;
pub mod transport;
pub mod xmlutil;

pub use cli::add_by_address;
pub use error::{Error, Result};
pub use model::{
    ColorMode, DeviceRecord, JobProtocol, OutputFormat, ProbeResult, ScanOutput, ScanRequest,
    ScanSource,
};
pub use scan::scan;
pub use store::Store;

/// Convenience: probe `host` and persist it using the real HTTP transport.
pub fn add_by_address_default(store: &mut Store, host: &str) -> Result<DeviceRecord> {
    let t = transport::UreqTransport::default();
    add_by_address(store, &t, host, None, None)
}
