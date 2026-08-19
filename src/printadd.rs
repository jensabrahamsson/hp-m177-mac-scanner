use crate::error::{Error, Result};
use crate::model::{PrintAddOutcome, PRODUCT_NAME};
use std::process::Command;

const QUEUE: &str = "HP_Color_LaserJet_Pro_MFP_M177fw";

/// Add an AirPrint/IPP Everywhere queue only if none already exists for this
/// MFP. Never deletes or replaces the user's working print queue.
pub fn add_printer_if_missing(host: &str) -> Result<PrintAddOutcome> {
    let existing = current_queues()?;
    if let Some(q) = existing.iter().find(|q| looks_like_m177(q)) {
        return Ok(PrintAddOutcome::LeftExisting {
            queue: q.name.clone(),
        });
    }
    if Command::new("lpadmin").output().is_err() {
        return Ok(PrintAddOutcome::Skipped {
            reason: "lpadmin is not available on this system".into(),
        });
    }
    let uri = format!("ipp://{host}/ipp/print");
    let out = Command::new("lpadmin")
        .args([
            "-p",
            QUEUE,
            "-E",
            "-v",
            &uri,
            "-m",
            "everywhere",
            "-D",
            PRODUCT_NAME,
        ])
        .output()
        .map_err(|e| Error::msg(format!("lpadmin failed to start: {e}")))?;
    if !out.status.success() {
        return Err(Error::msg(format!(
            "lpadmin failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(PrintAddOutcome::Added {
        queue: QUEUE.into(),
        uri,
    })
}

#[derive(Debug, Clone)]
struct QueueInfo {
    name: String,
    uri: String,
}

fn current_queues() -> Result<Vec<QueueInfo>> {
    let out = match Command::new("lpstat").arg("-v").output() {
        Ok(o) => o,
        Err(_) => return Ok(Vec::new()),
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut queues = Vec::new();
    for line in text.lines() {
        // device for NAME: URI
        if let Some(rest) = line.strip_prefix("device for ") {
            if let Some((name, uri)) = rest.split_once(':') {
                queues.push(QueueInfo {
                    name: name.trim().to_string(),
                    uri: uri.trim().to_string(),
                });
            }
        }
    }
    Ok(queues)
}

fn looks_like_m177(q: &QueueInfo) -> bool {
    let blob = format!("{} {}", q.name, q.uri).to_ascii_lowercase();
    blob.contains("m177")
        || blob.contains("26ba77")
        || blob.contains("color_laserjet_pro_mfp_m177")
}

/// Test helper: parse `lpstat -v` text without spawning.
pub fn parse_lpstat_v(text: &str) -> Vec<(String, String)> {
    let mut queues = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("device for ") {
            if let Some((name, uri)) = rest.split_once(':') {
                queues.push((name.trim().to_string(), uri.trim().to_string()));
            }
        }
    }
    queues
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_existing_airprint_queue() {
        let text = "device for HP_Color_LaserJet_Pro_MFP_M177fw: dnssd://HP%20Color%20LaserJet%20Pro%20MFP%20M177fw%5B26BA77%5D._ipp._tcp.local./?uuid=8adb6a9a-f28e-31fc-15de-50e0c4efdd92\n";
        let qs = parse_lpstat_v(text);
        assert_eq!(qs[0].0, "HP_Color_LaserJet_Pro_MFP_M177fw");
        let q = QueueInfo {
            name: qs[0].0.clone(),
            uri: qs[0].1.clone(),
        };
        assert!(looks_like_m177(&q));
    }
}
