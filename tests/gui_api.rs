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

#[test]
fn gui_binary_scans_twice_like_cli() {
    let fake = FakeDevice::start().unwrap();
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let home = std::env::temp_dir().join(format!("hp-m177-gui-twice-{n}"));
    std::fs::create_dir_all(&home).unwrap();
    let jpg = home.join("page.jpg");
    let pdf = home.join("page.pdf");
    let add = std::process::Command::new(env!("CARGO_BIN_EXE_hp-m177-gui"))
        .env("HP_M177_HOME", &home)
        .args(["add", &format!("{}:{}", fake.host(), fake.port())])
        .output()
        .unwrap();
    assert!(add.status.success(), "gui add: {}", String::from_utf8_lossy(&add.stderr));
    let s1 = std::process::Command::new(env!("CARGO_BIN_EXE_hp-m177-gui"))
        .env("HP_M177_HOME", &home)
        .args([
            "scan", "--source", "platen", "--color", "color", "--dpi", "300",
            "--format", "jpeg", "--output", jpg.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(s1.status.success(), "gui jpeg: {}", String::from_utf8_lossy(&s1.stderr));
    assert!(imagefmt::is_jpeg(&std::fs::read(&jpg).unwrap()));
    let t1 = fake.last_ticket().unwrap();
    assert_eq!(t1.source, ScanSource::Platen);
    assert_eq!(t1.color, ColorMode::Color);

    let s2 = std::process::Command::new(env!("CARGO_BIN_EXE_hp-m177-gui"))
        .env("HP_M177_HOME", &home)
        .args([
            "scan", "--source", "adf", "--color", "gray", "--dpi", "300",
            "--format", "pdf", "--output", pdf.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(s2.status.success(), "gui pdf: {}", String::from_utf8_lossy(&s2.stderr));
    assert!(imagefmt::is_pdf(&std::fs::read(&pdf).unwrap()));
    let t2 = fake.last_ticket().unwrap();
    assert_eq!(t2.source, ScanSource::Adf);
    assert_eq!(t2.color, ColorMode::Gray);
}

#[test]
fn gui_smoke_add_and_scans() {
    let fake = FakeDevice::start().unwrap();
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let home = std::env::temp_dir().join(format!("hp-m177-gui-smoke-{n}"));
    std::fs::create_dir_all(&home).unwrap();
    let dest = home.join("smoke.jpg");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_hp-m177-gui"))
        .env("HP_M177_HOME", &home)
        .args([
            "--smoke",
            "--host",
            &format!("{}:{}", fake.host(), fake.port()),
            "--output",
            dest.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "smoke: {}{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("gui-smoke-ok"));
    assert!(imagefmt::is_jpeg(&std::fs::read(&dest).unwrap()));
}

#[test]
fn appkit_exec_scan_against_fake() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let helper = std::path::Path::new(manifest).join("target/hp-m177-native-gui");
    if !helper.is_file() {
        let st = std::process::Command::new("sh")
            .arg("scripts/build-gui.sh")
            .current_dir(manifest)
            .status()
            .expect("build-gui");
        assert!(st.success(), "swiftc failed");
    }
    let fake = FakeDevice::start().unwrap();
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let home = std::env::temp_dir().join(format!("hp-m177-exec-{n}"));
    std::fs::create_dir_all(&home).unwrap();
    let dest = home.join("exec.jpg");
    let hp = env!("CARGO_BIN_EXE_hp-m177");
    let add = std::process::Command::new(env!("CARGO_BIN_EXE_hp-m177"))
        .env("HP_M177_HOME", &home)
        .args([
            "add",
            &fake.host(),
            "--soap-port",
            &fake.port().to_string(),
            "--escl-port",
            &fake.port().to_string(),
        ])
        .output()
        .unwrap();
    assert!(add.status.success(), "add: {}", String::from_utf8_lossy(&add.stderr));
    let exec = std::process::Command::new(&helper)
        .env("HP_M177_HOME", &home)
        .env("HP_M177_BIN", hp)
        .args([
            "--exec",
            "scan",
            "--source",
            "platen",
            "--color",
            "color",
            "--format",
            "jpeg",
            "--output",
            dest.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        exec.status.success(),
        "exec scan: {}{}",
        String::from_utf8_lossy(&exec.stderr),
        String::from_utf8_lossy(&exec.stdout)
    );
    assert!(imagefmt::is_jpeg(&std::fs::read(&dest).unwrap()));
    let ticket = fake.last_ticket().unwrap();
    assert_eq!(ticket.source, ScanSource::Platen);
}

#[test]
fn appkit_native_smoke_add_and_scans() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let helper = std::path::Path::new(manifest).join("target/hp-m177-native-gui");
    let src = std::path::Path::new(manifest).join("gui/HP-M177-Scan.swift");
    let stale = !helper.is_file()
        || helper.metadata().unwrap().modified().unwrap() < src.metadata().unwrap().modified().unwrap();
    if stale {
        let st = std::process::Command::new("sh")
            .arg("scripts/build-gui.sh")
            .current_dir(manifest)
            .status()
            .expect("build-gui");
        assert!(st.success(), "swiftc failed");
    }
    let fake = FakeDevice::start().unwrap();
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let home = std::env::temp_dir().join(format!("hp-m177-native-smoke-{n}"));
    std::fs::create_dir_all(&home).unwrap();
    let dest = home.join("native-smoke.jpg");
    let hp = env!("CARGO_BIN_EXE_hp-m177");
    let out = std::process::Command::new(&helper)
        .env("HP_M177_HOME", &home)
        .env("HP_M177_BIN", hp)
        .args([
            "--smoke",
            "--host",
            &format!("{}:{}", fake.host(), fake.port()),
            "--output",
            dest.to_str().unwrap(),
        ])
        .output()
        .expect("native --smoke");
    assert!(
        out.status.success(),
        "native --smoke: {}{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("gui-native-smoke-ok"),
        "native smoke must print gui-native-smoke-ok: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(imagefmt::is_jpeg(&std::fs::read(&dest).unwrap()));
    let ticket = fake.last_ticket().unwrap();
    assert_eq!(ticket.source, ScanSource::Platen);
}