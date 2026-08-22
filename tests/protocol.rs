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
        region: None,
    };
    let out = scan(&t, &rec, &req).expect("scan");
    match format {
        OutputFormat::Jpeg => assert!(imagefmt::is_jpeg(&out.bytes), "expected JPEG SOI/EOI"),
        OutputFormat::Pdf => {
            assert!(imagefmt::is_pdf(&out.bytes), "expected %PDF … %%EOF");
            assert!(out.bytes.windows(b"/Type /Page".len()).any(|w| w == b"/Type /Page"));
        }
        OutputFormat::Tiff => assert!(imagefmt::is_tiff(&out.bytes), "expected TIFF header"),
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
    assert!(
        rec.name.contains("M177fw"),
        "add_by_address must store the probed M177fw name, got {}",
        rec.name
    );
    assert_eq!(
        hp_m177::model::airscan_instance_name(&rec.name),
        hp_m177::APP_DISPLAY_NAME,
        "M177fw Image Capture instance must not change in 0.3.0"
    );
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
        region: None,
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
fn add_uses_wsd_when_device_has_escl_caps_but_soap_probe_is_down() {
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
        .expect("add should prefer WSD when SOAP is down");
    match rec.job {
        hp_m177::JobProtocol::Wsd { port } => assert_eq!(port, 3911),
        other => panic!("expected WSD, got {other:?}"),
    }
}

#[test]
fn soap_timeout_falls_back_to_wsd_dib() {
    use hp_m177::imagefmt;
    use hp_m177::model::{ColorMode, DeviceRecord, JobProtocol, OutputFormat, ScanSource};
    use hp_m177::transport::{FnTransport, HttpResponse};
    use std::sync::{Arc, Mutex};

    let bmp = imagefmt::solid_bmp_bgra(2, 2, 0, 0, 255);
    let mut mtom = Vec::new();
    mtom.extend_from_slice(
        b"--==b\r\nContent-Type: application/xop+xml\r\n\r\n<SOAP/>\r\n--==b\r\nContent-Type: image/bmp\r\n\r\n",
    );
    mtom.extend_from_slice(&bmp);
    mtom.extend_from_slice(b"\r\n--==b--\r\n");
    let create = br#"<?xml version="1.0"?><Envelope><JobId>7</JobId><JobToken>tok-7</JobToken></Envelope>"#;
    let sent = Arc::new(Mutex::new(Vec::<String>::new()));
    let sent2 = sent.clone();
    let t = FnTransport {
        f: move |req| {
            sent2.lock().unwrap().push(req.url.clone());
            if req.url.contains(":8289") {
                return Ok(HttpResponse {
                    status: 200,
                    headers: vec![],
                    body: br#"<?xml version="1.0"?><SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope"><SOAP-ENV:Body><SOAP-ENV:Fault><SOAP-ENV:Code><SOAP-ENV:Value>SOAP-ENV:Sender</SOAP-ENV:Value></SOAP-ENV:Code><SOAP-ENV:Reason><SOAP-ENV:Text>Error 4</SOAP-ENV:Text></SOAP-ENV:Reason></SOAP-ENV:Fault></SOAP-ENV:Body></SOAP-ENV:Envelope>"#.to_vec(),
                });
            }
            if req.url.contains("/scanner") {
                let text = String::from_utf8_lossy(&req.body);
                if text.contains("CreateScanJob") {
                    assert!(text.contains("<sca:Format>dib</sca:Format>"));
                    return Ok(HttpResponse {
                        status: 200,
                        headers: vec![],
                        body: create.to_vec(),
                    });
                }
                if text.contains("RetrieveImage") {
                    return Ok(HttpResponse {
                        status: 200,
                        headers: vec![(
                            "Content-Type".into(),
                            "multipart/related; type=application/xop+xml".into(),
                        )],
                        body: mtom.clone(),
                    });
                }
            }
            Err(hp_m177::Error::Transport {
                url: req.url,
                detail: "unexpected".into(),
            })
        },
    };
    let rec = DeviceRecord {
        id: "t".into(),
        name: "M177fw".into(),
        host: "192.168.50.14".into(),
        job: JobProtocol::Soap { port: 8289 },
        has_escl_caps: true,
        has_platen: true,
        has_adf: true,
        uuid: None,
    };
    let req = ScanRequest {
        source: ScanSource::Platen,
        color: ColorMode::Color,
        dpi: 300,
        format: OutputFormat::Jpeg,
        output: None,
        region: None,
    };
    let out = scan(&t, &rec, &req).expect("WSD fallback scan");
    assert!(imagefmt::is_jpeg(&out.bytes));
    let urls = sent.lock().unwrap().clone();
    assert!(urls.iter().any(|u| u.contains("/scanner")));
}

#[test]
fn platen_color_tiff_against_fake() {
    scan_combo(ScanSource::Platen, ColorMode::Color, OutputFormat::Tiff);
}

#[test]
fn wsd_only_scan_against_listening_fake() {
    use hp_m177::fake::FakeOptions;
    use hp_m177::model::{ColorMode, DeviceRecord, JobProtocol, OutputFormat, ScanSource};
    let fake = FakeDevice::start_with(FakeOptions {
        soap_dead: true,
        ..FakeOptions::default()
    })
    .unwrap();
    let rec = DeviceRecord {
        id: "w".into(),
        name: "M177fw".into(),
        host: fake.host(),
        job: JobProtocol::Wsd { port: fake.port() },
        has_escl_caps: true,
        has_platen: true,
        has_adf: true,
        uuid: None,
    };
    let t = UreqTransport::default();
    let req = ScanRequest {
        source: ScanSource::Platen,
        color: ColorMode::Color,
        dpi: 300,
        format: OutputFormat::Jpeg,
        output: None,
        region: None,
    };
    let out = scan(&t, &rec, &req).expect("WSD-only scan");
    assert!(imagefmt::is_jpeg(&out.bytes));
    let xml = fake.last_wsd_create_xml();
    assert!(xml.contains("CreateScanJob"), "client sent WSD CreateScanJob");
    assert!(xml.contains("dib"), "ticket format is dib");
}

#[test]
fn add_by_address_selects_listening_wsd_when_soap_is_dead() {
    use hp_m177::fake::FakeOptions;
    let fake = FakeDevice::start_with(FakeOptions {
        soap_dead: true,
        ..FakeOptions::default()
    })
    .unwrap();
    let mut store = Store::open(unique_home()).unwrap();
    let t = UreqTransport::default();
    let rec = add_by_address(
        &mut store,
        &t,
        &fake.host(),
        Some(fake.port()),
        Some(fake.port()),
    )
    .expect("add via WSD");
    match rec.job {
        hp_m177::JobProtocol::Wsd { port } => assert_eq!(port, fake.port()),
        other => panic!("expected WSD on fake port, got {other:?}"),
    }
}

#[test]
fn soap_create_fault_falls_back_to_listening_wsd() {
    use hp_m177::fake::FakeOptions;
    use hp_m177::model::{ColorMode, DeviceRecord, JobProtocol, OutputFormat, ScanSource};
    let fake = FakeDevice::start_with(FakeOptions {
        soap_create_fault: true,
        ..FakeOptions::default()
    })
    .unwrap();
    let rec = DeviceRecord {
        id: "s".into(),
        name: "M177fw".into(),
        host: fake.host(),
        job: JobProtocol::Soap { port: fake.port() },
        has_escl_caps: true,
        has_platen: true,
        has_adf: true,
        uuid: None,
    };
    let t = UreqTransport::default();
    let req = ScanRequest {
        source: ScanSource::Platen,
        color: ColorMode::Color,
        dpi: 300,
        format: OutputFormat::Pdf,
        output: None,
        region: None,
    };
    let started = std::time::Instant::now();
    let out = scan(&t, &rec, &req).expect("SOAP fault then WSD");
    assert!(started.elapsed().as_secs() < 5, "must not spin on SOAP fault");
    assert!(imagefmt::is_pdf(&out.bytes));
    assert!(fake.last_wsd_create_xml().contains("dib"));
}

#[test]
fn soap_empty_retrieve_falls_back_to_listening_wsd() {
    use hp_m177::fake::FakeOptions;
    use hp_m177::model::{ColorMode, DeviceRecord, JobProtocol, OutputFormat, ScanSource};
    let fake = FakeDevice::start_with(FakeOptions {
        retrieve_empty: true,
        ..FakeOptions::default()
    })
    .unwrap();
    let rec = DeviceRecord {
        id: "e".into(),
        name: "M177fw".into(),
        host: fake.host(),
        job: JobProtocol::Soap { port: fake.port() },
        has_escl_caps: true,
        has_platen: true,
        has_adf: true,
        uuid: None,
    };
    let t = UreqTransport::default();
    let req = ScanRequest {
        source: ScanSource::Platen,
        color: ColorMode::Color,
        dpi: 300,
        format: OutputFormat::Jpeg,
        output: None,
        region: None,
    };
    let out = scan(&t, &rec, &req).expect("empty retrieve then WSD");
    assert!(imagefmt::is_jpeg(&out.bytes));
}

#[test]
fn get_job_info_fault_does_not_spin() {
    use hp_m177::fake::FakeOptions;
    use hp_m177::model::{ColorMode, DeviceRecord, JobProtocol, OutputFormat, ScanSource};
    let fake = FakeDevice::start_with(FakeOptions {
        get_job_info_fault: true,
        ..FakeOptions::default()
    })
    .unwrap();
    let rec = DeviceRecord {
        id: "g".into(),
        name: "M177fw".into(),
        host: fake.host(),
        job: JobProtocol::Soap { port: fake.port() },
        has_escl_caps: true,
        has_platen: true,
        has_adf: true,
        uuid: None,
    };
    let t = UreqTransport::default();
    let req = ScanRequest {
        source: ScanSource::Platen,
        color: ColorMode::Color,
        dpi: 300,
        format: OutputFormat::Jpeg,
        output: None,
        region: None,
    };
    let started = std::time::Instant::now();
    let out = scan(&t, &rec, &req);
    assert!(started.elapsed().as_secs() < 5, "GetJobInfo fault must not wait 90s");
    // Fault is Protocol; fallback to WSD should still produce an image.
    assert!(imagefmt::is_jpeg(&out.expect("fallback after GetJobInfo fault").bytes));
}

#[test]
fn soap_retrieve_http_error_falls_back_to_listening_wsd() {
    use hp_m177::fake::FakeOptions;
    use hp_m177::model::{ColorMode, DeviceRecord, JobProtocol, OutputFormat, ScanSource};
    let fake = FakeDevice::start_with(FakeOptions {
        retrieve_http_error: true,
        ..FakeOptions::default()
    })
    .unwrap();
    let rec = DeviceRecord {
        id: "h".into(),
        name: "M177fw".into(),
        host: fake.host(),
        job: JobProtocol::Soap { port: fake.port() },
        has_escl_caps: true,
        has_platen: true,
        has_adf: true,
        uuid: None,
    };
    let t = UreqTransport::default();
    let req = ScanRequest {
        source: ScanSource::Platen,
        color: ColorMode::Color,
        dpi: 300,
        format: OutputFormat::Jpeg,
        output: None,
        region: None,
    };
    let started = std::time::Instant::now();
    let out = scan(&t, &rec, &req).expect("HTTP 500 retrieve then WSD");
    assert!(started.elapsed().as_secs() < 5, "must not spin on retrieve HTTP 500");
    assert!(imagefmt::is_jpeg(&out.bytes));
    assert!(fake.last_wsd_create_xml().contains("dib"));
}

#[test]
fn soap_busy_error13_retries_then_scans() {
    use hp_m177::fake::FakeOptions;
    use hp_m177::model::{ColorMode, DeviceRecord, JobProtocol, OutputFormat, ScanSource};
    let fake = FakeDevice::start_with(FakeOptions {
        soap_busy_creates: 2,
        ..FakeOptions::default()
    })
    .unwrap();
    let rec = DeviceRecord {
        id: "b".into(),
        name: "M177fw".into(),
        host: fake.host(),
        job: JobProtocol::Soap { port: fake.port() },
        has_escl_caps: true,
        has_platen: true,
        has_adf: true,
        uuid: None,
    };
    let t = UreqTransport::default();
    let req = ScanRequest {
        source: ScanSource::Platen,
        color: ColorMode::Color,
        dpi: 300,
        format: OutputFormat::Jpeg,
        output: None,
        region: None,
    };
    let started = std::time::Instant::now();
    let out = scan(&t, &rec, &req).expect("retry after Error 13");
    assert!(started.elapsed().as_secs() < 5);
    assert!(imagefmt::is_jpeg(&out.bytes));
    assert!(fake.last_create_job_xml().contains("CreateScanJob"));
}

#[test]
fn live_get_job_info_fixture_parses() {
    let xml = include_str!("../fixtures/live/soap-GetJobInfo.xml");
    let info = hp_m177::soap::parse_job_info(xml).unwrap();
    assert!(info.image_ready() || info.finished() || !info.job_id.is_empty());
}

#[test]
fn hung_soap_get_scanner_elements_does_not_burn_60s() {
    use hp_m177::model::{ColorMode, DeviceRecord, JobProtocol, OutputFormat, ScanSource};
    use std::net::TcpListener;
    use std::thread;
    use std::time::{Duration, Instant};

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            thread::sleep(Duration::from_secs(120));
            drop(stream);
        }
    });
    thread::sleep(Duration::from_millis(30));
    let rec = DeviceRecord {
        id: "hang".into(),
        name: "M177fw".into(),
        host: "127.0.0.1".into(),
        job: JobProtocol::Soap { port },
        has_escl_caps: false,
        has_platen: true,
        has_adf: true,
        uuid: None,
    };
    let t = UreqTransport::default();
    let req = ScanRequest {
        source: ScanSource::Platen,
        color: ColorMode::Color,
        dpi: 300,
        format: OutputFormat::Jpeg,
        output: None,
        region: None,
    };
    let started = Instant::now();
    let _ = scan(&t, &rec, &req);
    let elapsed = started.elapsed();
    assert!(
        elapsed.as_secs() < 40,
        "pre-job SOAP caps must use the 8s job bound, not 60s HTTP default (took {elapsed:?})"
    );
}

#[test]
fn escl_scanjobs_404_still_returns_pixels_via_soap() {
    use hp_m177::model::{ColorMode, DeviceRecord, JobProtocol, OutputFormat, ScanSource};
    let fake = FakeDevice::start().unwrap();
    let rec = DeviceRecord {
        id: "escl404".into(),
        name: "M177fw".into(),
        host: fake.host(),
        job: JobProtocol::Escl { port: fake.port() },
        has_escl_caps: true,
        has_platen: true,
        has_adf: true,
        uuid: None,
    };
    let t = UreqTransport::default();
    let req = ScanRequest {
        source: ScanSource::Platen,
        color: ColorMode::Color,
        dpi: 300,
        format: OutputFormat::Jpeg,
        output: None,
        region: None,
    };
    let out = scan(&t, &rec, &req).expect("eSCL 404 then SOAP");
    assert!(imagefmt::is_jpeg(&out.bytes));
    assert!(
        fake.last_create_job_xml().contains("CreateScanJob"),
        "SOAP CreateScanJob after native ScanJobs 404"
    );
}

#[test]
fn adf_soap_empty_retrieve_still_tries_wsd() {
    use hp_m177::fake::FakeOptions;
    use hp_m177::model::{ColorMode, DeviceRecord, JobProtocol, OutputFormat, ScanSource};
    let fake = FakeDevice::start_with(FakeOptions {
        retrieve_empty: true,
        ..FakeOptions::default()
    })
    .unwrap();
    let rec = DeviceRecord {
        id: "adf-empty".into(),
        name: "M177fw".into(),
        host: fake.host(),
        job: JobProtocol::Soap { port: fake.port() },
        has_escl_caps: true,
        has_platen: true,
        has_adf: true,
        uuid: None,
    };
    let t = UreqTransport::default();
    let req = ScanRequest {
        source: ScanSource::Adf,
        color: ColorMode::Color,
        dpi: 300,
        format: OutputFormat::Jpeg,
        output: None,
        region: None,
    };
    let out = scan(&t, &rec, &req);
    assert!(
        !fake.last_wsd_create_xml().is_empty(),
        "ADF empty SOAP retrieve must POST WSD CreateScanJob, xml={:?}",
        fake.last_wsd_create_xml()
    );
    assert!(
        out.is_ok() || matches!(out, Err(hp_m177::Error::AdfEmpty)),
        "after WSD attempt: {:?}",
        out.err()
    );
    if let Ok(scanned) = out {
        assert!(imagefmt::is_jpeg(&scanned.bytes));
    }
}

#[test]
fn shipped_converters_bitfields_gray_tiff_pdf_dpi_and_dime_cf() {
    let bmp = imagefmt::bitfields_bgra_bmp(2, 2, 0, 0, 255);
    let (w, h, rgb) = imagefmt::decode_bmp_or_dib(&bmp).unwrap();
    assert_eq!((w, h), (2, 2));
    assert_eq!(&rgb[0..3], &[255, 0, 0]);

    let gray = vec![10u8, 20, 30, 40];
    let gj = imagefmt::gray_to_jpeg(&gray, 2, 2, 80).unwrap();
    let tiff = imagefmt::jpeg_to_tiff(&gj, 300).unwrap();
    assert!(imagefmt::is_tiff(&tiff));
    let n = u16::from_le_bytes(tiff[8..10].try_into().unwrap()) as usize;
    let mut photo = 0u32;
    let mut spp = 0u32;
    for i in 0..n {
        let o = 10 + i * 12;
        let tag = u16::from_le_bytes(tiff[o..o + 2].try_into().unwrap());
        let val = u32::from_le_bytes(tiff[o + 8..o + 12].try_into().unwrap());
        if tag == 262 {
            photo = val;
        }
        if tag == 277 {
            spp = val;
        }
    }
    assert_eq!(photo, 1);
    assert_eq!(spp, 1);

    let color_jpeg = imagefmt::rgb_to_jpeg(&[255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0], 2, 2, 80)
        .unwrap();
    let pdf = imagefmt::jpeg_to_pdf_at_dpi(&color_jpeg, 300).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    assert!(
        text.contains("0.48"),
        "300dpi 2px MediaBox should be 0.48pt: {text}"
    );

    let chunked = include_bytes!("../fixtures/dime-jpeg-chunked.bin");
    let jpeg = hp_m177::dime::extract_image(chunked).unwrap();
    assert_eq!(&jpeg[..2], &[0xff, 0xd8]);
    assert_eq!(&jpeg[jpeg.len() - 2..], &[0xff, 0xd9]);
}

fn soap_rec(fake: &FakeDevice, id: &str) -> DeviceRecord {
    use hp_m177::model::JobProtocol;
    DeviceRecord {
        id: id.into(),
        name: "M177fw".into(),
        host: fake.host(),
        job: JobProtocol::Soap { port: fake.port() },
        has_escl_caps: true,
        has_platen: true,
        has_adf: true,
        uuid: None,
    }
}

fn platen_jpeg() -> ScanRequest {
    ScanRequest {
        source: ScanSource::Platen,
        color: ColorMode::Color,
        dpi: 300,
        format: OutputFormat::Jpeg,
        output: None,
        region: None,
    }
}

#[test]
fn hung_get_job_info_is_bounded() {
    use hp_m177::fake::FakeOptions;
    let fake = FakeDevice::start_with(FakeOptions {
        hang_get_job_info: true,
        ..FakeOptions::default()
    })
    .unwrap();
    let t = UreqTransport::default();
    let started = std::time::Instant::now();
    let out = scan(&t, &soap_rec(&fake, "hang-info"), &platen_jpeg());
    let elapsed = started.elapsed();
    assert!(
        elapsed.as_secs() < 20,
        "hung GetJobInfo must use 4s poll timeout, took {elapsed:?}"
    );
    assert!(imagefmt::is_jpeg(&out.expect("WSD fallback").bytes));
}

#[test]
fn hung_retrieve_image_is_bounded() {
    use hp_m177::fake::FakeOptions;
    let fake = FakeDevice::start_with(FakeOptions {
        hang_retrieve: true,
        ..FakeOptions::default()
    })
    .unwrap();
    let t = UreqTransport::default();
    let started = std::time::Instant::now();
    let out = scan(&t, &soap_rec(&fake, "hang-retr"), &platen_jpeg());
    let elapsed = started.elapsed();
    assert!(
        elapsed.as_secs() < 35,
        "hung SOAP RetrieveImage must use 20s timeout, took {elapsed:?}"
    );
    assert!(imagefmt::is_jpeg(&out.expect("WSD fallback").bytes));
}

#[test]
fn hung_wsd_retrieve_is_bounded() {
    use hp_m177::fake::FakeOptions;
    use hp_m177::model::JobProtocol;
    let fake = FakeDevice::start_with(FakeOptions {
        hang_wsd_retrieve: true,
        ..FakeOptions::default()
    })
    .unwrap();
    let rec = DeviceRecord {
        id: "hang-wsd".into(),
        name: "M177fw".into(),
        host: fake.host(),
        job: JobProtocol::Wsd { port: fake.port() },
        has_escl_caps: true,
        has_platen: true,
        has_adf: true,
        uuid: None,
    };
    let t = UreqTransport::default();
    let started = std::time::Instant::now();
    let out = scan(&t, &rec, &platen_jpeg());
    let elapsed = started.elapsed();
    assert!(
        elapsed.as_secs() < 100,
        "hung WSD RetrieveImage must use the 90s retrieve bound, took {elapsed:?}"
    );
    assert!(out.is_err(), "silent WSD retrieve should not yield pixels");
}
