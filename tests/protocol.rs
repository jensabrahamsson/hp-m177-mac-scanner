//! Drive the shipped scan/add/DIME/eSCL code against a protocol-accurate
//! fake M177fw. These tests do not re-implement the decoder or hard-code
//! pixel dumps; they inspect the job the client actually sent.

use hp_m177::fake::FakeDevice;
use hp_m177::imagefmt;
use hp_m177::model::{ColorMode, OutputFormat, ScanRequest, ScanSource};
use hp_m177::scan;
use hp_m177::store::Store;
use hp_m177::transport::UreqTransport;
use hp_m177::{add_by_address, DeviceRecord};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn scratch_dir() -> PathBuf {
    let base = std::env::temp_dir().join("hp-m177-protocol-tests");
    std::fs::create_dir_all(&base).unwrap();
    base
}

fn unique_home() -> PathBuf {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = scratch_dir().join(format!("home-{n}"));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn add_fake(fake: &FakeDevice) -> (Store, DeviceRecord) {
    let mut store = Store::open(unique_home()).unwrap();
    let t = UreqTransport::default();
    let rec = add_by_address(
        &mut store,
        &t,
        &fake.host(),
        Some(fake.port()),
        Some(fake.port()),
    )
    .expect("add_by_address against fake");
    (store, rec)
}

fn scan_combo(source: ScanSource, color: ColorMode, format: OutputFormat) {
    let fake = FakeDevice::start().expect("fake");
    let (_store, rec) = add_fake(&fake);
    let t = UreqTransport::default();
    let req = ScanRequest {
        source,
        color,
        dpi: 300,
        format,
        output: None,
    };
    let out = scan(&t, &rec, &req).expect("scan");
    match format {
        OutputFormat::Jpeg => assert!(imagefmt::is_jpeg(&out.bytes), "expected JPEG SOI/EOI"),
        OutputFormat::Pdf => {
            assert!(imagefmt::is_pdf(&out.bytes), "expected %PDF … %%EOF");
            assert!(out.bytes.windows(b"/Type /Page".len()).any(|w| w == b"/Type /Page"));
        }
    }
    let ticket = fake
        .last_ticket()
        .expect("fake recorded CreateScanJob");
    assert_eq!(ticket.source, source, "job source the client sent");
    assert_eq!(ticket.color, color, "job color the client sent");
    assert_eq!(ticket.dpi, 300);
    let xml = fake.last_create_job_xml();
    assert!(
        xml.contains("CreateScanJob"),
        "SOAP CreateScanJob must be what left the client"
    );
    assert!(xml.contains(source.soap_name()));
    assert!(xml.contains(color.soap_name()));
}

#[test]
fn add_by_address_persists_usable_record() {
    let fake = FakeDevice::start().unwrap();
    let (store, rec) = add_fake(&fake);
    assert_eq!(rec.host, fake.host());
    match rec.job {
        hp_m177::JobProtocol::Soap { port } => assert_eq!(port, fake.port()),
        other => panic!("expected SOAP job protocol, got {other:?}"),
    }
    assert!(rec.has_platen);
    assert!(rec.has_adf);
    let reopened = Store::open(store.path.parent().unwrap()).unwrap();
    let again = reopened.get(&rec.id).unwrap();
    assert_eq!(again.host, rec.host);
}

#[test]
fn platen_color_jpeg() {
    scan_combo(ScanSource::Platen, ColorMode::Color, OutputFormat::Jpeg);
}

#[test]
fn platen_gray_jpeg() {
    scan_combo(ScanSource::Platen, ColorMode::Gray, OutputFormat::Jpeg);
}

#[test]
fn adf_color_jpeg() {
    scan_combo(ScanSource::Adf, ColorMode::Color, OutputFormat::Jpeg);
}

#[test]
fn adf_gray_pdf() {
    scan_combo(ScanSource::Adf, ColorMode::Gray, OutputFormat::Pdf);
}

#[test]
fn platen_color_pdf() {
    scan_combo(ScanSource::Platen, ColorMode::Color, OutputFormat::Pdf);
}

#[test]
fn dime_spec_fixture_is_consumed_by_shipped_decoder() {
    let raw = include_bytes!("../fixtures/dime-jpeg.bin");
    let jpeg = hp_m177::dime::extract_image(raw).unwrap();
    assert!(imagefmt::is_jpeg(&jpeg));
}

#[test]
fn soap_create_job_encode_is_what_scan_sends() {
    let fake = FakeDevice::start().unwrap();
    let (_s, rec) = add_fake(&fake);
    let t = UreqTransport::default();
    let req = ScanRequest {
        source: ScanSource::Platen,
        color: ColorMode::Color,
        dpi: 300,
        format: OutputFormat::Jpeg,
        output: None,
    };
    let _ = scan(&t, &rec, &req).unwrap();
    let xml = fake.last_create_job_xml();
    let expected = hp_m177::soap::create_scan_job_xml(&req, "unused");
    // Same elements, not a hardcoded golden document: both must parse to the same ticket.
    let a = hp_m177::soap::parse_job_ticket(&xml).unwrap();
    let b = hp_m177::soap::parse_job_ticket(&expected).unwrap();
    assert_eq!(a.source, b.source);
    assert_eq!(a.color, b.color);
    assert_eq!(a.dpi, b.dpi);
}

#[test]
fn add_uses_soap_when_device_has_escl_caps_but_soap_probe_is_down() {
    use hp_m177::transport::{FnTransport, HttpResponse};
    let caps = include_str!("../fixtures/live/escl-ScannerCapabilities.xml");
    let t = FnTransport {
        f: |req| {
            if req.url.contains("ScannerCapabilities") {
                Ok(HttpResponse {
                    status: 200,
                    headers: vec![],
                    body: caps.as_bytes().to_vec(),
                })
            } else if req.url.contains("ScanJobs") {
                Err(hp_m177::Error::Http {
                    status: 404,
                    url: req.url,
                    detail: String::new(),
                })
            } else {
                Err(hp_m177::Error::Transport {
                    url: req.url,
                    detail: "soap not answering".into(),
                })
            }
        },
    };
    let mut store = Store::open(unique_home()).unwrap();
    let rec = add_by_address(&mut store, &t, "192.168.50.14", Some(8289), Some(80))
        .expect("add should keep SOAP for this firmware");
    match rec.job {
        hp_m177::JobProtocol::Soap { port } => assert_eq!(port, 8289),
        other => panic!("expected SOAP, got {other:?}"),
    }
}
