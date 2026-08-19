use crate::error::{Error, Result};
use crate::model::{DiscoveredDevice, PRODUCT_SHORT};
use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const SERVICES: &[&str] = &[
    "_ipp._tcp.local.",
    "_scanner._tcp.local.",
    "_uscan._tcp.local.",
    "_uscans._tcp.local.",
];

/// Browse common printer/scanner Bonjour types and return unique hosts.
///
/// The wait is wall-clock `timeout` across every service type at once.
/// An empty poll must not abort the browse — a typical mDNS reply lands
/// well after the first 250 ms.
pub fn browse_lan(timeout: Duration) -> Result<Vec<DiscoveredDevice>> {
    #[cfg(target_os = "macos")]
    {
        match browse_with_dns_sd(SERVICES, timeout) {
            Ok(found) => return Ok(found),
            Err(e) => eprintln!("dns-sd browse failed ({e}); falling back to mdns-sd"),
        }
    }
    browse_with_mdns_sd(SERVICES, timeout)
}

fn dns_sd_type(svc: &str) -> &str {
    svc.trim_end_matches(".local.").trim_end_matches('.')
}

fn browse_with_dns_sd(services: &[&str], timeout: Duration) -> Result<Vec<DiscoveredDevice>> {
    // Poll until the deadline. A single long-lived `dns-sd -B` pipe is
    // block-buffered; SIGKILL then drops Add events that arrived after start.
    // Short SIGTERM polls see the responder cache, including services that
    // appeared after the first 250 ms.
    let deadline = Instant::now() + timeout;
    let mut names: Vec<(String, String)> = Vec::new();
    while Instant::now() < deadline {
        let slice = deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(300));
        if slice.is_zero() {
            break;
        }
        let mut kids = Vec::new();
        for svc in services {
            let short = dns_sd_type(svc).to_string();
            let child = Command::new("dns-sd")
                .args(["-B", &short, "local."])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| Error::msg(format!("dns-sd -B {short}: {e}")))?;
            kids.push(((*svc).to_string(), short, child));
        }
        thread::sleep(slice);
        for (svc, short, mut child) in kids {
            let mut pipe = child.stdout.take();
            sigterm(child.id());
            let _ = child.wait();
            let mut buf = String::new();
            if let Some(mut out) = pipe.take() {
                let _ = out.read_to_string(&mut buf);
            }
            for instance in parse_browse_instances(&buf, &short) {
                if !names.iter().any(|(s, n)| s == &svc && n == &instance) {
                    names.push((svc.clone(), instance));
                }
            }
        }
    }
    let mut found = Vec::new();
    for (svc, instance) in names {
        let short = dns_sd_type(&svc).to_string();
        match resolve_dns_sd(&instance, &short) {
            Ok(Some((host, port))) => {
                let dev = DiscoveredDevice {
                    name: unescape_dnssd(&instance),
                    host,
                    service: svc,
                    port,
                };
                if !found.iter().any(|d: &DiscoveredDevice| {
                    d.name == dev.name && d.service == dev.service && d.host == dev.host
                }) {
                    found.push(dev);
                }
            }
            Ok(None) => {
                found.push(DiscoveredDevice {
                    name: unescape_dnssd(&instance),
                    host: String::new(),
                    service: svc,
                    port: 0,
                });
            }
            Err(_) => {
                found.push(DiscoveredDevice {
                    name: unescape_dnssd(&instance),
                    host: String::new(),
                    service: svc,
                    port: 0,
                });
            }
        }
    }
    Ok(found)
}

fn resolve_dns_sd(instance: &str, service: &str) -> Result<Option<(String, u16)>> {
    let mut child = Command::new("dns-sd")
        .args(["-L", instance, service, "local."])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::msg(format!("dns-sd -L: {e}")))?;
    thread::sleep(Duration::from_millis(1200));
    let mut pipe = child.stdout.take();
    sigterm(child.id());
    let _ = child.wait();
    let mut buf = String::new();
    if let Some(mut out) = pipe.take() {
        let _ = out.read_to_string(&mut buf);
    }
    Ok(parse_resolve_host_port(&buf))
}

/// Parse `dns-sd -B` lines for instance names of `service` (e.g. `_uscan._tcp`).
pub fn parse_browse_instances(output: &str, service: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in output.lines() {
        if !line.contains("Add") {
            continue;
        }
        let Some(idx) = line.find(service) else {
            continue;
        };
        let rest = line[idx + service.len()..].trim();
        let rest = rest.trim_start_matches('.').trim();
        if rest.is_empty() || rest.eq_ignore_ascii_case("local") {
            continue;
        }
        if !out.iter().any(|n| n == rest) {
            out.push(rest.to_string());
        }
    }
    out
}

/// Parse `dns-sd -L` output (`can be reached at host.:port`).
pub fn parse_resolve_host_port(output: &str) -> Option<(String, u16)> {
    let marker = "can be reached at ";
    let pos = output.find(marker)?;
    let token = output[pos + marker.len()..].split_whitespace().next()?;
    let token = token.trim_end_matches('.');
    let (host, port) = token.rsplit_once(':')?;
    let host = host.trim_end_matches('.').to_string();
    let port = port.parse().ok()?;
    if host.is_empty() {
        return None;
    }
    Some((host, port))
}

fn unescape_dnssd(name: &str) -> String {
    name.replace("\\032", " ").replace("\\ ", " ")
}

fn sigterm(pid: u32) {
    let _ = Command::new("kill").arg(pid.to_string()).status();
}

fn browse_with_mdns_sd(services: &[&str], timeout: Duration) -> Result<Vec<DiscoveredDevice>> {
    let mdns = ServiceDaemon::new().map_err(|e| {
        Error::msg(format!("cannot start mDNS (Bonjour) browser: {e}"))
    })?;
    let mut rxs = Vec::new();
    for svc in services {
        let rx = mdns
            .browse(svc)
            .map_err(|e| Error::msg(format!("mDNS browse {svc}: {e}")))?;
        rxs.push((*svc, rx));
    }
    let deadline = Instant::now() + timeout;
    let mut found: Vec<DiscoveredDevice> = Vec::new();
    while Instant::now() < deadline {
        let mut progressed = false;
        for (svc, rx) in &rxs {
            while let Ok(ev) = rx.try_recv() {
                progressed = true;
                if let ServiceEvent::ServiceResolved(info) = ev {
                    let host = info.get_hostname().trim_end_matches('.').to_string();
                    let full = info.get_fullname();
                    let name = full
                        .split("._")
                        .next()
                        .unwrap_or(full)
                        .replace("\\032", " ");
                    let dev = DiscoveredDevice {
                        name,
                        host,
                        service: (*svc).to_string(),
                        port: info.get_port(),
                    };
                    if !found
                        .iter()
                        .any(|d| d.host == dev.host && d.service == dev.service)
                    {
                        found.push(dev);
                    }
                }
            }
        }
        if !progressed {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            thread::sleep(remaining.min(Duration::from_millis(50)));
        }
    }
    let _ = mdns.shutdown();
    Ok(found)
}

pub fn likely_m177(d: &DiscoveredDevice) -> bool {
    let blob = format!("{} {}", d.name, d.host).to_ascii_lowercase();
    blob.contains("m177") || blob.contains(PRODUCT_SHORT.to_ascii_lowercase().as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_bonjour_names_from_this_mfp() {
        let hit = DiscoveredDevice {
            name: "HP Color LaserJet Pro MFP M177fw[26BA77]".into(),
            host: "DEV26BA77.local".into(),
            service: "_ipp._tcp.local.".into(),
            port: 631,
        };
        let miss = DiscoveredDevice {
            name: "Office Laser".into(),
            host: "printer.local".into(),
            service: "_ipp._tcp.local.".into(),
            port: 631,
        };
        assert!(likely_m177(&hit));
        assert!(!likely_m177(&miss));
    }

    #[test]
    fn parses_recorded_dns_sd_browse_and_resolve() {
        let browse = "\
22:00:18.863  Add        3  16 local.               _uscan._tcp.         HP M177fw (hp-m177)\n\
22:00:18.863  Add        2  18 local.               _uscan._tcp.         HP M177fw (hp-m177)\n";
        let names = parse_browse_instances(browse, "_uscan._tcp");
        assert_eq!(names, ["HP M177fw (hp-m177)"]);
        let resolve = "\
HP\\032M177fw\\032(hp-m177)._uscan._tcp.local. can be reached at mac-studio.local.:18090 (interface 16)\n\
 txtvers=1 vers=2.63 rs=eSCL ty=HP\\ Color\\ LaserJet\\ Pro\\ MFP\\ M177fw pdl=image/jpeg,application/pdf cs=color,grayscale is=platen,adf duplex=F\n";
        let (host, port) = parse_resolve_host_port(resolve).expect("resolve");
        assert_eq!(host, "mac-studio.local");
        assert_eq!(port, 18090);
    }
}
