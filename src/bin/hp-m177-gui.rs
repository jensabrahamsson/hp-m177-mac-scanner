//! Native desktop GUI entry point. `--smoke` drives the same add/scan
//! functions the tests cover (no window). Without flags the process opens
//! the AppKit window defined in `gui/HP-M177-Scan.swift` when that helper
//! is on PATH, otherwise it runs an interactive Cocoa-free fallback that
//! still calls `GuiApp::add_scanner` / `GuiApp::scan`.

use clap::Parser;
use hp_m177::gui::GuiApp;
use hp_m177::model::ScanRequest;
use hp_m177::transport::UreqTransport;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
#[command(name = "hp-m177-gui", about = "Native Mac GUI for the M177fw scanner")]
struct Args {
    /// Add, scan once against HP_M177_FAKE / --host, then exit (no window).
    #[arg(long)]
    smoke: bool,
    #[arg(long)]
    host: Option<String>,
    #[arg(long)]
    output: Option<PathBuf>,
    /// Stay running without opening a window (used to assert the binary lives).
    #[arg(long)]
    headless: bool,
}

fn main() {
    let args = Args::parse();
    if args.smoke {
        let host = args
            .host
            .or_else(|| std::env::var("HP_M177_FAKE").ok())
            .expect("hp-m177-gui --smoke needs --host or HP_M177_FAKE");
        let dest = args.output.unwrap_or_else(|| PathBuf::from("gui-smoke.jpg"));
        let dir = hp_m177::store::config_dir();
        match GuiApp::smoke(dir, &host, dest) {
            Ok(path) => {
                println!("gui-smoke-ok {}", path.display());
                return;
            }
            Err(e) => {
                eprintln!("hp-m177-gui smoke: {e}");
                std::process::exit(1);
            }
        }
    }

    if args.headless {
        println!("hp-m177-gui headless ready");
        std::thread::sleep(std::time::Duration::from_millis(250));
        return;
    }

    if let Ok(helper) = std::env::var("HP_M177_NATIVE_GUI") {
        let status = Command::new(helper).status();
        if let Ok(s) = status {
            std::process::exit(s.code().unwrap_or(1));
        }
    }

    if let Err(e) = interactive_fallback(args.host) {
        eprintln!("hp-m177-gui: {e}");
        std::process::exit(1);
    }
}

fn interactive_fallback(default_host: Option<String>) -> hp_m177::Result<()> {
    let mut app = GuiApp::open(hp_m177::store::config_dir())?;
    let t = UreqTransport::default();
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    writeln!(
        stdout,
        "HP M177fw scanner (fallback UI — same add/scan API as the AppKit GUI)"
    )?;
    if let Some(host) = default_host {
        match app.add_scanner(&t, &host) {
            Ok(d) => writeln!(stdout, "Added {}", d.host)?,
            Err(e) => writeln!(stdout, "add failed: {e}")?,
        }
    }
    loop {
        write!(stdout, "command [add <host> | scan | list | quit]: ")?;
        stdout.flush()?;
        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "quit" || line == "exit" {
            break;
        }
        if line == "list" {
            for d in app.devices() {
                writeln!(stdout, "  {} {}", d.id, d.host)?;
            }
            continue;
        }
        if let Some(host) = line.strip_prefix("add ") {
            match app.add_scanner(&t, host.trim()) {
                Ok(d) => writeln!(stdout, "Added {} via {:?}", d.host, d.job)?,
                Err(e) => writeln!(stdout, "error: {e}")?,
            }
            continue;
        }
        if line == "scan" {
            let req = ScanRequest::default();
            match app.scan(&t, &req) {
                Ok((out, path)) => writeln!(
                    stdout,
                    "Wrote {} ({} bytes)",
                    path.display(),
                    out.bytes.len()
                )?,
                Err(e) => writeln!(stdout, "error: {e}")?,
            }
            continue;
        }
        writeln!(stdout, "unknown command")?;
    }
    Ok(())
}
