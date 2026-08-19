//! Protocol-accurate fake M177fw: SOAP on one port (GetScannerElements,
//! CreateScanJob, GetJobInfo, RetrieveImage + DIME) and optional eSCL
//! capabilities that 404 ScanJobs, matching the live firmware.

use crate::dime;
use crate::error::Result;
use crate::imagefmt;
use crate::model::{ColorMode, ScanRequest, DEFAULT_SOAP_PORT};
use crate::soap;
use crate::xmlutil::first_text;
use std::io::Cursor;
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::thread;
use tiny_http::{Header, Method, Response, Server, StatusCode};

#[derive(Debug, Clone, Default)]
pub struct FakeState {
    pub create_job_bodies: Vec<String>,
    pub last_create_job_xml: String,
    pub last_wsd_create_xml: String,
    pub last_retrieve_xml: String,
    pub requests: Vec<String>,
    pub job_counter: u32,
    pub adf_pages_remaining: u32,
    pub paper_in_adf: bool,
}

#[derive(Clone)]
pub struct FakeDevice {
    pub addr: SocketAddr,
    pub state: Arc<Mutex<FakeState>>,
}

impl FakeDevice {
    pub fn start() -> Result<Self> {
        Self::start_with(FakeOptions::default())
    }

    pub fn start_with(opts: FakeOptions) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let state = Arc::new(Mutex::new(FakeState {
            paper_in_adf: opts.paper_in_adf,
            adf_pages_remaining: if opts.paper_in_adf { opts.adf_pages } else { 0 },
            ..FakeState::default()
        }));
        let server = Server::from_listener(listener, None).map_err(|e| {
            crate::error::Error::msg(format!("fake device listen: {e}"))
        })?;
        let st = state.clone();
        thread::Builder::new()
            .name("hp-m177-fake".into())
            .spawn(move || run_server(server, st, opts))
            .map_err(|e| crate::error::Error::msg(format!("fake device thread: {e}")))?;
        Ok(Self { addr, state })
    }

    pub fn host(&self) -> String {
        self.addr.ip().to_string()
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    pub fn last_create_job_xml(&self) -> String {
        self.state.lock().unwrap().last_create_job_xml.clone()
    }

    pub fn last_ticket(&self) -> Option<ScanRequest> {
        soap::parse_job_ticket(&self.last_create_job_xml())
    }

    pub fn last_wsd_create_xml(&self) -> String {
        self.state.lock().unwrap().last_wsd_create_xml.clone()
    }

    pub fn request_log(&self) -> Vec<String> {
        self.state.lock().unwrap().requests.clone()
    }
}

#[derive(Debug, Clone)]
pub struct FakeOptions {
    pub paper_in_adf: bool,
    pub adf_pages: u32,
    pub escl_caps: bool,
    pub escl_jobs: bool,
    /// SOAP CreateScanJob returns Error 4; WSD /scanner still works.
    pub soap_create_fault: bool,
    /// GetJobInfo returns a SOAP fault (must not spin).
    pub get_job_info_fault: bool,
    /// SOAP RetrieveImage has no image bytes.
    pub retrieve_empty: bool,
    /// SOAP RetrieveImage returns HTTP 500 (must fall through to WSD).
    pub retrieve_http_error: bool,
    /// GetScannerElements does not answer (probe falls through to WSD).
    pub soap_dead: bool,
}

impl Default for FakeOptions {
    fn default() -> Self {
        Self {
            paper_in_adf: true,
            adf_pages: 1,
            escl_caps: true,
            escl_jobs: false,
            soap_create_fault: false,
            get_job_info_fault: false,
            retrieve_empty: false,
            retrieve_http_error: false,
            soap_dead: false,
        }
    }
}

fn run_server(server: Server, state: Arc<Mutex<FakeState>>, opts: FakeOptions) {
    loop {
        let mut req = match server.recv() {
            Ok(r) => r,
            Err(_) => break,
        };
        let mut body = Vec::new();
        let _ = std::io::Read::read_to_end(req.as_reader(), &mut body);
        let text = String::from_utf8_lossy(&body).into_owned();
        let url = req.url().to_string();
        let method = req.method().clone();
        {
            let mut st = state.lock().unwrap();
            st.requests
                .push(format!("{method:?} {url} {}", preview(&text)));
        }
        let (status, ctype, payload) = handle(&method, &url, &text, &body, &state, &opts);
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
        let _ = req.respond(response);
    }
}

fn handle(
    method: &Method,
    url: &str,
    body: &str,
    _raw: &[u8],
    state: &Arc<Mutex<FakeState>>,
    opts: &FakeOptions,
) -> (u16, String, Vec<u8>) {
    if url.contains("/scanner") || body.contains("wdp/scan") {
        return handle_wsd(url, body, state);
    }
    if url.starts_with("/eSCL/ScannerCapabilities") && *method == Method::Get {
        if opts.escl_caps {
            return (
                200,
                "text/xml; charset=utf-8".into(),
                include_str!("../fixtures/live/escl-ScannerCapabilities.xml")
                    .as_bytes()
                    .to_vec(),
            );
        }
        return (404, "text/plain".into(), b"no".to_vec());
    }
    if url.starts_with("/eSCL/ScannerStatus") && *method == Method::Get {
        return (
            200,
            "text/xml; charset=utf-8".into(),
            include_str!("../fixtures/live/escl-ScannerStatus.xml")
                .as_bytes()
                .to_vec(),
        );
    }
    if url.starts_with("/eSCL/ScanJobs") {
        if opts.escl_jobs && *method == Method::Post {
            return (
                201,
                "text/xml".into(),
                b"<ok/>".to_vec(),
            );
        }
        return (404, "text/plain".into(), Vec::new());
    }
    if url.starts_with("/debug/last-job") {
        let xml = state.lock().unwrap().last_create_job_xml.clone();
        return (200, "text/xml; charset=utf-8".into(), xml.into_bytes());
    }

    // SOAP — the live device accepts every method as POST / 
    if body.contains("GetScannerElements") {
        if opts.soap_dead {
            return (500, "text/plain".into(), b"soap wedged".to_vec());
        }
        let mut xml = include_str!("../fixtures/live/soap-GetScannerElements.xml").to_string();
        let paper = if state.lock().unwrap().paper_in_adf {
            "true"
        } else {
            "false"
        };
        xml = xml.replace(
            "<PaperInADF>false</PaperInADF>",
            &format!("<PaperInADF>{paper}</PaperInADF>"),
        );
        return (202, "application/soap+xml; charset=utf-8".into(), xml.into_bytes());
    }
    if body.contains("CreateScanJob") {
        if opts.soap_create_fault {
            let fault = r#"<?xml version="1.0"?><SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope"><SOAP-ENV:Body><SOAP-ENV:Fault><SOAP-ENV:Code><SOAP-ENV:Value>SOAP-ENV:Sender</SOAP-ENV:Value></SOAP-ENV:Code><SOAP-ENV:Reason><SOAP-ENV:Text>Error 4</SOAP-ENV:Text></SOAP-ENV:Reason></SOAP-ENV:Fault></SOAP-ENV:Body></SOAP-ENV:Envelope>"#;
            return (
                500,
                "application/soap+xml; charset=utf-8".into(),
                fault.as_bytes().to_vec(),
            );
        }
        let ticket = soap::parse_job_ticket(body);
        let mut st = state.lock().unwrap();
        st.last_create_job_xml = body.to_string();
        st.create_job_bodies.push(body.to_string());
        st.job_counter += 1;
        let id = st.job_counter;
        if let Some(t) = &ticket {
            if t.source == crate::model::ScanSource::Adf {
                st.adf_pages_remaining = if st.paper_in_adf { opts.adf_pages.max(1) } else { 0 };
            } else {
                st.adf_pages_remaining = 1;
            }
        }
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope" xmlns:wscn="http://tempuri.org/wscn.xsd">
<SOAP-ENV:Body><wscn:CreateScanJobResponseType>
<JobId>{id}</JobId><JobToken>tok-{id}</JobToken>
<DocumentFinalParameters>
<Format>jfif</Format>
<InputSource>{src}</InputSource>
<MediaSides><MediaFront>
<ColorProcessing>{color}</ColorProcessing>
<Resolution><Width>{dpi}</Width><Height>{dpi}</Height></Resolution>
</MediaFront></MediaSides>
</DocumentFinalParameters>
</wscn:CreateScanJobResponseType></SOAP-ENV:Body></SOAP-ENV:Envelope>"#,
            src = ticket
                .as_ref()
                .map(|t| t.source.soap_name())
                .unwrap_or("Platen"),
            color = ticket
                .as_ref()
                .map(|t| t.color.soap_name())
                .unwrap_or("RGB24"),
            dpi = ticket.as_ref().map(|t| t.dpi).unwrap_or(300),
        );
        return (200, "application/soap+xml; charset=utf-8".into(), xml.into_bytes());
    }
    if body.contains("GetJobInfo") {
        if opts.get_job_info_fault {
            let fault = r#"<?xml version="1.0"?><SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope"><SOAP-ENV:Body><SOAP-ENV:Fault><SOAP-ENV:Code><SOAP-ENV:Value>SOAP-ENV:Sender</SOAP-ENV:Value></SOAP-ENV:Code><SOAP-ENV:Reason><SOAP-ENV:Text>GetJobInfo fault</SOAP-ENV:Text></SOAP-ENV:Reason></SOAP-ENV:Fault></SOAP-ENV:Body></SOAP-ENV:Envelope>"#;
            return (
                200,
                "application/soap+xml; charset=utf-8".into(),
                fault.as_bytes().to_vec(),
            );
        }
        let id = first_text(body, "JobId")
            .or_else(|| first_text(body, "jobId"))
            .unwrap_or_else(|| "1".into());
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope">
<SOAP-ENV:Body><JobSummaryType>
<JobId>{id}</JobId><JobState>Processing</JobState><ScansCompleted>1</ScansCompleted>
</JobSummaryType></SOAP-ENV:Body></SOAP-ENV:Envelope>"#
        );
        return (200, "application/soap+xml; charset=utf-8".into(), xml.into_bytes());
    }
    if body.contains("RetrieveImage") {
        if opts.retrieve_http_error {
            return (500, "text/plain".into(), b"retrieve failed".to_vec());
        }
        let mut st = state.lock().unwrap();
        st.last_retrieve_xml = body.to_string();
        if st.adf_pages_remaining == 0 {
            let fault = r#"<?xml version="1.0" encoding="UTF-8"?>
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope">
<SOAP-ENV:Body><SOAP-ENV:Fault><SOAP-ENV:Code><SOAP-ENV:Value>SOAP-ENV:Sender</SOAP-ENV:Value>
<SOAP-ENV:Subcode><SOAP-ENV:Value>wscn:ClientErrorNoImagesAvailable</SOAP-ENV:Value></SOAP-ENV:Subcode>
</SOAP-ENV:Code><SOAP-ENV:Reason><SOAP-ENV:Text>no images</SOAP-ENV:Text></SOAP-ENV:Reason>
</SOAP-ENV:Fault></SOAP-ENV:Body></SOAP-ENV:Envelope>"#;
            return (
                400,
                "application/soap+xml; charset=utf-8".into(),
                fault.as_bytes().to_vec(),
            );
        }
        st.adf_pages_remaining = st.adf_pages_remaining.saturating_sub(1);
        if opts.retrieve_empty {
            return (
                200,
                "application/dime".into(),
                dime::wrap_soap_and_jpeg(soap::retrieve_image_soap_stub(), b""),
            );
        }
        let ticket = soap::parse_job_ticket(&st.last_create_job_xml).unwrap_or_default();
        let mut rgb = vec![0u8; 8 * 8 * 3];
        for i in 0..64 {
            let (r, g, b) = match ticket.color {
                ColorMode::Color => (200, 40, 40),
                ColorMode::Gray => (160, 160, 160),
                ColorMode::Lineart => {
                    if i % 2 == 0 {
                        (0, 0, 0)
                    } else {
                        (255, 255, 255)
                    }
                }
            };
            rgb[i * 3] = r;
            rgb[i * 3 + 1] = g;
            rgb[i * 3 + 2] = b;
        }
        let jpeg = imagefmt::rgb_to_jpeg(&rgb, 8, 8, 80).unwrap_or_else(|_| {
            imagefmt::synthetic_jpeg(
                &format!("source={} color={} dpi={}", ticket.source, ticket.color, ticket.dpi),
                ticket.color,
            )
        });
        let dime_body = dime::wrap_soap_and_jpeg(soap::retrieve_image_soap_stub(), &jpeg);
        return (200, "application/dime".into(), dime_body);
    }
    if body.contains("CancelJob") {
        return (
            200,
            "application/soap+xml; charset=utf-8".into(),
            b"<ok/>".to_vec(),
        );
    }
    (
        404,
        "text/plain".into(),
        format!("unknown fake request {url}").into_bytes(),
    )
}

fn handle_wsd(
    url: &str,
    body: &str,
    state: &Arc<Mutex<FakeState>>,
) -> (u16, String, Vec<u8>) {
    if body.contains("CreateScanJob") {
        let ticket = soap::parse_job_ticket(body);
        let mut st = state.lock().unwrap();
        st.last_wsd_create_xml = body.to_string();
        st.last_create_job_xml = body.to_string();
        st.create_job_bodies.push(body.to_string());
        st.job_counter += 1;
        let id = st.job_counter;
        if let Some(t) = &ticket {
            st.adf_pages_remaining = if t.source == crate::model::ScanSource::Adf {
                if st.paper_in_adf {
                    1
                } else {
                    0
                }
            } else {
                1
            };
        } else {
            st.adf_pages_remaining = 1;
        }
        let xml = format!(
            r#"<?xml version="1.0"?><SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope" xmlns:wscn="http://schemas.microsoft.com/windows/2006/08/wdp/scan"><SOAP-ENV:Body><wscn:CreateScanJobResponse><wscn:JobId>{id}</wscn:JobId><wscn:JobToken>tok-{id}</wscn:JobToken></wscn:CreateScanJobResponse></SOAP-ENV:Body></SOAP-ENV:Envelope>"#
        );
        return (
            200,
            "application/soap+xml; charset=utf-8".into(),
            xml.into_bytes(),
        );
    }
    if body.contains("RetrieveImage") {
        let bmp = crate::imagefmt::solid_bmp_bgra(2, 2, 0, 0, 255);
        let mtom = crate::wsd::wrap_bmp_mtom(&bmp);
        return (
            200,
            r#"multipart/related; type="application/xop+xml""#.into(),
            mtom,
        );
    }
    // Transfer Get / anything else: prove the service is alive.
    let xml = format!(
        r#"<?xml version="1.0"?><s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope" xmlns:wsa="http://schemas.xmlsoap.org/ws/2004/08/addressing"><s:Header><wsa:Action>http://schemas.xmlsoap.org/ws/2004/08/addressing/role/anonymous</wsa:Action></s:Header><s:Body><Envelope/></s:Body></s:Envelope>"#
    );
    let _ = url;
    (
        200,
        "application/soap+xml; charset=utf-8".into(),
        xml.into_bytes(),
    )
}

fn preview(s: &str) -> String {
    let t = s.replace('\n', " ");
    if t.len() > 80 {
        format!("{}…", &t[..80])
    } else {
        t
    }
}

/// Keep the SOAP default port documented for callers that speak to a real MFP.
#[allow(dead_code)]
pub const LIVE_SOAP_PORT: u16 = DEFAULT_SOAP_PORT;

// ColorMode is used by synthetic_jpeg.
#[allow(dead_code)]
fn _color_used() -> ColorMode {
    ColorMode::Color
}
