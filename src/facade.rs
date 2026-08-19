//! Local eSCL (AirScan) HTTP surface. Image Capture / Preview talk to this
//! process; we translate ScanJobs into the same `scan()` backend the CLI uses.

use crate::error::{Error, Result};
use crate::escl;
use crate::imagefmt;
use crate::model::{DeviceRecord, OutputFormat, PRODUCT_NAME};
use crate::scan;
use std::collections::HashMap;
use std::io::Cursor;
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::thread;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

#[derive(Clone)]
struct Job {
    body: Vec<u8>,
    mime: String,
    consumed: bool,
}

struct Inner {
    jobs: HashMap<String, Job>,
    device: Option<DeviceRecord>,
}

pub struct EsclFacade {
    pub addr: SocketAddr,
    inner: Arc<Mutex<Inner>>,
}

impl EsclFacade {
    pub fn start(device: Option<DeviceRecord>) -> Result<Self> {
        Self::bind("127.0.0.1:0", device)
    }

    pub fn bind(addr: &str, device: Option<DeviceRecord>) -> Result<Self> {
        let listener = TcpListener::bind(addr)?;
        let bound = listener.local_addr()?;
        let inner = Arc::new(Mutex::new(Inner {
            jobs: HashMap::new(),
            device,
        }));
        let server = Server::from_listener(listener, None)
            .map_err(|e| Error::msg(format!("eSCL facade listen: {e}")))?;
        let state = inner.clone();
        thread::Builder::new()
            .name("hp-m177-escl".into())
            .spawn(move || run(server, state))
            .map_err(|e| Error::msg(format!("eSCL facade thread: {e}")))?;
        Ok(Self { addr: bound, inner })
    }

    pub fn set_device(&self, device: DeviceRecord) {
        self.inner.lock().unwrap().device = Some(device);
    }

    pub fn url(&self) -> String {
        format!("http://{}:{}", self.addr.ip(), self.addr.port())
    }
}

fn run(server: Server, inner: Arc<Mutex<Inner>>) {
    loop {
        let req = match server.recv() {
            Ok(r) => r,
            Err(_) => break,
        };
        if let Err(e) = handle(req, &inner) {
            eprintln!("eSCL facade: {e}");
        }
    }
}

fn handle(mut req: Request, inner: &Arc<Mutex<Inner>>) -> Result<()> {
    let mut body = Vec::new();
    std::io::Read::read_to_end(req.as_reader(), &mut body)?;
    let url = req.url().to_string();
    let method = req.method().clone();
    let (status, ctype, payload, extra) = dispatch(&method, &url, &body, inner);
    let mut response = Response::new(
        StatusCode(status),
        Vec::new(),
        Cursor::new(payload),
        None,
        None,
    );
    if let Ok(h) = Header::from_bytes(b"Content-Type", ctype.as_bytes()) {
        response = response.with_header(h);
    }
    for (k, v) in extra {
        if let Ok(h) = Header::from_bytes(k.as_bytes(), v.as_bytes()) {
            response = response.with_header(h);
        }
    }
    req.respond(response)
        .map_err(|e| Error::msg(format!("eSCL respond: {e}")))
}

fn dispatch(
    method: &Method,
    url: &str,
    body: &[u8],
    inner: &Arc<Mutex<Inner>>,
) -> (u16, String, Vec<u8>, Vec<(String, String)>) {
    let path = url.split('?').next().unwrap_or(url);
    if (*method == Method::Get)
        && (path == "/eSCL/ScannerCapabilities" || path == "/ScannerCapabilities")
    {
        return (
            200,
            "text/xml; charset=utf-8".into(),
            escl::default_capabilities_xml().into_bytes(),
            vec![],
        );
    }
    if (*method == Method::Get) && (path == "/eSCL/ScannerStatus" || path == "/ScannerStatus") {
        let jobs: Vec<(String, &str)> = inner
            .lock()
            .unwrap()
            .jobs
            .iter()
            .map(|(id, j)| {
                (
                    id.clone(),
                    if j.consumed { "Completed" } else { "Processing" },
                )
            })
            .collect();
        let xml = escl::status_xml(true, &jobs);
        return (200, "text/xml; charset=utf-8".into(), xml.into_bytes(), vec![]);
    }
    if *method == Method::Post && (path == "/eSCL/ScanJobs" || path == "/ScanJobs") {
        return create_job(body, inner);
    }
    if *method == Method::Get && path.contains("/NextDocument") {
        return next_document(path, inner);
    }
    if *method == Method::Delete && path.contains("/ScanJobs/") {
        if let Some(id) = job_id_from(path) {
            inner.lock().unwrap().jobs.remove(&id);
        }
        return (200, "text/plain".into(), b"ok".to_vec(), vec![]);
    }
    (
        404,
        "text/plain".into(),
        format!("not found {path}").into_bytes(),
        vec![],
    )
}

fn create_job(
    body: &[u8],
    inner: &Arc<Mutex<Inner>>,
) -> (u16, String, Vec<u8>, Vec<(String, String)>) {
    let xml = String::from_utf8_lossy(body);
    let req = match escl::parse_scan_settings(&xml) {
        Ok(r) => r,
        Err(e) => {
            return (
                400,
                "text/plain".into(),
                format!("bad ScanSettings: {e}").into_bytes(),
                vec![],
            );
        }
    };
    let device = {
        let g = inner.lock().unwrap();
        g.device.clone()
    };
    let Some(device) = device else {
        return (
            503,
            "text/plain".into(),
            b"no scanner configured; run hp-m177 add".to_vec(),
            vec![],
        );
    };
    let transport = crate::transport::UreqTransport::new(std::time::Duration::from_secs(60));
    match scan::scan(&transport, &device, &req) {
        Ok(out) => {
            let id = uuid::Uuid::new_v4().to_string();
            let mime = if out.format == OutputFormat::Pdf || imagefmt::is_pdf(&out.bytes) {
                "application/pdf"
            } else {
                "image/jpeg"
            };
            inner.lock().unwrap().jobs.insert(
                id.clone(),
                Job {
                    body: out.bytes,
                    mime: mime.into(),
                    consumed: false,
                },
            );
            let location = format!("/eSCL/ScanJobs/{id}");
            (
                201,
                "text/xml; charset=utf-8".into(),
                format!("<scan:JobUri>{location}</scan:JobUri>").into_bytes(),
                vec![("Location".into(), location)],
            )
        }
        Err(e) => (
            503,
            "text/plain".into(),
            format!("scan failed: {e}").into_bytes(),
            vec![],
        ),
    }
}

fn next_document(
    path: &str,
    inner: &Arc<Mutex<Inner>>,
) -> (u16, String, Vec<u8>, Vec<(String, String)>) {
    let Some(id) = job_id_from(path) else {
        return (404, "text/plain".into(), b"no job".to_vec(), vec![]);
    };
    let mut g = inner.lock().unwrap();
    match g.jobs.get_mut(&id) {
        Some(job) if !job.consumed => {
            job.consumed = true;
            (200, job.mime.clone(), job.body.clone(), vec![])
        }
        Some(_) => (404, "text/plain".into(), b"no more documents".to_vec(), vec![]),
        None => (404, "text/plain".into(), b"unknown job".to_vec(), vec![]),
    }
}

fn job_id_from(path: &str) -> Option<String> {
    // /eSCL/ScanJobs/{id}/NextDocument or /eSCL/ScanJobs/{id}
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let idx = parts.iter().position(|p| *p == "ScanJobs")?;
    parts.get(idx + 1).map(|s| (*s).to_string())
}

pub fn capabilities_mention_required_features(xml: &str) -> bool {
    xml.contains("Platen")
        && xml.contains("Adf")
        && xml.contains("RGB24")
        && xml.contains("Grayscale8")
        && xml.contains("image/jpeg")
        && xml.contains("application/pdf")
}

#[allow(dead_code)]
fn _product() -> &'static str {
    PRODUCT_NAME
}
