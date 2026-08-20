//! Native desktop GUI entry point. `--smoke` drives the same add/scan
//! functions the tests cover (no window). Without flags the process opens
//! the AppKit window defined in `gui/HP-M177-Scan.swift` when that helper
//! is on PATH, otherwise it runs an interactive Cocoa-free fallback that
//! still calls `GuiApp::add_scanner` / `GuiApp::scan`.

use clap::{Parser, Subcommand};
use hp_m177::gui::GuiApp;
use hp_m177::model::{ColorMode, OutputFormat, ScanRegion, ScanRequest, ScanSource};
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
    /// Ask the AppKit helper to print control frames and exit (no clicks).
    #[arg(long)]
    layout_check: bool,
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
        #[arg(long)]
        region: Option<String>,
    },
    /// List saved scanners.
    List,
    /// Forward to the AppKit binary (`--exec add|scan|…`).
    Exec {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        args: Vec<String>,
    },
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

    if args.layout_check {
        match spawn_native(&["--layout-check"]) {
            Ok(code) => std::process::exit(code),
            Err(e) => {
                eprintln!("hp-m177-gui: {e}");
                std::process::exit(1);
            }
        }
    }

    if let Some(helper) = find_native_gui() {
        let mut cmd = Command::new(&helper);
        apply_gui_env(&mut cmd);
        match cmd.status() {
            Ok(s) => std::process::exit(s.code().unwrap_or(1)),
            Err(e) => eprintln!("hp-m177-gui: native helper {}: {e}", helper.display()),
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
            region,
        } => {
            let req = ScanRequest {
                source: ScanSource::parse(&source)?,
                color: ColorMode::parse(&color)?,
                dpi,
                format: OutputFormat::parse(&format)?,
                output: output.clone(),
                region: region.as_deref().map(ScanRegion::parse).transpose()?,
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
        GuiCmd::Exec { args } => {
            let mut forwarded = vec!["--exec".to_string()];
            forwarded.extend(args.iter().cloned());
            match spawn_native(&forwarded.iter().map(|s| s.as_str()).collect::<Vec<_>>()) {
                Ok(code) => return Ok(code),
                Err(_) => return run_api(parse_exec_fallback(&args)?),
            }
        }
    }
    Ok(0)
}

fn parse_exec_fallback(args: &[String]) -> hp_m177::Result<GuiCmd> {
    let verb = args.first().map(|s| s.as_str()).unwrap_or("");
    let mut host = String::new();
    let mut source = "platen".to_string();
    let mut color = "color".to_string();
    let mut dpi = 300u32;
    let mut format = "jpeg".to_string();
    let mut output = None;
    let mut region = None;
    let mut i = 1;
    while i < args.len() {
        let a = args[i].as_str();
        let next = || args.get(i + 1).cloned().unwrap_or_default();
        match a {
            "--host" => {
                host = next();
                i += 1;
            }
            "--source" => {
                source = next();
                i += 1;
            }
            "--color" => {
                color = next();
                i += 1;
            }
            "--dpi" => {
                dpi = next().parse().unwrap_or(300);
                i += 1;
            }
            "--format" => {
                format = next();
                i += 1;
            }
            "--output" => {
                output = Some(PathBuf::from(next()));
                i += 1;
            }
            "--region" => {
                region = Some(next());
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    match verb {
        "add" => {
            if host.is_empty() {
                return Err(hp_m177::Error::msg("exec add needs --host"));
            }
            Ok(GuiCmd::Add { host })
        }
        "scan" => Ok(GuiCmd::Scan {
            source,
            color,
            dpi,
            format,
            output,
            region,
        }),
        "list" => Ok(GuiCmd::List),
        other => Err(hp_m177::Error::msg(format!(
            "unknown exec verb '{other}' (use add|scan|list)"
        ))),
    }
}

fn spawn_native(args: &[&str]) -> hp_m177::Result<i32> {
    let helper = find_native_gui().ok_or_else(|| {
        hp_m177::Error::msg("native AppKit helper not found; run ./scripts/install-gui.sh")
    })?;
    let mut cmd = Command::new(&helper);
    cmd.args(args);
    apply_gui_env(&mut cmd);
    let status = cmd
        .status()
        .map_err(|e| hp_m177::Error::msg(format!("native gui {}: {e}", helper.display())))?;
    Ok(status.code().unwrap_or(1))
}

fn apply_gui_env(cmd: &mut Command) {
    let mut bin = None;
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("hp-m177");
            if p.is_file() {
                bin = Some(p);
            }
        }
    }
    if bin.is_none() {
        if let Some(home) = std::env::var_os("HOME") {
            let p = PathBuf::from(home).join(".cargo/bin/hp-m177");
            if p.is_file() {
                bin = Some(p);
            }
        }
    }
    if let Some(p) = bin {
        cmd.env("HP_M177_BIN", p);
    }
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
            home.join("Applications/HP Color LaserJet Pro MFP M177fw Scanner.app/Contents/MacOS/HP-M177-Scan"),
        );
        candidates.push(
            home.join("Applications/HP M177 Scanner.app/Contents/MacOS/HP-M177-Scan"),
        );
    }
    candidates.push(PathBuf::from(
        "/Applications/HP Color LaserJet Pro MFP M177fw Scanner.app/Contents/MacOS/HP-M177-Scan",
    ));
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
        "{} (fallback UI — same add/scan API as the AppKit GUI)",
        hp_m177::APP_DISPLAY_NAME
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
