use crate::error::{Error, Result};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::net::IpAddr;
use std::process::{Child, Command, Stdio};

/// Bonjour / mDNS advertisement for a local eSCL scanner (`_uscan._tcp`).
///
/// On macOS we prefer Apple's `dns-sd -R` so Image Capture sees the same
/// responder as every other AirScan device. `mdns-sd` is the fallback.
pub struct Advertisement {
    mdns: Option<ServiceDaemon>,
    child: Option<Child>,
}

impl Advertisement {
    pub fn start(port: u16, instance: &str) -> Result<Self> {
        Self::start_with_ty(port, instance, crate::model::PRODUCT_NAME)
    }

    /// `instance` is the Bonjour name; `ty` is the AirScan TXT make/model.
    pub fn start_with_ty(port: u16, instance: &str, ty: &str) -> Result<Self> {
        if cfg!(target_os = "macos") {
            match start_dns_sd(port, instance, ty) {
                Ok(adv) => return Ok(adv),
                Err(e) => eprintln!("dns-sd advertise failed ({e}); falling back to mdns-sd"),
            }
        }
        start_mdns_sd(port, instance, ty)
    }

    pub fn stop(mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(mdns) = self.mdns.take() {
            let _ = mdns.shutdown();
        }
    }
}

impl Drop for Advertisement {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn start_dns_sd(port: u16, instance: &str, ty: &str) -> Result<Advertisement> {
    let child = Command::new("dns-sd")
        .args([
            "-R",
            instance,
            "_uscan._tcp",
            "local.",
            &port.to_string(),
            "txtvers=1",
            "vers=2.63",
            "rs=eSCL",
            &format!("ty={ty}"),
            "pdl=image/jpeg,application/pdf",
            "cs=color,grayscale",
            "is=platen,adf",
            "duplex=F",
            "note=hp-m177 local AirScan bridge",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::msg(format!("dns-sd -R: {e}")))?;
    std::thread::sleep(std::time::Duration::from_millis(200));
    Ok(Advertisement {
        mdns: None,
        child: Some(child),
    })
}

fn start_mdns_sd(port: u16, instance: &str, ty: &str) -> Result<Advertisement> {
    let mdns = ServiceDaemon::new()
        .map_err(|e| Error::msg(format!("cannot start mDNS responder: {e}")))?;
    let host = local_hostname();
    let ips = local_ips();
    if ips.is_empty() {
        return Err(Error::msg("no IP addresses available to advertise eSCL"));
    }
    let props = [
        ("txtvers", "1"),
        ("vers", "2.63"),
        ("rs", "eSCL"),
        ("ty", ty),
        ("pdl", "image/jpeg,application/pdf"),
        ("cs", "color,grayscale"),
        ("is", "platen,adf"),
        ("duplex", "F"),
        ("note", "hp-m177 local AirScan bridge"),
    ];
    let info = ServiceInfo::new(
        "_uscan._tcp.local.",
        instance,
        &host,
        &ips[..],
        port,
        &props[..],
    )
    .map_err(|e| Error::msg(format!("invalid mDNS service info: {e}")))?;
    mdns.register(info)
        .map_err(|e| Error::msg(format!("mDNS register failed: {e}")))?;
    Ok(Advertisement {
        mdns: Some(mdns),
        child: None,
    })
}

fn local_hostname() -> String {
    // Prefer the real machine name so Bonjour has a resolvable host record.
    let raw = hostname_cmd().unwrap_or_else(|| "hp-m177".into());
    let raw = raw.trim().trim_end_matches('.').to_string();
    if raw.ends_with(".local") {
        format!("{raw}.")
    } else {
        format!("{raw}.local.")
    }
}

fn hostname_cmd() -> Option<String> {
    if let Ok(out) = std::process::Command::new("hostname").arg("-s").output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
}

fn local_ips() -> Vec<IpAddr> {
    let mut ips = vec![IpAddr::from([127, 0, 0, 1])];
    if let Ok(s) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if s.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = s.local_addr() {
                if !addr.ip().is_loopback() {
                    ips.push(addr.ip());
                }
            }
        }
    }
    ips
}

/// TXT records the advertisement is supposed to publish. Tests assert this
/// table rather than re-implementing Bonjour.
pub fn uscan_txt_records() -> Vec<(&'static str, &'static str)> {
    vec![
        ("txtvers", "1"),
        ("vers", "2.63"),
        ("rs", "eSCL"),
        ("ty", crate::model::PRODUCT_NAME),
        ("pdl", "image/jpeg,application/pdf"),
        ("cs", "color,grayscale"),
        ("is", "platen,adf"),
        ("duplex", "F"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn txt_records_match_airscan_expectations() {
        let map: std::collections::HashMap<_, _> = uscan_txt_records().into_iter().collect();
        assert_eq!(map["rs"], "eSCL");
        assert_eq!(map["is"], "platen,adf");
        assert!(map["cs"].contains("color"));
        assert!(map["cs"].contains("grayscale"));
        assert!(map["pdl"].contains("image/jpeg"));
        assert!(map["pdl"].contains("application/pdf"));
    }
}
