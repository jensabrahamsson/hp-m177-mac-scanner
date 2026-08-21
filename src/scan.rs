//! Unified scan job cycle. CLI, GUI, and the eSCL facade all call `scan()`.

use crate::dime;
use crate::error::{Error, Result};
use crate::imagefmt;
use crate::model::{
    DeviceRecord, JobProtocol, OutputFormat, ScanOutput, ScanRequest, ScanSource,
    DEFAULT_SOAP_PORT, DEFAULT_WSD_PORT,
};
use crate::soap;
use crate::transport::{HttpRequest, HttpResponse, Transport};
use crate::wsd;
use std::thread;
use std::time::{Duration, Instant};

const SOAP_FAST_TIMEOUT: Duration = Duration::from_secs(8);
const SOAP_JOBINFO_TIMEOUT: Duration = Duration::from_secs(4);
const SOAP_JOBINFO_DEADLINE: Duration = Duration::from_secs(20);
const SOAP_RETRIEVE_TIMEOUT: Duration = Duration::from_secs(20);
const WSD_CREATE_TIMEOUT: Duration = Duration::from_secs(8);
const WSD_RETRIEVE_TIMEOUT: Duration = Duration::from_secs(90);
const SOAP_BUSY_RETRIES: u32 = 3;
const SOAP_BUSY_WAIT: Duration = Duration::from_millis(800);

pub fn scan(
    transport: &dyn Transport,
    device: &DeviceRecord,
    req: &ScanRequest,
) -> Result<ScanOutput> {
    req.validate()?;
    match &device.job {
        JobProtocol::Soap { port } => match soap_scan(transport, &device.host, *port, req) {
            Ok(out) => Ok(out),
            Err(e) if should_fallback_to_wsd(&e) => wsd_after_soap(
                transport,
                &device.host,
                *port,
                req,
                e,
            ),
            Err(e) => Err(e),
        },
        JobProtocol::Escl { port } => match escl_scan(transport, &device.host, *port, req) {
            Ok(out) => Ok(out),
            Err(e) if should_fallback_from_escl(&e) => {
                escl_then_soap_wsd(transport, &device.host, *port, req, e)
            }
            Err(e) => Err(e),
        },
        JobProtocol::Wsd { port } => wsd_scan(transport, &device.host, *port, req),
    }
}

fn should_fallback_to_wsd(e: &Error) -> bool {
    match e {
        Error::AdfEmpty | Error::InvalidRequest(_) => false,
        Error::Transport { .. } | Error::Timeout(_) | Error::Http { .. } => true,
        Error::Protocol(msg) => {
            let n = msg.to_ascii_lowercase();
            n.contains("error 4")
                || n.contains("error 13")
                || n.contains("error ")
                || n.contains("tag_mismatch")
                || n.contains("tag mismatch")
                || n.contains("documentformatnotsupported")
                || n.contains("no image")
                || n.contains("retrieveimage")
                || n.contains("fault")
                || n.contains("http ")
                || n.contains("404")
                || n.contains("scanjobs")
        }
        _ => false,
    }
}

fn should_fallback_from_escl(e: &Error) -> bool {
    if should_fallback_to_wsd(e) {
        return true;
    }
    let n = e.to_string().to_ascii_lowercase();
    n.contains("escl") || n.contains("404") || n.contains("scanjobs")
}

fn escl_then_soap_wsd(
    transport: &dyn Transport,
    host: &str,
    escl_port: u16,
    req: &ScanRequest,
    escl_err: Error,
) -> Result<ScanOutput> {
    match soap_scan(transport, host, escl_port, req) {
        Ok(out) => return Ok(out),
        Err(e) if should_fallback_to_wsd(&e) => {
            if let Ok(out) = wsd_after_soap(transport, host, escl_port, req, e) {
                return Ok(out);
            }
        }
        Err(_) => {}
    }
    if escl_port != DEFAULT_SOAP_PORT {
        match soap_scan(transport, host, DEFAULT_SOAP_PORT, req) {
            Ok(out) => return Ok(out),
            Err(e) if should_fallback_to_wsd(&e) => {
                return wsd_after_soap(transport, host, DEFAULT_SOAP_PORT, req, e).map_err(|w| {
                    Error::msg(format!(
                        "eSCL ScanJobs failed ({escl_err}); SOAP/WSD failed ({w})"
                    ))
                });
            }
            Err(e) => {
                return Err(Error::msg(format!(
                    "eSCL ScanJobs failed ({escl_err}); SOAP failed ({e})"
                )));
            }
        }
    }
    wsd_scan(transport, host, DEFAULT_WSD_PORT, req).map_err(|w| {
        Error::msg(format!(
            "eSCL ScanJobs failed ({escl_err}); SOAP/WSD failed ({w})"
        ))
    })
}

fn wsd_after_soap(
    transport: &dyn Transport,
    host: &str,
    soap_port: u16,
    req: &ScanRequest,
    soap_err: Error,
) -> Result<ScanOutput> {
    match wsd_scan(transport, host, soap_port, req) {
        Ok(out) => Ok(out),
        Err(w) => wsd_scan(transport, host, DEFAULT_WSD_PORT, req).map_err(|w2| {
            let soap_s = soap_err.to_string().to_ascii_lowercase();
            if req.source == ScanSource::Adf
                && (soap_s.contains("no image") || soap_s.contains("retrieveimage"))
            {
                Error::AdfEmpty
            } else {
                Error::msg(format!(
                    "SOAP on :{soap_port} failed ({soap_err}); WSD failed ({w}; {w2})"
                ))
            }
        }),
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
    wait_until_idle(transport, host, port);
    let url = format!("http://{host}:{port}/");
    let created = create_job_retrying(transport, &url, req)?;
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
        // Eligible for WSD fallback (including ADF). AdfEmpty is only
        // returned after WSD also produced no pixels.
        return Err(Error::protocol("RetrieveImage returned no image data"));
    }
    let _ = cancel_job(transport, &url, &created.job_id);
    finalize(pages, req)
}

fn wait_until_idle(transport: &dyn Transport, host: &str, port: u16) {
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        let remain = deadline.saturating_duration_since(Instant::now());
        if remain.is_zero() {
            return;
        }
        let t = remain.min(SOAP_FAST_TIMEOUT);
        match fetch_caps_timeout(transport, host, port, t) {
            Ok(caps) if caps.state.eq_ignore_ascii_case("idle") => return,
            Err(Error::Transport { .. }) | Err(Error::Timeout(_)) => return,
            _ => {}
        }
        if Instant::now() >= deadline {
            return;
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn is_device_busy(e: &Error) -> bool {
    let n = e.to_string().to_ascii_lowercase();
    n.contains("error 13")
        || n.contains("busy")
        || n.contains("in use")
        || n.contains("job already")
        || n.contains("device not ready")
}

fn create_job_retrying(
    transport: &dyn Transport,
    url: &str,
    req: &ScanRequest,
) -> Result<soap::CreatedJob> {
    let mut last = Error::protocol("CreateScanJob failed");
    for attempt in 0..SOAP_BUSY_RETRIES {
        let scan_id = uuid::Uuid::new_v4().to_string();
        match create_job(transport, url, req, &scan_id) {
            Ok(job) => return Ok(job),
            Err(e) if is_device_busy(&e) && attempt + 1 < SOAP_BUSY_RETRIES => {
                last = e;
                thread::sleep(SOAP_BUSY_WAIT);
            }
            Err(e) => return Err(e),
        }
    }
    Err(last)
}

fn cancel_job(transport: &dyn Transport, url: &str, job_id: &str) -> Result<()> {
    let xml = soap::cancel_job_xml(job_id);
    let req = HttpRequest::post(url, xml.into_bytes(), soap::SOAP_CONTENT_TYPE)
        .with_timeout(Duration::from_secs(3));
    let _ = transport.execute(req);
    Ok(())
}

fn create_job(
    transport: &dyn Transport,
    url: &str,
    req: &ScanRequest,
    scan_id: &str,
) -> Result<soap::CreatedJob> {
    let post = |xml: String| {
        transport.execute(
            HttpRequest::post(url, xml.into_bytes(), soap::SOAP_CONTENT_TYPE)
                .with_timeout(SOAP_FAST_TIMEOUT),
        )
    };
    let parse_or_detail = |r: crate::transport::HttpResponse| soap::parse_create_job(&r.text());
    match post(soap::create_scan_job_xml(req, scan_id)) {
        Ok(r) => match parse_or_detail(r) {
            Ok(job) => Ok(job),
            Err(first) => match post(soap::create_scan_job_short_xml(req, scan_id)) {
                Ok(r2) => soap::parse_create_job(&r2.text()).or(Err(first)),
                Err(Error::Http { detail, .. }) => {
                    soap::parse_create_job(&detail).or(Err(first))
                }
                Err(_) => Err(first),
            },
        },
        Err(Error::Http { status, detail, .. }) if status == 400 || status == 500 => {
            if let Ok(job) = soap::parse_create_job(&detail) {
                return Ok(job);
            }
            match post(soap::create_scan_job_short_xml(req, scan_id)) {
                Ok(r) => soap::parse_create_job(&r.text()),
                Err(Error::Http { detail: d2, .. }) => soap::parse_create_job(&d2),
                Err(e) => Err(e),
            }
        }
        Err(e) => Err(e),
    }
}

fn wait_for_job(transport: &dyn Transport, url: &str, job_id: &str) -> Result<()> {
    let deadline = Instant::now() + SOAP_JOBINFO_DEADLINE;
    let mut last = String::new();
    while Instant::now() < deadline {
        let xml = soap::get_job_info_xml(job_id);
        match post_xml_timeout(transport, url, xml.as_bytes(), SOAP_JOBINFO_TIMEOUT) {
            Ok(body) => {
                last = body.clone();
                if let Some(fault) = soap::soap_fault(&body) {
                    return Err(Error::protocol(fault));
                }
                if let Ok(info) = soap::parse_job_info(&body) {
                    if info.image_ready() || info.finished() {
                        return Ok(());
                    }
                } else {
                    // Unparseable and not a fault: skip ahead to RetrieveImage.
                    return Ok(());
                }
            }
            Err(Error::Http { .. }) => return Ok(()),
            Err(e) => return Err(e),
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(Error::Timeout(SOAP_JOBINFO_DEADLINE).or_context(&last))
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
    let req = HttpRequest::post(url, xml.into_bytes(), soap::SOAP_CONTENT_TYPE)
        .with_timeout(SOAP_RETRIEVE_TIMEOUT);
    let resp = match transport.execute(req) {
        Ok(r) => r,
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
                imagefmt::jpeg_to_pdf_at_dpi(&first, req.dpi.max(1))?
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
        .execute(
            HttpRequest::post(
                format!("{base}/ScanJobs"),
                body.into_bytes(),
                "text/xml",
            )
            .with_timeout(SOAP_FAST_TIMEOUT),
        )
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
        match transport.execute(HttpRequest::get(&doc_url).with_timeout(SOAP_FAST_TIMEOUT)) {
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
    fetch_caps_timeout(transport, host, port, SOAP_FAST_TIMEOUT)
}

fn fetch_caps_timeout(
    transport: &dyn Transport,
    host: &str,
    port: u16,
    timeout: Duration,
) -> Result<crate::model::SoapCapabilities> {
    let url = format!("http://{host}:{port}/");
    let body = post_xml_timeout(
        transport,
        &url,
        soap::get_scanner_elements_xml().as_bytes(),
        timeout,
    )?;
    soap::parse_capabilities(&body)
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
    let retrieve_xml = wsd::retrieve_image_xml(&url, &created.job_id, &created.job_token);
    let mut last = None;
    for _ in 0..2 {
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
                thread::sleep(Duration::from_millis(50));
            }
            Err(Error::Http { detail, .. }) => {
                last = Some(detail);
                thread::sleep(Duration::from_millis(50));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::{FakeDevice, FakeOptions};
    use crate::model::{ColorMode, DeviceRecord, OutputFormat};
    use crate::transport::UreqTransport;

    #[test]
    fn job_timeouts_match_short_policy() {
        assert_eq!(SOAP_FAST_TIMEOUT, Duration::from_secs(8));
        assert_eq!(SOAP_JOBINFO_TIMEOUT, Duration::from_secs(4));
        assert_eq!(SOAP_JOBINFO_DEADLINE, Duration::from_secs(20));
        assert_eq!(SOAP_RETRIEVE_TIMEOUT, Duration::from_secs(20));
        assert_eq!(WSD_CREATE_TIMEOUT, Duration::from_secs(8));
        assert!(WSD_RETRIEVE_TIMEOUT <= Duration::from_secs(90));
    }

    #[test]
    fn soap_adf_two_pages_finalize_keeps_one_jpeg() {
        let fake = FakeDevice::start_with(FakeOptions {
            adf_pages: 2,
            paper_in_adf: true,
            ..FakeOptions::default()
        })
        .unwrap();
        let rec = DeviceRecord {
            id: "adf2".into(),
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
        let out = scan(&t, &rec, &req).expect("ADF two-page SOAP");
        assert!(imagefmt::is_jpeg(&out.bytes));
        let sois = out
            .bytes
            .windows(2)
            .filter(|w| *w == [0xff, 0xd8])
            .count();
        assert_eq!(sois, 1, "finalize must keep only the first page");
        let retrieves = fake.retrieve_count();
        assert!(
            retrieves >= 2,
            "SOAP must retrieve more than one ADF page before dropping extras: {retrieves}"
        );
    }
}
