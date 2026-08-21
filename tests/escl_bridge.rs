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
    let doc = wait_next_document(&t, &doc_url, label);
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
    let doc = wait_next_document(&t, &url, "pdf");
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

fn wait_next_document(
    t: &UreqTransport,
    url: &str,
    label: &str,
) -> hp_m177::transport::HttpResponse {
    for _ in 0..80 {
        match t.get(url) {
            Ok(doc) if doc.is_success() && !doc.body.is_empty() => return doc,
            Ok(_) | Err(hp_m177::Error::Http { status: 503, .. }) => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => panic!("{label} NextDocument {e}"),
        }
    }
    panic!("{label} NextDocument never became ready");
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
    let doc = wait_next_document(&t, &next_document_url(origin, &loc), label);
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

#[test]
fn facade_adf_status_and_scan_region() {
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
    facade.set_adf_loaded(true);
    let loaded = t.get(&format!("{base}/ScannerStatus")).unwrap().text();
    assert!(loaded.contains("ScannerAdfLoaded"), "{loaded}");
    facade.set_adf_loaded(false);
    let empty = t.get(&format!("{base}/ScannerStatus")).unwrap().text();
    assert!(empty.contains("ScannerAdfEmpty"), "{empty}");

    let req = hp_m177::ScanRequest {
        source: hp_m177::ScanSource::Platen,
        color: hp_m177::ColorMode::Color,
        dpi: 300,
        format: hp_m177::OutputFormat::Jpeg,
        output: None,
        region: Some(hp_m177::ScanRegion {
            x: 100,
            y: 200,
            width: 3000,
            height: 4000,
        }),
    };
    let settings = escl::scan_settings_xml(&req);
    assert!(settings.contains("ScanRegionWidth>3000"));
    let created = t
        .post(&format!("{base}/ScanJobs"), settings.as_bytes(), "text/xml")
        .unwrap();
    assert_eq!(created.status, 201, "{}", created.text());
    let loc = created.header("Location").unwrap().to_string();
    let _ = wait_next_document(
        &t,
        &format!("{}{}/NextDocument", facade.url(), loc),
        "region",
    );
    let ticket = fake.last_ticket().expect("facade forwarded SOAP");
    let region = ticket.region.expect("ScanRegion on backend ticket");
    assert_eq!(region.width, 3000);
    assert_eq!(region.x, 100);
}

#[test]
fn scanjobs_returns_201_before_pixels_and_status_works() {
    use hp_m177::fake::FakeOptions;
    let fake = FakeDevice::start_with(FakeOptions {
        retrieve_delay_ms: 400,
        ..FakeOptions::default()
    })
    .unwrap();
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
    let settings = escl::scan_settings_xml(&hp_m177::ScanRequest {
        source: hp_m177::ScanSource::Platen,
        color: hp_m177::ColorMode::Color,
        dpi: 300,
        format: hp_m177::OutputFormat::Jpeg,
        output: None,
        region: None,
    });
    let started = std::time::Instant::now();
    let created = t
        .post(&format!("{base}/ScanJobs"), settings.as_bytes(), "text/xml")
        .unwrap();
    assert_eq!(created.status, 201, "{}", created.text());
    assert!(
        started.elapsed().as_millis() < 250,
        "ScanJobs must not wait for backend pixels"
    );
    let status = t.get(&format!("{base}/ScannerStatus")).unwrap();
    assert!(status.is_success(), "status during job {}", status.status);
    assert!(
        status.text().contains("Processing") || status.text().contains("ScannerStatus"),
        "{}",
        status.text()
    );
    let loc = created.header("Location").unwrap().to_string();
    let doc = wait_next_document(
        &t,
        &format!("{}{}/NextDocument", facade.url(), loc),
        "async-job",
    );
    assert!(imagefmt::is_jpeg(&doc.body) || imagefmt::is_pdf(&doc.body));
}

#[test]
fn adf_status_empty_even_when_device_has_adf() {
    use hp_m177::fake::FakeOptions;
    let fake = FakeDevice::start_with(FakeOptions {
        paper_in_adf: false,
        ..FakeOptions::default()
    })
    .unwrap();
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
    assert!(rec.has_adf, "fixture device has an ADF");
    let facade = EsclFacade::start(Some(rec)).unwrap();
    let st = t
        .get(&format!("{}/eSCL/ScannerStatus", facade.url()))
        .unwrap()
        .text();
    assert!(
        st.contains("ScannerAdfEmpty"),
        "has_adf must not imply paper loaded: {st}"
    );
}

#[test]
fn adf_status_loaded_when_paper_in_adf() {
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
    let st = t
        .get(&format!("{}/eSCL/ScannerStatus", facade.url()))
        .unwrap()
        .text();
    assert!(
        st.contains("ScannerAdfLoaded"),
        "PaperInADF true must show loaded: {st}"
    );
}

#[test]
fn shipped_fake_binary_adf_empty_maps_to_scanner_adf_empty() {
    use std::io::{BufRead, BufReader};
    let mut child = Command::new(env!("CARGO_BIN_EXE_hp-m177-fake"))
        .arg("--adf-empty")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hp-m177-fake --adf-empty");
    let stdout = child.stdout.take().expect("stdout");
    let mut line = String::new();
    BufReader::new(stdout).read_line(&mut line).unwrap();
    let port: u16 = line
        .rsplit(':')
        .next()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or_else(|| panic!("parse fake addr from {line}"));
    let t = UreqTransport::default();
    let mut store = Store::open(home()).unwrap();
    let rec = add_by_address(&mut store, &t, "127.0.0.1", Some(port), Some(port))
        .expect("add fake --adf-empty");
    let facade = EsclFacade::start(Some(rec)).unwrap();
    let st = t
        .get(&format!("{}/eSCL/ScannerStatus", facade.url()))
        .unwrap()
        .text();
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        st.contains("ScannerAdfEmpty"),
        "hp-m177-fake --adf-empty must probe as empty ADF: {st}"
    );
}

#[test]
fn pwg_region_in_300ths_maps_to_soap_thousandths() {
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
    let pwg = r#"<?xml version="1.0"?>
<scan:ScanSettings xmlns:scan="http://schemas.hp.com/imaging/escl/2011/05/03" xmlns:pwg="http://www.pwg.org/schemas/2010/12/sm">
  <pwg:InputSource>Platen</pwg:InputSource>
  <scan:ColorMode>RGB24</scan:ColorMode>
  <scan:XResolution>300</scan:XResolution>
  <pwg:DocumentFormat>image/jpeg</pwg:DocumentFormat>
  <pwg:ScanRegion>
    <pwg:XOffset>30</pwg:XOffset>
    <pwg:YOffset>60</pwg:YOffset>
    <pwg:Width>2550</pwg:Width>
    <pwg:Height>3300</pwg:Height>
  </pwg:ScanRegion>
</scan:ScanSettings>"#;
    let created = t
        .post(
            &format!("{}/eSCL/ScanJobs", facade.url()),
            pwg.as_bytes(),
            "text/xml",
        )
        .unwrap();
    assert_eq!(created.status, 201, "{}", created.text());
    let loc = created.header("Location").unwrap().to_string();
    let _ = wait_next_document(
        &t,
        &format!("{}{}/NextDocument", facade.url(), loc),
        "pwg-region",
    );
    let region = fake
        .last_ticket()
        .expect("SOAP ticket")
        .region
        .expect("region");
    assert_eq!(region.width, 8500);
    assert_eq!(region.height, 11000);
    assert_eq!(region.x, escl::pwg_300ths_to_thousandths(30));
}

#[test]
fn adf_status_loaded_when_saved_job_protocol_is_wsd() {
    use hp_m177::model::{DeviceRecord, JobProtocol};
    let fake = FakeDevice::start().unwrap();
    let rec = DeviceRecord {
        id: "wsd-adf".into(),
        name: "M177fw".into(),
        host: fake.host(),
        job: JobProtocol::Wsd { port: fake.port() },
        has_escl_caps: true,
        has_platen: true,
        has_adf: true,
        uuid: None,
    };
    let facade = EsclFacade::start(Some(rec)).unwrap();
    let t = UreqTransport::default();
    let st = t
        .get(&format!("{}/eSCL/ScannerStatus", facade.url()))
        .unwrap()
        .text();
    assert!(
        st.contains("ScannerAdfLoaded"),
        "WSD-saved devices must still probe SOAP PaperInADF: {st}"
    );
}
