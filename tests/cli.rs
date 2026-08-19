//! Launch the real `hp-m177` binary twice against the fake device,
//! and drive `hp-m177 discover` against a live Bonjour advertisement.

use hp_m177::advertise::Advertisement;
use hp_m177::fake::FakeDevice;
use hp_m177::imagefmt;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn hp_m177() -> Command {
    Command::new(env!("CARGO_BIN_EXE_hp-m177"))
}

fn unique(prefix: &str) -> PathBuf {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("hp-m177-cli-{prefix}-{n}"));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn run_once(fake: &FakeDevice, home: &PathBuf, dest: &PathBuf, source: &str, color: &str, format: &str) {
    let add = hp_m177()
        .env("HP_M177_HOME", home)
        .args([
            "add",
            &fake.host(),
            "--soap-port",
            &fake.port().to_string(),
            "--escl-port",
            &fake.port().to_string(),
        ])
        .output()
        .expect("spawn add");
    assert!(
        add.status.success(),
        "add failed: {}{}",
        String::from_utf8_lossy(&add.stderr),
        String::from_utf8_lossy(&add.stdout)
    );

    let scan = hp_m177()
        .env("HP_M177_HOME", home)
        .args([
            "scan",
            "--source",
            source,
            "--color",
            color,
            "--dpi",
            "300",
            "--format",
            format,
            "--output",
            dest.to_str().unwrap(),
        ])
        .output()
        .expect("spawn scan");
    assert!(
        scan.status.success(),
        "scan failed: {}{}",
        String::from_utf8_lossy(&scan.stderr),
        String::from_utf8_lossy(&scan.stdout)
    );
    let bytes = std::fs::read(dest).expect("read scan output");
    if format == "pdf" {
        assert!(imagefmt::is_pdf(&bytes), "CLI output is not a PDF");
    } else {
        assert!(imagefmt::is_jpeg(&bytes), "CLI output is not a JPEG");
    }
    let ticket = fake.last_ticket().expect("fake saw the CLI CreateScanJob");
    assert_eq!(ticket.source.to_string(), source);
    assert_eq!(ticket.color.to_string(), color);
    assert_eq!(ticket.dpi, 300);
}

#[test]
fn cli_scan_twice_against_fake() {
    let fake = FakeDevice::start().unwrap();
    let home1 = unique("h1");
    let dest1 = unique("o1").join("page.jpg");
    run_once(&fake, &home1, &dest1, "platen", "color", "jpeg");

    let home2 = unique("h2");
    let dest2 = unique("o2").join("page.pdf");
    run_once(&fake, &home2, &dest2, "adf", "gray", "pdf");
}

#[test]
fn cli_discover_sees_advertised_scanner() {
    let instance = format!(
        "hp-m177cli{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % 1_000_000_000
    );
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let _adv = Advertisement::start(port, &instance).expect("advertise");
    thread::sleep(Duration::from_millis(800));
    let out = hp_m177()
        .args(["discover", "--timeout", "3"])
        .output()
        .expect("spawn discover");
    assert!(
        out.status.success(),
        "discover failed: {}{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(&instance),
        "hp-m177 discover did not print {instance}:\n{stdout}"
    );
}
