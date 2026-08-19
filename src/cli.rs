use crate::error::{Error, Result};
use crate::model::{
    default_scan_path, OutputFormat, ScanRegion, ScanRequest, DEFAULT_BRIDGE_PORT,
    DEFAULT_ESCL_PORT, DEFAULT_SOAP_PORT,
};
use crate::probe::{self, split_host_port};
use crate::scan;
use crate::store::Store;
use crate::transport::{Transport, UreqTransport};
use clap::{Parser, Subcommand};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(
    name = "hp-m177",
    version,
    about = "Add and scan the HP Color LaserJet Pro MFP M177fw without replacing the AirPrint queue"
)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Browse LAN printer/scanner advertisements (Bonjour).
    Discover {
        #[arg(long, default_value_t = 3)]
        timeout: u64,
    },
    /// Probe a host and persist it as a scanner (IP or hostname).
    Add {
        /// Hostname or IPv4 address, optionally `host:soap-port`.
        host: String,
        #[arg(long)]
        soap_port: Option<u16>,
        #[arg(long)]
        escl_port: Option<u16>,
    },
    /// List scanners saved in the local device store.
    List,
    /// Run a scan using a saved scanner (default: the last one added).
    Scan {
        #[arg(long, default_value = "platen")]
        source: String,
        #[arg(long, default_value = "color")]
        color: String,
        #[arg(long, default_value_t = 300)]
        dpi: u32,
        #[arg(long, default_value = "jpeg")]
        format: String,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long)]
        device: Option<String>,
        /// Crop in 1/1000 inch: x,y,width,height
        #[arg(long)]
        region: Option<String>,
    },
    /// Probe a host without saving it.
    Probe {
        host: String,
        #[arg(long)]
        soap_port: Option<u16>,
        #[arg(long)]
        escl_port: Option<u16>,
    },
    /// Add an AirPrint queue only if one does not already exist.
    AddPrinter { host: Option<String> },
    /// Serve a local eSCL / AirScan endpoint for Image Capture.
    Bridge {
        #[arg(long, default_value_t = DEFAULT_BRIDGE_PORT)]
        port: u16,
        #[arg(long)]
        no_advertise: bool,
        #[arg(long)]
        bind: Option<String>,
    },
}

pub fn parse_from<I, T>(iter: I) -> Result<Cli>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    Cli::try_parse_from(iter).map_err(|e| Error::msg(e.to_string()))
}

pub fn run(cli: Cli, store: &mut Store, transport: &dyn Transport, out: &mut dyn Write) -> Result<i32> {
    match cli.cmd {
        Command::Discover { timeout } => {
            let found = crate::discovery::browse_lan(Duration::from_secs(timeout))?;
            if found.is_empty() {
                writeln!(out, "No IPP/scanner advertisements seen in {timeout}s.")?;
            }
            for d in &found {
                let mark = if crate::discovery::likely_m177(d) {
                    "  (likely M177fw)"
                } else {
                    ""
                };
                writeln!(
                    out,
                    "{name}  {host}:{port}  {svc}{mark}",
                    name = d.name,
                    host = d.host,
                    port = d.port,
                    svc = d.service
                )?;
            }
            Ok(0)
        }
        Command::Add {
            host,
            soap_port,
            escl_port,
        } => {
            let device = add_by_address(store, transport, &host, soap_port, escl_port)?;
            writeln!(
                out,
                "Added scanner {id} ({name}) at {host} via {job:?}",
                id = device.id,
                name = device.name,
                host = device.host,
                job = device.job
            )?;
            Ok(0)
        }
        Command::List => {
            for d in store.list() {
                writeln!(
                    out,
                    "{id}\t{name}\t{host}\t{job:?}",
                    id = d.id,
                    name = d.name,
                    host = d.host,
                    job = d.job
                )?;
            }
            if store.list().is_empty() {
                writeln!(out, "(no scanners; run hp-m177 add <ip>)")?;
            }
            Ok(0)
        }
        Command::Scan {
            source,
            color,
            dpi,
            format,
            output,
            device,
            region,
        } => {
            let rec = match device {
                Some(id) => store.get(&id)?,
                None => store.default_device()?,
            };
            let req = ScanRequest {
                source: crate::model::ScanSource::parse(&source)?,
                color: crate::model::ColorMode::parse(&color)?,
                dpi,
                format: OutputFormat::parse(&format)?,
                output: output.clone(),
                region: region.as_deref().map(ScanRegion::parse).transpose()?,
            };
            let dest = output.unwrap_or_else(|| default_scan_path(req.format));
            let scanned = scan::scan(transport, &rec, &req)?;
            let path = scan::write_output(&scanned, &dest)?;
            writeln!(
                out,
                "Wrote {} ({} bytes, {} {} {}dpi)",
                path.display(),
                scanned.bytes.len(),
                scanned.source,
                scanned.color,
                scanned.dpi
            )?;
            Ok(0)
        }
        Command::Probe {
            host,
            soap_port,
            escl_port,
        } => {
            let (h, p) = split_host_port(&host, soap_port.unwrap_or(DEFAULT_SOAP_PORT));
            let soap_p = soap_port.unwrap_or(p);
            let escl_p = escl_port.unwrap_or(DEFAULT_ESCL_PORT);
            let probe = probe::probe_host_ports(transport, &h, soap_p, escl_p)?;
            writeln!(out, "host: {}", probe.host)?;
            writeln!(out, "name: {}", probe.name)?;
            writeln!(out, "escl_caps: {}", probe.escl_caps)?;
            writeln!(out, "escl_jobs: {}", probe.escl_jobs)?;
            writeln!(out, "soap: {:?}", probe.soap.is_some())?;
            writeln!(out, "preferred: {:?}", probe.preferred)?;
            if let Some(c) = &probe.soap {
                writeln!(
                    out,
                    "soap.details: platen={} adf={} colors={:?} formats={:?} state={}",
                    c.platen, c.adf, c.colors, c.formats, c.state
                )?;
            }
            Ok(0)
        }
        Command::AddPrinter { host } => {
            let host = match host {
                Some(h) => h,
                None => store.default_device().map(|d| d.host).unwrap_or_else(|_| {
                    "DEV26BA77.local".into()
                }),
            };
            match crate::printadd::add_printer_if_missing(&host)? {
                crate::model::PrintAddOutcome::LeftExisting { queue } => {
                    writeln!(out, "Left existing print queue {queue} in place.")?;
                }
                crate::model::PrintAddOutcome::Added { queue, uri } => {
                    writeln!(out, "Added AirPrint queue {queue} -> {uri}")?;
                }
                crate::model::PrintAddOutcome::Skipped { reason } => {
                    writeln!(out, "Skipped add-printer: {reason}")?;
                }
            }
            Ok(0)
        }
        Command::Bridge {
            port,
            no_advertise,
            bind,
        } => {
            let bind = bind.unwrap_or_else(|| format!("127.0.0.1:{port}"));
            let device = store.default_device().ok();
            let facade = crate::facade::EsclFacade::bind(&bind, device)?;
            writeln!(
                out,
                "eSCL listening on {}/eSCL/ScannerCapabilities",
                facade.url()
            )?;
            let _adv = if no_advertise {
                None
            } else {
                match crate::advertise::Advertisement::start(facade.addr.port(), "HP M177fw (hp-m177)")
                {
                    Ok(a) => {
                        writeln!(out, "Advertised _uscan._tcp (rs=eSCL, is=platen,adf)")?;
                        Some(a)
                    }
                    Err(e) => {
                        writeln!(out, "Bonjour advertisement failed ({e}); HTTP eSCL still works")?;
                        None
                    }
                }
            };
            writeln!(out, "Press Ctrl+C to stop.")?;
            let _ = out.flush();
            // Park the CLI thread; the facade runs in its own thread.
            loop {
                std::thread::sleep(Duration::from_secs(60));
            }
        }
    }
}

/// Probe `host` and persist a usable device record. Shared by CLI and GUI.
pub fn add_by_address(
    store: &mut Store,
    transport: &dyn Transport,
    host: &str,
    soap_port: Option<u16>,
    escl_port: Option<u16>,
) -> Result<crate::model::DeviceRecord> {
    let (h, parsed_port) = split_host_port(host, soap_port.unwrap_or(DEFAULT_SOAP_PORT));
    let soap_p = soap_port.unwrap_or(parsed_port);
    let escl_p = escl_port.unwrap_or(DEFAULT_ESCL_PORT);
    let probe = probe::probe_host_ports(transport, &h, soap_p, escl_p)?;
    let device = probe.into_device()?;
    store.upsert(device)
}

pub fn run_with_env<I, T>(args: I) -> Result<i32>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = parse_from(args)?;
    let mut store = Store::from_env_or_default()?;
    let transport = UreqTransport::default();
    let mut stdout = std::io::stdout();
    run(cli, &mut store, &transport, &mut stdout)
}
