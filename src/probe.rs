use crate::error::{Error, Result};
use crate::escl;
use crate::model::{
    JobProtocol, ProbeResult, DEFAULT_ESCL_PORT, DEFAULT_SOAP_PORT, DEFAULT_WSD_PORT, PRODUCT_NAME,
};
use crate::soap;
use crate::transport::{HttpRequest, Transport};
use crate::wsd;
use std::time::Duration;

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
    let mut name = String::new();
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
                        let t = model.trim();
                        if !t.is_empty() {
                            name = t.to_string();
                        }
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
    if name.is_empty() {
        if let Some(model) = soap.as_ref().and_then(|c| c.make_and_model.clone()) {
            name = model;
        }
    }
    if name.is_empty() {
        name = PRODUCT_NAME.to_string();
    }

    let wsd_port = if wsd_alive(transport, &host, soap_port) {
        soap_port
    } else {
        DEFAULT_WSD_PORT
    };
    let wsd_ok = wsd_alive(transport, &host, wsd_port);

    let preferred = if escl_jobs {
        Some(JobProtocol::Escl { port: escl_port })
    } else if soap.is_some() {
        Some(JobProtocol::Soap { port: soap_port })
    } else if wsd_ok {
        Some(JobProtocol::Wsd { port: wsd_port })
    } else if escl_caps {
        Some(JobProtocol::Wsd { port: wsd_port })
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
    let req = HttpRequest::post(&url, xml.into_bytes(), soap::SOAP_CONTENT_TYPE)
        .with_timeout(Duration::from_secs(4));
    let resp = match transport.execute(req) {
        Ok(r) => r,
        Err(Error::Http { detail, .. }) => {
            return soap::parse_capabilities(&detail);
        }
        Err(e) => return Err(e),
    };
    soap::parse_capabilities(&resp.text())
}

fn wsd_alive(transport: &dyn Transport, host: &str, port: u16) -> bool {
    let url = wsd::scanner_url(host, port);
    let xml = wsd::transfer_get_xml(&url);
    let req = wsd::request(&url, wsd::ACTION_TRANSFER_GET, xml, Duration::from_secs(4));
    match transport.execute(req) {
        Ok(r) => wsd::looks_alive(&r.body),
        Err(Error::Http { detail, .. }) => wsd::looks_alive(detail.as_bytes()),
        Err(_) => false,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeDevice;
    use crate::transport::UreqTransport;

    #[test]
    fn probe_fake_m177_uses_escl_make_and_model_and_soap_jobs() {
        let fake = FakeDevice::start().unwrap();
        let t = UreqTransport::default();
        let p = probe_host_ports(&t, &fake.host(), fake.port(), fake.port()).unwrap();
        assert!(
            p.name.contains("M177fw"),
            "validated M177fw caps must keep the real product name, got {}",
            p.name
        );
        assert!(p.escl_caps);
        assert!(!p.escl_jobs, "live-like fake ScanJobs are 404");
        match p.preferred {
            Some(JobProtocol::Soap { port }) => assert_eq!(port, fake.port()),
            other => panic!("M177fw-like fake must prefer SOAP, got {other:?}"),
        }
    }

    #[test]
    fn soap_only_fake_does_not_take_juntest_model_number() {
        use crate::fake::FakeOptions;
        let fake = FakeDevice::start_with(FakeOptions {
            escl_caps: false,
            ..FakeOptions::default()
        })
        .unwrap();
        let t = UreqTransport::default();
        let p = probe_host_ports(&t, &fake.host(), fake.port(), fake.port()).unwrap();
        assert!(
            !p.name.to_ascii_lowercase().contains("juntest"),
            "SOAP ModelNumber junk must not win: {}",
            p.name
        );
        assert_eq!(p.name, crate::model::PRODUCT_NAME);
        assert!(!p.escl_caps);
        match p.preferred {
            Some(JobProtocol::Soap { port }) => assert_eq!(port, fake.port()),
            other => panic!("SOAP-only fake must prefer SOAP, got {other:?}"),
        }
    }
}
