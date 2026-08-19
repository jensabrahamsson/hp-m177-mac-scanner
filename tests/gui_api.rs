//! Drive the GUI application object against the fake device. This is the
//! same `add_by_address` / `scan` path the AppKit window and `hp-m177-gui
//! --smoke` use.

use hp_m177::fake::FakeDevice;
use hp_m177::gui::GuiApp;
use hp_m177::imagefmt;
use hp_m177::model::{ColorMode, OutputFormat, ScanRequest, ScanSource};
use hp_m177::transport::UreqTransport;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn gui_app_add_and_scan_against_fake() {
    let fake = FakeDevice::start().unwrap();
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let home = std::env::temp_dir().join(format!("hp-m177-gui-api-{n}"));
    std::fs::create_dir_all(&home).unwrap();
    let mut app = GuiApp::open(&home).unwrap();
    let t = UreqTransport::default();
    let rec = app
        .add_scanner(&t, &format!("{}:{}", fake.host(), fake.port()))
        .expect("GuiApp::add_scanner");
    assert_eq!(rec.host, fake.host());

    let dest = home.join("gui-scan.jpg");
    let req = ScanRequest {
        source: ScanSource::Platen,
        color: ColorMode::Color,
        dpi: 300,
        format: OutputFormat::Jpeg,
        output: Some(dest.clone()),
        region: None,
    };
    let (out, path) = app.scan(&t, &req).expect("GuiApp::scan");
    assert!(imagefmt::is_jpeg(&out.bytes));
    assert_eq!(path, dest);
    let ticket = fake.last_ticket().expect("GUI scan sent SOAP");
    assert_eq!(ticket.source, ScanSource::Platen);
    assert_eq!(ticket.color, ColorMode::Color);
}

#[test]
fn gui_binary_api_add_and_scan() {
    let fake = FakeDevice::start().unwrap();
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let home = std::env::temp_dir().join(format!("hp-m177-gui-bin-{n}"));
    std::fs::create_dir_all(&home).unwrap();
    let dest = home.join("api.jpg");
    let add = std::process::Command::new(env!("CARGO_BIN_EXE_hp-m177-gui"))
        .env("HP_M177_HOME", &home)
        .args(["add", &format!("{}:{}", fake.host(), fake.port())])
        .output()
        .expect("gui add");
    assert!(
        add.status.success(),
        "gui add: {}{}",
        String::from_utf8_lossy(&add.stderr),
        String::from_utf8_lossy(&add.stdout)
    );
    assert!(String::from_utf8_lossy(&add.stdout).contains("gui-api added"));
    let scan = std::process::Command::new(env!("CARGO_BIN_EXE_hp-m177-gui"))
        .env("HP_M177_HOME", &home)
        .args([
            "scan",
            "--source",
            "platen",
            "--color",
            "color",
            "--dpi",
            "300",
            "--format",
            "jpeg",
            "--output",
            dest.to_str().unwrap(),
        ])
        .output()
        .expect("gui scan");
    assert!(
        scan.status.success(),
        "gui scan: {}{}",
        String::from_utf8_lossy(&scan.stderr),
        String::from_utf8_lossy(&scan.stdout)
    );
    let bytes = std::fs::read(&dest).expect("api jpg");
    assert!(imagefmt::is_jpeg(&bytes));
    let ticket = fake.last_ticket().unwrap();
    assert_eq!(ticket.source, ScanSource::Platen);
}