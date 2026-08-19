//! Microsoft WSD Scan Service on TCP 3911.
//!
//! Live M177fw: `POST http://<host>:3911/scanner` with WS-Addressing.
//! Document format is `dib` only. RetrieveImage is MTOM/XOP with `image/bmp`.

use crate::error::{Error, Result};
use crate::imagefmt;
use crate::model::{ScanRequest, ScanSource};
use crate::transport::HttpRequest;
use std::time::Duration;
use uuid::Uuid;

pub const WSD_SCAN_NS: &str = "http://schemas.microsoft.com/windows/2006/08/wdp/scan";
pub const WSA_NS: &str = "http://schemas.xmlsoap.org/ws/2004/08/addressing";
pub const ACTION_CREATE: &str =
    "http://schemas.microsoft.com/windows/2006/08/wdp/scan/CreateScanJob";
pub const ACTION_RETRIEVE: &str =
    "http://schemas.microsoft.com/windows/2006/08/wdp/scan/RetrieveImage";
pub const ACTION_TRANSFER_GET: &str = "http://schemas.xmlsoap.org/ws/2004/09/transfer/Get";

pub fn scanner_url(host: &str, port: u16) -> String {
    format!("http://{host}:{port}/scanner")
}

pub fn envelope(to: &str, action: &str, body: &str) -> String {
    let msgid = format!("urn:uuid:{}", Uuid::new_v4());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope" xmlns:a="{WSA_NS}" xmlns:sca="{WSD_SCAN_NS}">
<s:Header>
<a:Action s:mustUnderstand="1">{action}</a:Action>
<a:MessageID>{msgid}</a:MessageID>
<a:To s:mustUnderstand="1">{to}</a:To>
<a:ReplyTo><a:Address>http://schemas.xmlsoap.org/ws/2004/08/addressing/role/anonymous</a:Address></a:ReplyTo>
</s:Header>
<s:Body>{body}</s:Body>
</s:Envelope>"#
    )
}

pub fn transfer_get_xml(to: &str) -> String {
    envelope(to, ACTION_TRANSFER_GET, "")
}

pub fn create_scan_job_xml(to: &str, req: &ScanRequest) -> String {
    let source = req.source.soap_name();
    let color = req.color.soap_name();
    let dpi = req.dpi;
    let (media_w, media_h) = req.media_size();
    let region = req.region_or_full();
    let images = match req.source {
        ScanSource::Platen => 1,
        ScanSource::Adf => 0,
    };
    let inner = format!(
        r#"<sca:CreateScanJobRequest><sca:ScanTicket>
<sca:JobDescription>
<sca:JobName>hp-m177</sca:JobName>
<sca:JobOriginatingUserName>hp-m177</sca:JobOriginatingUserName>
<sca:JobInformation>hp-m177</sca:JobInformation>
</sca:JobDescription>
<sca:DocumentParameters>
<sca:Format>dib</sca:Format>
<sca:ImagesToTransfer>{images}</sca:ImagesToTransfer>
<sca:ContentType>Photo</sca:ContentType>
<sca:InputSize><sca:InputMediaSize><sca:Width>{media_w}</sca:Width><sca:Height>{media_h}</sca:Height></sca:InputMediaSize></sca:InputSize>
<sca:InputSource>{source}</sca:InputSource>
<sca:MediaSides><sca:MediaFront>
<sca:ColorProcessing>{color}</sca:ColorProcessing>
<sca:Resolution><sca:Width>{dpi}</sca:Width><sca:Height>{dpi}</sca:Height></sca:Resolution>
<sca:ScanRegion>
<sca:ScanRegionXOffset>{rx}</sca:ScanRegionXOffset>
<sca:ScanRegionYOffset>{ry}</sca:ScanRegionYOffset>
<sca:ScanRegionWidth>{rw}</sca:ScanRegionWidth>
<sca:ScanRegionHeight>{rh}</sca:ScanRegionHeight>
</sca:ScanRegion>
</sca:MediaFront></sca:MediaSides>
</sca:DocumentParameters>
</sca:ScanTicket></sca:CreateScanJobRequest>"#,
        rx = region.x,
        ry = region.y,
        rw = region.width,
        rh = region.height,
    );
    envelope(to, ACTION_CREATE, &inner)
}

pub fn retrieve_image_xml(to: &str, job_id: &str, token: &str) -> String {
    let inner = format!(
        r#"<sca:RetrieveImageRequest>
<sca:DocumentDescription><sca:DocumentName>IMAGE000.BMP</sca:DocumentName></sca:DocumentDescription>
<sca:JobId>{id}</sca:JobId>
<sca:JobToken>{token}</sca:JobToken>
</sca:RetrieveImageRequest>"#,
        id = xml_escape(job_id),
        token = xml_escape(token)
    );
    envelope(to, ACTION_RETRIEVE, &inner)
}

pub fn content_type(action: &str) -> String {
    format!(r#"application/soap+xml; charset=utf-8; action="{action}""#)
}

pub fn request(url: &str, action: &str, xml: String, timeout: Duration) -> HttpRequest {
    HttpRequest::post(url, xml.into_bytes(), &content_type(action))
        .header("User-Agent", "WSDAPI")
        .with_timeout(timeout)
}

pub fn looks_alive(body: &[u8]) -> bool {
    let t = String::from_utf8_lossy(body);
    t.contains("Envelope") || t.contains("wsa:") || t.contains("Fault") || t.contains("wscn:")
}

/// Pull a BMP or JPEG out of a WSD RetrieveImage body (raw or MTOM/XOP).
pub fn extract_image(body: &[u8]) -> Result<Vec<u8>> {
    if imagefmt::is_jpeg(body) {
        return Ok(body.to_vec());
    }
    if imagefmt::is_bmp(body) {
        return clip_bmp(body);
    }
    if let Some(part) = mime_part(body, b"image/bmp")
        .or_else(|| mime_part(body, b"image/jpeg"))
        .or_else(|| mime_part(body, b"image/dib"))
    {
        if imagefmt::is_jpeg(&part) || part.starts_with(&[0xff, 0xd8]) {
            return Ok(part);
        }
        if imagefmt::is_bmp(&part) || imagefmt::is_dib(&part) {
            return clip_bmp(&part);
        }
    }
    if let Some(i) = find_sub(body, b"BM") {
        return clip_bmp(&body[i..]);
    }
    if let Some(i) = find_sub(body, &[0xff, 0xd8]) {
        return Ok(body[i..].to_vec());
    }
    Err(Error::protocol(
        "WSD RetrieveImage had no BMP or JPEG part",
    ))
}

/// MTOM/XOP body the live M177fw returns for RetrieveImage.
pub fn wrap_bmp_mtom(bmp: &[u8]) -> Vec<u8> {
    let boundary = b"==hp-m177-wsd";
    let mut out = Vec::new();
    out.extend_from_slice(b"--");
    out.extend_from_slice(boundary);
    out.extend_from_slice(b"\r\nContent-Type: application/xop+xml\r\n\r\n");
    out.extend_from_slice(br#"<?xml version="1.0"?><SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope"><SOAP-ENV:Body><RetrieveImageResponse/></SOAP-ENV:Body></SOAP-ENV:Envelope>"#);
    out.extend_from_slice(b"\r\n--");
    out.extend_from_slice(boundary);
    out.extend_from_slice(b"\r\nContent-Type: image/bmp\r\n\r\n");
    out.extend_from_slice(bmp);
    out.extend_from_slice(b"\r\n--");
    out.extend_from_slice(boundary);
    out.extend_from_slice(b"--\r\n");
    out
}

fn clip_bmp(bytes: &[u8]) -> Result<Vec<u8>> {
    if imagefmt::is_dib(bytes) && !imagefmt::is_bmp(bytes) {
        return Ok(bytes.to_vec());
    }
    if bytes.len() < 6 || !bytes.starts_with(b"BM") {
        return Err(Error::protocol("truncated BMP"));
    }
    let size = u32::from_le_bytes(bytes[2..6].try_into().unwrap()) as usize;
    if size >= 54 && size <= bytes.len() {
        Ok(bytes[..size].to_vec())
    } else if bytes.len() >= 54 {
        Ok(bytes.to_vec())
    } else {
        Err(Error::protocol("truncated BMP"))
    }
}

fn mime_part(body: &[u8], content_type: &[u8]) -> Option<Vec<u8>> {
    let pos = find_sub(body, content_type)?;
    let rel = find_sub(&body[pos..], b"\r\n\r\n")?;
    let start = pos + rel + 4;
    let rest = body.get(start..)?;
    let end = find_sub(rest, b"\r\n--").unwrap_or(rest.len());
    Some(rest[..end].to_vec())
}

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ColorMode, OutputFormat};

    #[test]
    fn create_job_xml_is_wsd_dib() {
        let req = ScanRequest {
            source: ScanSource::Platen,
            color: ColorMode::Color,
            dpi: 300,
            format: OutputFormat::Jpeg,
            output: None,
            region: None,
        };
        let xml = create_scan_job_xml("http://192.168.50.14:3911/scanner", &req);
        assert!(xml.contains("CreateScanJobRequest"));
        assert!(xml.contains("<sca:Format>dib</sca:Format>"));
        assert!(xml.contains(">Platen<"));
        assert!(xml.contains("RGB24"));
        assert!(xml.contains(">300<"));
        assert!(xml.contains(WSD_SCAN_NS));
        assert!(xml.contains(ACTION_CREATE));
    }

    #[test]
    fn extract_mtom_bmp_uses_bfsize() {
        let bmp = crate::imagefmt::solid_bmp_bgra(2, 2, 0, 0, 255);
        let mut body = Vec::new();
        body.extend_from_slice(b"--==bound\r\nContent-Type: application/xop+xml\r\n\r\n<SOAP/>\r\n--==bound\r\nContent-Type: image/bmp\r\n\r\n");
        body.extend_from_slice(&bmp);
        body.extend_from_slice(b"\r\n--==bound--\r\n");
        let got = extract_image(&body).unwrap();
        assert!(imagefmt::is_bmp(&got));
        assert_eq!(got, bmp);
        let jpeg = imagefmt::raster_to_jpeg(&got).unwrap();
        assert!(imagefmt::is_jpeg(&jpeg));
    }
}
