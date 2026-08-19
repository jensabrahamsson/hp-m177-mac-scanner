//! Boot the real local eSCL listener against the fake SOAP backend.

use hp_m177::escl;
use hp_m177::facade::{self, EsclFacade};
use hp_m177::fake::FakeDevice;
use hp_m177::imagefmt;
use hp_m177::store::Store;
use hp_m177::transport::{Transport, UreqTransport};
use hp_m177::add_by_address;
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn home() -> std::path::PathBuf {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("hp-m177-escl-{n}"));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn cycle(label: &str) {
    let fake = FakeDevice::start().unwrap();
    let mut store = Store::open(home()).unwrap();
    let t = UreqTransport::default();
    let rec = add_by_address(
        &mut store,
        &t,
        &fake.host(),
        Some(fake.port()),
        Some(fake.port()),
    )
    .unwrap();
    let facade = EsclFacade::start(Some(rec)).unwrap();
    let base = format!("{}/eSCL", facade.url());

    let caps = t.get(&format!("{base}/ScannerCapabilities")).unwrap();
    assert!(caps.is_success(), "{label} caps HTTP {}", caps.status);
    let xml = caps.text();
    assert!(
        facade::capabilities_mention_required_features(&xml),
        "{label} capabilities missing platen/ADF/color/gray/jpeg/pdf:\n{xml}"
    );
    assert!(xml.contains("RGB24") && xml.contains("Grayscale8"));

    let status = t.get(&format!("{base}/ScannerStatus")).unwrap();
    assert!(status.is_success(), "{label} status HTTP {}", status.status);
    let st = status.text();
    assert!(st.contains("ScannerStatus"));
    assert!(st.contains("AdfState") || st.contains("Idle"));

    let settings = escl::scan_settings_xml(&hp_m177::ScanRequest {
        source: hp_m177::ScanSource::Platen,
        color: hp_m177::ColorMode::Color,
        dpi: 300,
        format: hp_m177::OutputFormat::Jpeg,
        output: None,
        region: None,
    });
    let created = t
        .post(&format!("{base}/ScanJobs"), settings.as_bytes(), "text/xml")
        .unwrap();
    assert_eq!(created.status, 201, "{label} ScanJobs {}", created.text());
    let loc = created
        .header("Location")
        .expect("Location header")
        .to_string();
    let doc_url = if loc.starts_with("http") {
        format!("{}/NextDocument", loc.trim_end_matches('/'))
    } else {
        format!(
            "{}{}/NextDocument",
            facade.url(),
            loc.trim_end_matches('/')
        )
    };
    let doc = t.get(&doc_url).unwrap();
    assert!(doc.is_success(), "{label} NextDocument {}", doc.status);
    assert!(
        imagefmt::is_jpeg(&doc.body) || imagefmt::is_pdf(&doc.body),
        "{label} document is not JPEG/PDF"
    );
    let ticket = fake.last_ticket().expect("facade forwarded a SOAP job");
    assert_eq!(ticket.source, hp_m177::ScanSource::Platen);
    assert_eq!(ticket.color, hp_m177::ColorMode::Color);
}

#[test]
fn escl_launch_twice() {
    cycle("run-1");
    cycle("run-2");
}

#[test]
fn escl_pdf_job() {
    let fake = FakeDevice::start().unwrap();
    let mut store = Store::open(home()).unwrap();
    let t = UreqTransport::default();
    let rec = add_by_address(
        &mut store,
        &t,
        &fake.host(),
        Some(fake.port()),
        Some(fake.port()),
    )
    .unwrap();
    let facade = EsclFacade::start(Some(rec)).unwrap();
    let settings = escl::scan_settings_xml(&hp_m177::ScanRequest {
        source: hp_m177::ScanSource::Adf,
        color: hp_m177::ColorMode::Gray,
        dpi: 300,
        format: hp_m177::OutputFormat::Pdf,
        output: None,
        region: None,
    });
    let created = t
        .post(
            &format!("{}/eSCL/ScanJobs", facade.url()),
            settings.as_bytes(),
            "text/xml",
        )
        .unwrap();
    assert_eq!(created.status, 201, "{}", created.text());
    let loc = created.header("Location").unwrap().to_string();
    let url = format!("{}{}/NextDocument", facade.url(), loc);
    let doc = t.get(&url).unwrap();
    assert!(imagefmt::is_pdf(&doc.body));
    let ticket = fake.last_ticket().unwrap();
    assert_eq!(ticket.source, hp_m177::ScanSource::Adf);
    assert_eq!(ticket.color, hp_m177::ColorMode::Gray);
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn next_document_url(origin: &str, location: &str) -> String {
    if location.starts_with("http") {
        format!("{}/NextDocument", location.trim_end_matches('/'))
    } else {
        format!(
            "{}{}/NextDocument",
            origin.trim_end_matches('/'),
            location.trim_end_matches('/')
        )
    }
}

fn hit_escl_once(t: &UreqTransport, origin: &str, fake: &FakeDevice, label: &str) {
    let base = format!("{origin}/eSCL");
    let caps = t.get(&format!("{base}/ScannerCapabilities")).unwrap();
    assert!(caps.is_success(), "{label} caps {}", caps.status);
    assert!(
        facade::capabilities_mention_required_features(&caps.text()),
        "{label} caps XML: {}",
        caps.text()
    );
    let status = t.get(&format!("{base}/ScannerStatus")).unwrap();
    assert!(status.is_success(), "{label} status {}", status.status);
    let settings = escl::scan_settings_xml(&hp_m177::ScanRequest {
        source: hp_m177::ScanSource::Platen,
        color: hp_m177::ColorMode::Color,
        dpi: 300,
        format: hp_m177::OutputFormat::Jpeg,
        output: None,
        region: None,
    });
    let created = t
        .post(&format!("{base}/ScanJobs"), settings.as_bytes(), "text/xml")
        .unwrap();
    assert_eq!(created.status, 201, "{label} ScanJobs {}", created.text());
    let loc = created.header("Location").expect("Location").to_string();
    let doc = t.get(&next_document_url(origin, &loc)).unwrap();
    assert!(doc.is_success(), "{label} NextDocument {}", doc.status);
    assert!(imagefmt::is_jpeg(&doc.body) || imagefmt::is_pdf(&doc.body));
    let ticket = fake.last_ticket().expect("bridge forwarded SOAP");
    assert_eq!(ticket.source, hp_m177::ScanSource::Platen);
    assert_eq!(ticket.color, hp_m177::ColorMode::Color);
}

#[test]
fn bridge_binary_http_twice() {
    let fake = FakeDevice::start().unwrap();
    let mut store = Store::open(home()).unwrap();
    let t = UreqTransport::default();
    add_by_address(
        &mut store,
        &t,
        &fake.host(),
        Some(fake.port()),
        Some(fake.port()),
    )
    .unwrap();
    let home_dir = store.path.parent().unwrap().to_path_buf();

    for i in 1..=2 {
        let port = free_port();
        let bind = format!("127.0.0.1:{port}");
        let mut child = Command::new(env!("CARGO_BIN_EXE_hp-m177-bridge"))
            .env("HP_M177_HOME", &home_dir)
            .args(["--port", &port.to_string(), "--bind", &bind, "--no-advertise"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn hp-m177-bridge");
        let origin = format!("http://{bind}");
        let mut ready = false;
        for _ in 0..40 {
            if let Ok(r) = t.get(&format!("{origin}/eSCL/ScannerCapabilities")) {
                if r.is_success() {
                    ready = true;
                    break;
                }
            }
            thread::sleep(Duration::from_millis(50));
        }
        assert!(ready, "bridge run-{i} never answered ScannerCapabilities");
        hit_escl_once(&t, &origin, &fake, &format!("bridge-run-{i}"));
        let _ = child.kill();
        let _ = child.wait();
    }
}
