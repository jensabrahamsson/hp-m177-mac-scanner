use crate::error::Result;
use crate::escl;
use crate::model::{
    JobProtocol, ProbeResult, DEFAULT_ESCL_PORT, DEFAULT_SOAP_PORT, PRODUCT_NAME,
};
use crate::soap;
use crate::transport::Transport;

/// Probe eSCL capabilities/jobs and the HP SOAP scanner API.
///
/// On the live M177fw: GET /eSCL/ScannerCapabilities is 200, POST
/// /eSCL/ScanJobs is 404, and POST GetScannerElements to :8289 succeeds.
pub fn probe_host(transport: &dyn Transport, host: &str) -> Result<ProbeResult> {
    probe_host_ports(transport, host, DEFAULT_SOAP_PORT, DEFAULT_ESCL_PORT)
}

pub fn probe_host_ports(
    transport: &dyn Transport,
    host: &str,
    soap_port: u16,
    escl_port: u16,
) -> Result<ProbeResult> {
    let host = strip_scheme(host);
    let mut name = PRODUCT_NAME.to_string();
    let mut escl_caps = false;
    let mut escl_jobs = false;

    for port in [escl_port, 80, 8080] {
        let url = format!("http://{host}:{port}/eSCL/ScannerCapabilities");
        if let Ok(resp) = transport.get(&url) {
            if resp.is_success() {
                let text = resp.text();
                if escl::looks_like_capabilities(&text) {
                    escl_caps = true;
                    if let Some(model) = crate::xmlutil::first_text(&text, "MakeAndModel") {
                        name = model;
                    }
                    escl_jobs = escl_jobs_available(transport, &host, port);
                    break;
                }
            }
        }
        if port == escl_port {
            continue;
        }
    }

    let soap = match soap_caps(transport, &host, soap_port) {
        Ok(c) => Some(c),
        Err(_) => None,
    };

    let preferred = if escl_jobs {
        Some(JobProtocol::Escl { port: escl_port })
    } else if soap.is_some() {
        Some(JobProtocol::Soap { port: soap_port })
    } else {
        None
    };

    Ok(ProbeResult {
        host,
        name,
        soap,
        escl_caps,
        escl_jobs,
        preferred,
    })
}

fn soap_caps(
    transport: &dyn Transport,
    host: &str,
    port: u16,
) -> Result<crate::model::SoapCapabilities> {
    let url = format!("http://{host}:{port}/");
    let xml = soap::get_scanner_elements_xml();
    let resp = transport.post(&url, xml.as_bytes(), soap::SOAP_CONTENT_TYPE)?;
    soap::parse_capabilities(&resp.text())
}

/// We must not POST a real ScanJobs during probe (that would start a scan).
/// Treat jobs as available only when GET /eSCL/ScanJobs is not 404, or the
/// firmware serves a NextDocument-style resource listing. The live M177fw
/// returns 404 here.
fn escl_jobs_available(transport: &dyn Transport, host: &str, port: u16) -> bool {
    let url = format!("http://{host}:{port}/eSCL/ScanJobs");
    match transport.get(&url) {
        Ok(r) => r.status != 404,
        Err(crate::error::Error::Http { status, .. }) => status != 404,
        Err(_) => false,
    }
}

fn strip_scheme(host: &str) -> String {
    let h = host
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    h.split('/').next().unwrap_or(h).trim_end_matches('/').to_string()
}

/// Split `host`, `host:port`, or `http://host:port/` into (host, optional port).
pub fn split_host_port(input: &str, default_port: u16) -> (String, u16) {
    let cleaned = strip_scheme(input);
    if let Some((h, p)) = cleaned.rsplit_once(':') {
        if let Ok(port) = p.parse::<u16>() {
            return (h.to_string(), port);
        }
    }
    (cleaned, default_port)
}
