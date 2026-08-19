//! Application logic for the native Mac GUI. The windowing layer (AppKit or
//! the Rust binary's smoke/headless path) calls these functions; they are
//! the same `add_by_address` / `scan` entry points the tests drive.

use crate::cli;
use crate::error::Result;
use crate::model::{DeviceRecord, ScanOutput, ScanRequest};
use crate::scan;
use crate::store::Store;
use crate::transport::{Transport, UreqTransport};
use std::path::PathBuf;

pub struct GuiApp {
    pub store: Store,
}

impl GuiApp {
    pub fn open(dir: impl AsRef<std::path::Path>) -> Result<Self> {
        Ok(Self {
            store: Store::open(dir)?,
        })
    }

    pub fn add_scanner(
        &mut self,
        transport: &dyn Transport,
        host: &str,
    ) -> Result<DeviceRecord> {
        cli::add_by_address(&mut self.store, transport, host, None, None)
    }

    pub fn devices(&self) -> &[DeviceRecord] {
        self.store.list()
    }

    pub fn scan(
        &self,
        transport: &dyn Transport,
        req: &ScanRequest,
    ) -> Result<(ScanOutput, PathBuf)> {
        let device = self.store.default_device()?;
        let dest = req.output.clone().unwrap_or_else(|| {
            PathBuf::from(format!("scan-{}.{}", "gui", req.format.extension()))
        });
        let out = scan::scan(transport, &device, req)?;
        let path = scan::write_output(&out, &dest)?;
        Ok((out, path))
    }

    /// Headless self-test used by `hp-m177-gui --smoke`.
    pub fn smoke(config_dir: impl AsRef<std::path::Path>, host: &str, dest: PathBuf) -> Result<PathBuf> {
        let mut app = Self::open(config_dir)?;
        let transport = UreqTransport::default();
        app.add_scanner(&transport, host)?;
        let req = ScanRequest {
            output: Some(dest.clone()),
            ..ScanRequest::default()
        };
        let (_out, path) = app.scan(&transport, &req)?;
        Ok(path)
    }
}
