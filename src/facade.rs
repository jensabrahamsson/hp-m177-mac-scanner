//! Local eSCL (AirScan) HTTP surface. Image Capture / Preview talk to this
//! process; we translate ScanJobs into the same `scan()` backend the CLI uses.

use crate::error::{Error, Result};
use crate::escl;
use crate::imagefmt;
use crate::model::{
    DeviceRecord, JobProtocol, OutputFormat, DEFAULT_SOAP_PORT, PRODUCT_NAME,
};
use crate::soap;
use crate::transport::{HttpRequest, Transport, UreqTransport};
use crate::scan;
use std::collections::HashMap;
use std::io::Cursor;
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

#[derive(Clone)]
struct Job {
    body: Vec<u8>,
    mime: String,
    consumed: bool,
    ready: bool,
    error: Option<String>,
}

struct Inner {
    jobs: HashMap<String, Job>,
    device: Option<DeviceRecord>,
    adf_loaded: bool,
    adf_probed_at: Option<Instant>,
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
        // Paper in the feeder is independent of whether the hardware has an ADF.
        let adf_loaded = false;
        let inner = Arc::new(Mutex::new(Inner {
            jobs: HashMap::new(),
            device,
            adf_loaded,
            adf_probed_at: None,
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
        let mut g = self.inner.lock().unwrap();
        g.device = Some(device);
        g.adf_probed_at = None;
    }

    pub fn set_adf_loaded(&self, loaded: bool) {
        let mut g = self.inner.lock().unwrap();
        g.adf_loaded = loaded;
        g.adf_probed_at = Some(Instant::now());
    }

    pub fn refresh_adf_from_device(&self) {
        refresh_adf(&self.inner);
    }

    pub fn adf_loaded(&self) -> bool {
        self.inner.lock().unwrap().adf_loaded
    }

    pub fn url(&self) -> String {
        format!("http://{}:{}", self.addr.ip(), self.addr.port())
    }
}

fn soap_probe_ports(device: &DeviceRecord) -> Vec<u16> {
    let job_port = match device.job {
        JobProtocol::Soap { port } | JobProtocol::Escl { port } | JobProtocol::Wsd { port } => {
            port
        }
    };
    let mut ports = vec![job_port];
    if job_port != DEFAULT_SOAP_PORT {
        ports.push(DEFAULT_SOAP_PORT);
    }
    ports
}

fn refresh_adf(inner: &Arc<Mutex<Inner>>) {
    let (host, ports) = {
        let g = inner.lock().unwrap();
        if let Some(at) = g.adf_probed_at {
            if at.elapsed() < Duration::from_secs(1) {
                return;
            }
        }
        match g.device.as_ref() {
            Some(d) => (d.host.clone(), soap_probe_ports(d)),
            None => return,
        }
    };
    for port in ports {
        if let Some(loaded) = probe_paper_in_adf(&host, port) {
            let mut g = inner.lock().unwrap();
            g.adf_loaded = loaded;
            g.adf_probed_at = Some(Instant::now());
            return;
        }
    }
}

pub fn probe_paper_in_adf(host: &str, port: u16) -> Option<bool> {
    let t = UreqTransport::new(Duration::from_secs(2));
    let url = format!("http://{host}:{port}/");
    let xml = soap::get_scanner_elements_xml();
    let req = HttpRequest::post(url, xml.into_bytes(), soap::SOAP_CONTENT_TYPE)
        .with_timeout(Duration::from_secs(2));
    let body = match t.execute(req) {
        Ok(r) => r.text(),
        Err(Error::Http { detail, .. }) => detail,
        Err(_) => return None,
    };
    soap::parse_capabilities(&body)
        .ok()
        .map(|c| c.paper_in_adf)
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
        let (uuid, model) = {
            let g = inner.lock().unwrap();
            match g.device.as_ref() {
                Some(d) => (
                    d.uuid
                        .clone()
                        .unwrap_or_else(|| escl::DEFAULT_UUID.to_string()),
                    d.name.clone(),
                ),
                None => (
                    escl::DEFAULT_UUID.to_string(),
                    crate::model::PRODUCT_NAME.to_string(),
                ),
            }
        };
        return (
            200,
            "text/xml; charset=utf-8".into(),
            escl::capabilities_xml(&uuid, &model).into_bytes(),
            vec![],
        );
    }
    if (*method == Method::Get) && (path == "/eSCL/ScannerStatus" || path == "/ScannerStatus") {
        refresh_adf(inner);
        let jobs: Vec<(String, &str)> = inner
            .lock()
            .unwrap()
            .jobs
            .iter()
            .map(|(id, j)| {
                (
                    id.clone(),
                    if j.error.is_some() {
                        "Aborted"
                    } else if j.consumed {
                        "Completed"
                    } else {
                        "Processing"
                    },
                )
            })
            .collect();
        let adf_empty = !inner.lock().unwrap().adf_loaded;
        let xml = escl::status_xml(adf_empty, &jobs);
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
    let id = uuid::Uuid::new_v4().to_string();
    inner.lock().unwrap().jobs.insert(
        id.clone(),
        Job {
            body: Vec::new(),
            mime: "image/jpeg".into(),
            consumed: false,
            ready: false,
            error: None,
        },
    );
    let bg = inner.clone();
    let job_id = id.clone();
    let _ = thread::Builder::new()
        .name("hp-m177-escl-job".into())
        .spawn(move || {
            let transport = crate::transport::UreqTransport::new(std::time::Duration::from_secs(90));
            let result = scan::scan(&transport, &device, &req);
            if let Ok(mut g) = bg.lock() {
                if let Some(job) = g.jobs.get_mut(&job_id) {
                    match result {
                        Ok(out) => {
                            job.mime = if out.format == OutputFormat::Pdf
                                || imagefmt::is_pdf(&out.bytes)
                            {
                                "application/pdf".into()
                            } else {
                                "image/jpeg".into()
                            };
                            job.body = out.bytes;
                            job.ready = true;
                        }
                        Err(e) => {
                            job.error = Some(e.to_string());
                            job.ready = true;
                        }
                    }
                }
            }
        });
    let location = format!("/eSCL/ScanJobs/{id}");
    (
        201,
        "text/xml; charset=utf-8".into(),
        format!("<scan:JobUri>{location}</scan:JobUri>").into_bytes(),
        vec![("Location".into(), location)],
    )
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
        Some(job) if !job.ready => (
            503,
            "text/plain".into(),
            b"processing".to_vec(),
            vec![],
        ),
        Some(job) if job.error.is_some() => (
            503,
            "text/plain".into(),
            job.error.clone().unwrap_or_default().into_bytes(),
            vec![],
        ),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::UreqTransport;

    #[test]
    fn scanner_status_adf_empty_and_loaded_without_device() {
        let facade = EsclFacade::start(None).unwrap();
        let t = UreqTransport::default();
        let url = format!("{}/eSCL/ScannerStatus", facade.url());
        let empty = t.get(&url).unwrap().text();
        assert!(empty.contains("ScannerAdfEmpty"), "{empty}");
        facade.set_adf_loaded(true);
        let loaded = t.get(&url).unwrap().text();
        assert!(loaded.contains("ScannerAdfLoaded"), "{loaded}");
        facade.set_adf_loaded(false);
        let empty2 = t.get(&url).unwrap().text();
        assert!(empty2.contains("ScannerAdfEmpty"), "{empty2}");
    }

    #[test]
    fn probe_paper_in_adf_reads_fake_paper_flag() {
        use crate::fake::{FakeDevice, FakeOptions};
        let fake = FakeDevice::start_with(FakeOptions {
            paper_in_adf: false,
            ..FakeOptions::default()
        })
        .unwrap();
        assert_eq!(
            probe_paper_in_adf(&fake.host(), fake.port()),
            Some(false)
        );
        fake.set_paper_in_adf(true);
        assert_eq!(
            probe_paper_in_adf(&fake.host(), fake.port()),
            Some(true)
        );
    }

    #[test]
    fn soap_probe_ports_try_job_then_default_soap() {
        let wsd = DeviceRecord {
            id: "w".into(),
            name: "M177fw".into(),
            host: "127.0.0.1".into(),
            job: JobProtocol::Wsd { port: 3911 },
            has_escl_caps: true,
            has_platen: true,
            has_adf: true,
            uuid: None,
        };
        assert_eq!(soap_probe_ports(&wsd), vec![3911, DEFAULT_SOAP_PORT]);
        let soap = DeviceRecord {
            job: JobProtocol::Soap { port: DEFAULT_SOAP_PORT },
            ..wsd.clone()
        };
        assert_eq!(soap_probe_ports(&soap), vec![DEFAULT_SOAP_PORT]);
    }

    #[test]
    fn scanner_capabilities_use_saved_device_name() {
        let rec = DeviceRecord {
            id: "o".into(),
            name: "Other LAN MFP".into(),
            host: "127.0.0.1".into(),
            job: JobProtocol::Soap { port: 9 },
            has_escl_caps: true,
            has_platen: true,
            has_adf: true,
            uuid: None,
        };
        let facade = EsclFacade::start(Some(rec)).unwrap();
        let t = UreqTransport::default();
        let xml = t
            .get(&format!("{}/eSCL/ScannerCapabilities", facade.url()))
            .unwrap()
            .text();
        assert!(
            xml.contains("Other LAN MFP"),
            "facade must advertise the probed make/model: {xml}"
        );
    }
}
