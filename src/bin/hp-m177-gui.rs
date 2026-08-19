//! Native desktop GUI entry point. `--smoke` drives the same add/scan
//! functions the tests cover (no window). Without flags the process opens
//! the AppKit window defined in `gui/HP-M177-Scan.swift` when that helper
//! is on PATH, otherwise it runs an interactive Cocoa-free fallback that
//! still calls `GuiApp::add_scanner` / `GuiApp::scan`.

use clap::{Parser, Subcommand};
use hp_m177::gui::GuiApp;
use hp_m177::model::{ColorMode, OutputFormat, ScanRequest, ScanSource};
use hp_m177::transport::UreqTransport;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

/// Automate the same add/scan path the AppKit window uses.
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
    #[command(subcommand)]
    cmd: Option<GuiCmd>,
}

#[derive(Subcommand, Debug)]
enum GuiCmd {
    /// Add a scanner (same as the Add button).
    Add {
        host: String,
    },
    /// Run a scan (same as the Scan button).
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
    },
    /// List saved scanners.
    List,
}

fn main() {
    let args = Args::parse();
    if let Some(cmd) = args.cmd {
        match run_api(cmd) {
            Ok(code) => std::process::exit(code),
            Err(e) => {
                eprintln!("hp-m177-gui: {e}");
                std::process::exit(1);
            }
        }
    }
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

    if let Some(helper) = find_native_gui() {
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

fn run_api(cmd: GuiCmd) -> hp_m177::Result<i32> {
    let mut app = GuiApp::open(hp_m177::store::config_dir())?;
    let t = UreqTransport::default();
    match cmd {
        GuiCmd::Add { host } => {
            let rec = app.add_scanner(&t, &host)?;
            println!(
                "gui-api added {} via {:?} id={}",
                rec.host, rec.job, rec.id
            );
        }
        GuiCmd::Scan {
            source,
            color,
            dpi,
            format,
            output,
        } => {
            let req = ScanRequest {
                source: ScanSource::parse(&source)?,
                color: ColorMode::parse(&color)?,
                dpi,
                format: OutputFormat::parse(&format)?,
                output: output.clone(),
            };
            let (out, path) = app.scan(&t, &req)?;
            println!(
                "gui-api wrote {} ({} bytes, {} {} {}dpi)",
                path.display(),
                out.bytes.len(),
                out.source,
                out.color,
                out.dpi
            );
        }
        GuiCmd::List => {
            for d in app.devices() {
                println!("{}\t{}\t{:?}", d.id, d.host, d.job);
            }
            if app.devices().is_empty() {
                println!("(no scanners)");
            }
        }
    }
    Ok(0)
}

fn find_native_gui() -> Option<PathBuf> {
    if let Ok(helper) = std::env::var("HP_M177_NATIVE_GUI") {
        let p = PathBuf::from(helper);
        if p.is_file() {
            return Some(p);
        }
    }
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("hp-m177-native-gui"));
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        candidates.push(home.join(".cargo/bin/hp-m177-native-gui"));
        candidates.push(
            home.join("Applications/HP M177 Scanner.app/Contents/MacOS/HP-M177-Scan"),
        );
    }
    candidates.push(PathBuf::from(
        "/Applications/HP M177 Scanner.app/Contents/MacOS/HP-M177-Scan",
    ));
    candidates.into_iter().find(|p| p.is_file())
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
