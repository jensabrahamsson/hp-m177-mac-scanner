//! Unified scan job cycle. CLI, GUI, and the eSCL facade all call `scan()`.

use crate::dime;
use crate::error::{Error, Result};
use crate::imagefmt;
use crate::model::{
    DeviceRecord, JobProtocol, OutputFormat, ScanOutput, ScanRequest, ScanSource,
    DEFAULT_WSD_PORT,
};
use crate::soap;
use crate::transport::{HttpRequest, HttpResponse, Transport};
use crate::wsd;
use std::thread;
use std::time::{Duration, Instant};

const SOAP_FAST_TIMEOUT: Duration = Duration::from_secs(8);
const WSD_CREATE_TIMEOUT: Duration = Duration::from_secs(40);
const WSD_RETRIEVE_TIMEOUT: Duration = Duration::from_secs(180);

pub fn scan(
    transport: &dyn Transport,
    device: &DeviceRecord,
    req: &ScanRequest,
) -> Result<ScanOutput> {
    req.validate()?;
    match &device.job {
        JobProtocol::Soap { port } => match soap_scan(transport, &device.host, *port, req) {
            Ok(out) => Ok(out),
            Err(Error::AdfEmpty) => Err(Error::AdfEmpty),
            Err(e) if should_fallback_to_wsd(&e) => {
                wsd_scan(transport, &device.host, DEFAULT_WSD_PORT, req).map_err(|w| {
                    Error::msg(format!(
                        "SOAP on :{port} failed ({e}); WSD on :{DEFAULT_WSD_PORT} failed ({w})"
                    ))
                })
            }
            Err(e) => Err(e),
        },
        JobProtocol::Escl { port } => escl_scan(transport, &device.host, *port, req),
        JobProtocol::Wsd { port } => wsd_scan(transport, &device.host, *port, req),
    }
}

fn should_fallback_to_wsd(e: &Error) -> bool {
    match e {
        Error::Transport { .. } | Error::Timeout(_) => true,
        Error::Protocol(msg) => {
            let n = msg.to_ascii_lowercase();
            n.contains("error 4")
                || n.contains("tag_mismatch")
                || n.contains("tag mismatch")
                || n.contains("documentformatnotsupported")
        }
        Error::Http { .. } => true,
        _ => false,
    }
}

fn soap_scan(
    transport: &dyn Transport,
    host: &str,
    port: u16,
    req: &ScanRequest,
) -> Result<ScanOutput> {
    if req.source == ScanSource::Adf {
        // Best-effort empty-ADF check from a fresh GetScannerElements.
        if let Ok(caps) = fetch_caps(transport, host, port) {
            if !caps.paper_in_adf {
                // Still attempt the job: the fake and some firmware report
                // PaperInADF late. The retrieve fault is the source of truth.
            }
        }
    }
    let url = format!("http://{host}:{port}/");
    let scan_id = uuid::Uuid::new_v4().to_string();
    let created = create_job(transport, &url, req, &scan_id)?;
    wait_for_job(transport, &url, &created.job_id)?;
    let mut pages: Vec<Vec<u8>> = Vec::new();
    loop {
        match retrieve(transport, &url, &created.job_id, &created.job_token) {
            Ok(bytes) => pages.push(normalize_image(bytes, req.color)?),
            Err(Error::Protocol(msg)) if soap::is_no_images_fault(&msg) => break,
            Err(Error::Http { detail, .. }) if soap::is_no_images_fault(&detail) => break,
            Err(e) => {
                if pages.is_empty() {
                    return Err(e);
                }
                break;
            }
        }
        if req.source == ScanSource::Platen {
            break;
        }
        if pages.len() >= 50 {
            break;
        }
    }
    if pages.is_empty() {
        if req.source == ScanSource::Adf {
            return Err(Error::AdfEmpty);
        }
        return Err(Error::protocol("RetrieveImage returned no image data"));
    }
    finalize(pages, req)
}

fn create_job(
    transport: &dyn Transport,
    url: &str,
    req: &ScanRequest,
    scan_id: &str,
) -> Result<soap::CreatedJob> {
    let primary = soap::create_scan_job_xml(req, scan_id);
    match post_xml_timeout(transport, url, primary.as_bytes(), SOAP_FAST_TIMEOUT) {
        Ok(body) => match soap::parse_create_job(&body) {
            Ok(job) => return Ok(job),
            Err(first) => {
                let fallback = soap::create_scan_job_short_xml(req, scan_id);
                if let Ok(body) =
                    post_xml_timeout(transport, url, fallback.as_bytes(), SOAP_FAST_TIMEOUT)
                {
                    if let Ok(job) = soap::parse_create_job(&body) {
                        return Ok(job);
                    }
                }
                return Err(first);
            }
        },
        Err(Error::Http { status, .. }) if status == 400 || status == 500 => {
            let fallback = soap::create_scan_job_short_xml(req, scan_id);
            let body = post_xml_timeout(transport, url, fallback.as_bytes(), SOAP_FAST_TIMEOUT)?;
            soap::parse_create_job(&body)
        }
        Err(e) => Err(e),
    }
}

fn wait_for_job(transport: &dyn Transport, url: &str, job_id: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut last = String::new();
    while Instant::now() < deadline {
        let xml = soap::get_job_info_xml(job_id);
        match post_xml(transport, url, xml.as_bytes()) {
            Ok(body) => {
                last = body.clone();
                if let Ok(info) = soap::parse_job_info(&body) {
                    if info.image_ready() || info.finished() {
                        return Ok(());
                    }
                }
            }
            Err(Error::Http { .. }) => {
                // Some firmware skips GetJobInfo; proceed to retrieve.
                return Ok(());
            }
            Err(e) => return Err(e),
        }
        thread::sleep(Duration::from_millis(150));
    }
    Err(Error::Timeout(Duration::from_secs(90)).or_context(&last))
}

trait OrContext {
    fn or_context(self, extra: &str) -> Error;
}

impl OrContext for Error {
    fn or_context(self, extra: &str) -> Error {
        if extra.is_empty() {
            self
        } else {
            Error::msg(format!("{self}; last body: {extra}"))
        }
    }
}

fn retrieve(
    transport: &dyn Transport,
    url: &str,
    job_id: &str,
    token: &str,
) -> Result<Vec<u8>> {
    let xml = soap::retrieve_image_xml(job_id, token);
    let resp = match transport.post(url, xml.as_bytes(), soap::SOAP_CONTENT_TYPE) {
        Ok(r) => r,
        Err(Error::Http { status, detail, .. }) => {
            return Err(Error::protocol(format!("HTTP {status}: {detail}")));
        }
        Err(e) => return Err(e),
    };
    if resp.body.starts_with(b"%PDF") || imagefmt::is_jpeg(&resp.body) {
        return Ok(resp.body);
    }
    if looks_like_dime(&resp) {
        return dime::extract_image(&resp.body);
    }
    // Some stacks return raw JPEG after a SOAP XML prefix without DIME.
    if let Some(soi) = find_sub(&resp.body, &[0xff, 0xd8]) {
        return Ok(resp.body[soi..].to_vec());
    }
    if let Ok(text) = std::str::from_utf8(&resp.body) {
        if let Some(fault) = soap::soap_fault(text) {
            return Err(Error::protocol(fault));
        }
    }
    dime::extract_image(&resp.body)
}

fn looks_like_dime(resp: &HttpResponse) -> bool {
    resp.header("Content-Type")
        .map(|c| c.to_ascii_lowercase().contains("dime"))
        .unwrap_or(false)
        || resp.body.len() >= 12 && (resp.body[0] >> 3) == 1
}

fn normalize_image(bytes: Vec<u8>, color: crate::model::ColorMode) -> Result<Vec<u8>> {
    let jpeg = if imagefmt::is_jpeg(&bytes) || bytes.starts_with(&[0xff, 0xd8]) {
        bytes
    } else if imagefmt::is_pdf(&bytes) {
        return Ok(bytes);
    } else if imagefmt::is_bmp(&bytes) || imagefmt::is_dib(&bytes) {
        imagefmt::raster_to_jpeg(&bytes)?
    } else {
        return Err(Error::protocol(
            "retrieved payload is neither JPEG, PDF, nor BMP",
        ));
    };
    if color == crate::model::ColorMode::Lineart && imagefmt::is_jpeg(&jpeg) {
        match imagefmt::apply_lineart_jpeg(&jpeg) {
            Ok(bw) => Ok(bw),
            Err(_) => Ok(jpeg),
        }
    } else {
        Ok(jpeg)
    }
}

fn finalize(pages: Vec<Vec<u8>>, req: &ScanRequest) -> Result<ScanOutput> {
    let first = pages.into_iter().next().unwrap();
    let bytes = match req.format {
        OutputFormat::Jpeg => {
            if imagefmt::is_jpeg(&first) {
                first
            } else {
                return Err(Error::protocol("device did not return a JPEG"));
            }
        }
        OutputFormat::Pdf => {
            if imagefmt::is_pdf(&first) {
                first
            } else {
                imagefmt::jpeg_to_pdf(&first)?
            }
        }
        OutputFormat::Tiff => {
            if imagefmt::is_tiff(&first) {
                first
            } else if imagefmt::is_jpeg(&first) {
                imagefmt::jpeg_to_tiff(&first, req.dpi)?
            } else if imagefmt::is_bmp(&first) || imagefmt::is_dib(&first) {
                let (w, h, rgb) = imagefmt::decode_bmp_or_dib(&first)?;
                imagefmt::rgb_to_tiff(&rgb, w, h, req.dpi)?
            } else {
                return Err(Error::protocol("cannot build TIFF from scan payload"));
            }
        }
    };
    Ok(ScanOutput {
        bytes,
        format: req.format,
        source: req.source,
        color: req.color,
        dpi: req.dpi,
    })
}

fn escl_scan(
    transport: &dyn Transport,
    host: &str,
    port: u16,
    req: &ScanRequest,
) -> Result<ScanOutput> {
    let base = format!("http://{host}:{port}/eSCL");
    let body = crate::escl::scan_settings_xml(req);
    let resp = transport
        .post(&format!("{base}/ScanJobs"), body.as_bytes(), "text/xml")
        .or_else(|e| match e {
            Error::Http { status, detail, .. } => Err(Error::protocol(format!(
                "eSCL ScanJobs failed ({status}): {detail}"
            ))),
            other => Err(other),
        })?;
    let location = resp
        .header("Location")
        .map(|s| s.to_string())
        .ok_or_else(|| Error::protocol("eSCL ScanJobs response missing Location"))?;
    let doc_url = if location.contains("NextDocument") {
        location
    } else {
        format!("{}/NextDocument", location.trim_end_matches('/'))
    };
    let mut last_err = None;
    for _ in 0..40 {
        match transport.get(&doc_url) {
            Ok(r) if r.is_success() && !r.body.is_empty() => {
                return finalize(vec![r.body], req);
            }
            Ok(r) if r.status == 503 || r.status == 404 => {
                last_err = Some(format!("HTTP {}", r.status));
            }
            Ok(r) => last_err = Some(format!("HTTP {}", r.status)),
            Err(e) => last_err = Some(e.to_string()),
        }
        thread::sleep(Duration::from_millis(200));
    }
    Err(Error::protocol(format!(
        "eSCL NextDocument failed: {}",
        last_err.unwrap_or_else(|| "no body".into())
    )))
}

fn fetch_caps(
    transport: &dyn Transport,
    host: &str,
    port: u16,
) -> Result<crate::model::SoapCapabilities> {
    let url = format!("http://{host}:{port}/");
    let body = post_xml(
        transport,
        &url,
        soap::get_scanner_elements_xml().as_bytes(),
    )?;
    soap::parse_capabilities(&body)
}

fn post_xml(transport: &dyn Transport, url: &str, body: &[u8]) -> Result<String> {
    post_xml_timeout(transport, url, body, Duration::from_secs(60))
}

fn post_xml_timeout(
    transport: &dyn Transport,
    url: &str,
    body: &[u8],
    timeout: Duration,
) -> Result<String> {
    let req = HttpRequest::post(url, body.to_vec(), soap::SOAP_CONTENT_TYPE).with_timeout(timeout);
    match transport.execute(req) {
        Ok(r) => Ok(r.text()),
        Err(Error::Http { detail, .. }) => Ok(detail),
        Err(e) => Err(e),
    }
}

fn wsd_scan(
    transport: &dyn Transport,
    host: &str,
    port: u16,
    req: &ScanRequest,
) -> Result<ScanOutput> {
    let url = wsd::scanner_url(host, port);
    let xml = wsd::create_scan_job_xml(&url, req);
    let created = match transport.execute(wsd::request(
        &url,
        wsd::ACTION_CREATE,
        xml,
        WSD_CREATE_TIMEOUT,
    )) {
        Ok(r) => soap::parse_create_job(&r.text())?,
        Err(Error::Http { detail, .. }) => soap::parse_create_job(&detail)?,
        Err(e) => return Err(e),
    };
    thread::sleep(Duration::from_millis(400));
    let retrieve_xml = wsd::retrieve_image_xml(&url, &created.job_id, &created.job_token);
    let mut last = None;
    for _ in 0..8 {
        match transport.execute(wsd::request(
            &url,
            wsd::ACTION_RETRIEVE,
            retrieve_xml.clone(),
            WSD_RETRIEVE_TIMEOUT,
        )) {
            Ok(r) => {
                let raw = wsd::extract_image(&r.body)?;
                let jpeg = normalize_image(raw, req.color)?;
                return finalize(vec![jpeg], req);
            }
            Err(Error::Http { detail, .. }) if soap::is_no_images_fault(&detail) => {
                if req.source == ScanSource::Adf {
                    return Err(Error::AdfEmpty);
                }
                last = Some(detail);
                thread::sleep(Duration::from_millis(400));
            }
            Err(Error::Http { detail, .. }) => {
                last = Some(detail);
                thread::sleep(Duration::from_millis(400));
            }
            Err(e) => return Err(e),
        }
    }
    Err(Error::protocol(format!(
        "WSD RetrieveImage failed: {}",
        last.unwrap_or_default()
    )))
}

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Write `output` to `req.output` or a generated path in `cwd`.
pub fn write_output(output: &ScanOutput, dest: &std::path::Path) -> Result<std::path::PathBuf> {
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| Error::Output {
                path: dest.to_path_buf(),
                detail: e.to_string(),
            })?;
        }
    }
    std::fs::write(dest, &output.bytes).map_err(|e| Error::Output {
        path: dest.to_path_buf(),
        detail: e.to_string(),
    })?;
    Ok(dest.to_path_buf())
}
