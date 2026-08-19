//! HP SOAP / WSD-like scan API used on TCP 8289 (`http://tempuri.org/wscn.xsd`).
//!
//! Live M177fw `GetScannerElements` succeeds and advertises JFIF + hpraw,
//! platen + ADF, RGB24 and GrayScale8. Job execution is CreateScanJobRequest
//! → GetJobInfo → RetrieveImageRequest (DIME).

use crate::error::{Error, Result};
use crate::model::{ColorMode, ScanRequest, ScanSource, SoapCapabilities};
use crate::xmlutil::{first_text, texts_named};

pub const SOAP_CONTENT_TYPE: &str = "text/xml; charset=utf-8";
pub const WSCN_NS: &str = "http://tempuri.org/wscn.xsd";

const ENVELOPE_OPEN: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<SOAP-ENV:Envelope xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:SOAP-ENC="http://www.w3.org/2003/05/soap-encoding" SOAP-ENV:encodingStyle="http://www.w3.org/2003/05/soap-encoding" xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope" xmlns:wscn="http://tempuri.org/wscn.xsd"><SOAP-ENV:Body>"#;
const ENVELOPE_CLOSE: &str = "</SOAP-ENV:Body></SOAP-ENV:Envelope>";

pub fn get_scanner_elements_xml() -> String {
    format!("{ENVELOPE_OPEN}<GetScannerElements xmlns=\"{WSCN_NS}\"/>{ENVELOPE_CLOSE}")
}

pub fn create_scan_job_xml(req: &ScanRequest, scan_id: &str) -> String {
    let source = req.source.soap_name();
    let color = req.color.soap_name();
    let dpi = req.dpi;
    // Firmware sizes are 1/1000 inch. Letter / A4-class platen.
    let (media_w, media_h) = req.media_size();
    let region = req.region_or_full();
    let images = match req.source {
        ScanSource::Platen => 1,
        ScanSource::Adf => 0,
    };
    format!(
        r#"{ENVELOPE_OPEN}<wscn:CreateScanJobRequest>
<ScanIdentifier xsi:type="xsd:string">{id}</ScanIdentifier>
<ScanTicket xmlns="{WSCN_NS}" xsi:type="ScanTicketType">
<JobDescription xmlns="{WSCN_NS}" xsi:type="JobDescriptionType">
<JobOriginatingUserName xsi:type="xsd:string">hp-m177</JobOriginatingUserName>
<JobName xsi:type="xsd:string">hp-m177-scan</JobName>
</JobDescription>
<DocumentParameters xmlns="{WSCN_NS}" xsi:type="DocumentParametersType">
<Format xmlns="{WSCN_NS}" xsi:type="ScanDocumentFormat">jfif</Format>
<ImagesToTransfer xsi:type="xsd:int">{images}</ImagesToTransfer>
<Exposure xmlns="{WSCN_NS}" xsi:type="ScanExposureType">
<AutoExposure xsi:type="xsd:boolean">true</AutoExposure>
<ExposureSettings xmlns="{WSCN_NS}" xsi:type="ExposureSettingsOverrideType">
<Contrast xsi:type="xsd:int">0</Contrast>
</ExposureSettings>
</Exposure>
<ContentType xsi:type="ContentType">Auto</ContentType>
<CompressionQualityFactor xsi:type="xsd:int">0</CompressionQualityFactor>
<InputSource xsi:type="ScanInputSource">{source}</InputSource>
<InputSize xmlns="{WSCN_NS}" xsi:type="DocumentInputSizeType">
<InputMediaSize xmlns="{WSCN_NS}" xsi:type="DimensionsType">
<Width xsi:type="xsd:int">{media_w}</Width>
<Height xsi:type="xsd:int">{media_h}</Height>
</InputMediaSize>
</InputSize>
<MediaSides xmlns="{WSCN_NS}" xsi:type="MediaSideOverrideType">
<MediaFront xmlns="{WSCN_NS}" xsi:type="MediaSideOverrideType">
<Resolution xmlns="{WSCN_NS}" xsi:type="MediaSideOverrideType">
<Width xsi:type="xsd:int">{dpi}</Width>
<Height xsi:type="xsd:int">{dpi}</Height>
</Resolution>
<ScanRegion xmlns="{WSCN_NS}" xsi:type="ScanRegionType">
<ScanRegionXOffset xsi:type="xsd:int">{rx}</ScanRegionXOffset>
<ScanRegionYOffset xsi:type="xsd:int">{ry}</ScanRegionYOffset>
<ScanRegionHeight xsi:type="xsd:int">{rh}</ScanRegionHeight>
<ScanRegionWidth xsi:type="xsd:int">{rw}</ScanRegionWidth>
</ScanRegion>
<ColorProcessing xsi:type="ColorEntryType">{color}</ColorProcessing>
</MediaFront>
</MediaSides>
</DocumentParameters>
<RetrieveImageTimeout xsi:type="xsd:int">1800</RetrieveImageTimeout>
</ScanTicket>
</wscn:CreateScanJobRequest>{ENVELOPE_CLOSE}"#,
        id = xml_escape(scan_id),
        rx = region.x,
        ry = region.y,
        rw = region.width,
        rh = region.height,
    )
}

/// Same ticket, operation name without the WSD `Request` suffix. Some gSOAP
/// bindings (this firmware's GetScannerElements) use the short name.
pub fn create_scan_job_short_xml(req: &ScanRequest, scan_id: &str) -> String {
    create_scan_job_xml(req, scan_id).replace("CreateScanJobRequest", "CreateScanJob")
}

pub fn get_job_info_xml(job_id: &str) -> String {
    format!(
        r#"{ENVELOPE_OPEN}<wscn:GetJobInfo>
<JobId xsi:type="xsd:int">{id}</JobId>
</wscn:GetJobInfo>{ENVELOPE_CLOSE}"#,
        id = xml_escape(job_id)
    )
}

pub fn retrieve_image_xml(job_id: &str, job_token: &str) -> String {
    format!(
        r#"{ENVELOPE_OPEN}<wscn:RetrieveImageRequest>
<JobId xsi:type="xsd:int">{id}</JobId>
<JobToken xsi:type="xsd:string">{token}</JobToken>
</wscn:RetrieveImageRequest>{ENVELOPE_CLOSE}"#,
        id = xml_escape(job_id),
        token = xml_escape(job_token)
    )
}

pub fn cancel_job_xml(job_id: &str) -> String {
    format!(
        r#"{ENVELOPE_OPEN}<wscn:CancelJobRequest>
<JobId xsi:type="xsd:int">{id}</JobId>
</wscn:CancelJobRequest>{ENVELOPE_CLOSE}"#,
        id = xml_escape(job_id)
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedJob {
    pub job_id: String,
    pub job_token: String,
}

pub fn parse_create_job(xml: &str) -> Result<CreatedJob> {
    if let Some(fault) = soap_fault(xml) {
        return Err(Error::protocol(fault));
    }
    let job_id = first_text(xml, "JobId").ok_or_else(|| Error::protocol("CreateScanJob response missing JobId"))?;
    let job_token = first_text(xml, "JobToken").unwrap_or_default();
    Ok(CreatedJob { job_id, job_token })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobInfo {
    pub job_id: String,
    pub state: String,
    pub scans_completed: u32,
}

impl JobInfo {
    pub fn image_ready(&self) -> bool {
        let s = self.state.to_ascii_lowercase();
        s == "processing" || s == "completed" || self.scans_completed > 0
    }

    pub fn finished(&self) -> bool {
        let s = self.state.to_ascii_lowercase();
        matches!(
            s.as_str(),
            "completed" | "canceled" | "cancelled" | "aborted" | "terminated"
        )
    }
}

pub fn parse_job_info(xml: &str) -> Result<JobInfo> {
    if let Some(fault) = soap_fault(xml) {
        return Err(Error::protocol(fault));
    }
    Ok(JobInfo {
        job_id: first_text(xml, "JobId").unwrap_or_default(),
        state: first_text(xml, "JobState").unwrap_or_else(|| "Unknown".into()),
        scans_completed: first_text(xml, "ScansCompleted")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
    })
}

pub fn parse_capabilities(xml: &str) -> Result<SoapCapabilities> {
    if let Some(fault) = soap_fault(xml) {
        return Err(Error::protocol(fault));
    }
    if !xml.contains("ScanElements") && !xml.contains("ScannerConfiguration") {
        return Err(Error::protocol(
            "SOAP body is not a GetScannerElements response",
        ));
    }
    let formats = texts_named(xml, "item")
        .into_iter()
        .filter(|s| {
            let n = s.to_ascii_lowercase();
            n == "jfif" || n == "hpraw" || n == "jpeg" || n == "pdf" || n == "exif"
        })
        .collect();
    // Color items share the generic <item> tag with formats; pick known colors.
    let colors: Vec<String> = texts_named(xml, "item")
        .into_iter()
        .filter(|s| {
            let n = s.to_ascii_lowercase();
            n.contains("rgb") || n.contains("gray") || n.contains("black") || n.contains("bw")
        })
        .collect();
    let platen = first_text(xml, "FlatbedSupported")
        .map(|s| is_true(&s))
        .unwrap_or(true);
    let adf = first_text(xml, "ADFSupported")
        .map(|s| is_true(&s))
        .unwrap_or(false);
    let adf_duplex = first_text(xml, "ADFSupportsDuplex")
        .map(|s| is_true(&s))
        .unwrap_or(false);
    let state = first_text(xml, "ScannerState").unwrap_or_else(|| "Unknown".into());
    let paper_in_adf = first_text(xml, "PaperInADF")
        .map(|s| is_true(&s))
        .unwrap_or(false);
    let widths: Vec<u32> = texts_named(xml, "Width")
        .into_iter()
        .filter_map(|s| s.parse().ok())
        .collect();
    let heights: Vec<u32> = texts_named(xml, "Height")
        .into_iter()
        .filter_map(|s| s.parse().ok())
        .collect();
    // Order in the live fixture: platen min, platen max, optical, adf min, adf max, optical.
    let platen_max = (
        widths.get(1).copied().unwrap_or(8500),
        heights.get(1).copied().unwrap_or(11690),
    );
    let adf_max = (
        widths.get(4).copied().or_else(|| widths.get(1).copied()).unwrap_or(8500),
        heights.get(4).copied().or_else(|| heights.get(1).copied()).unwrap_or(14000),
    );
    Ok(SoapCapabilities {
        formats,
        colors,
        platen,
        adf,
        adf_duplex,
        state,
        paper_in_adf,
        platen_max,
        adf_max,
        platen_optical_dpi: widths.get(2).copied().unwrap_or(1200),
        adf_optical_dpi: widths.get(5).copied().unwrap_or(300),
    })
}

pub fn soap_fault(xml: &str) -> Option<String> {
    if !xml.contains("Fault") {
        return None;
    }
    let reason = first_text(xml, "Text")
        .or_else(|| first_text(xml, "Reason"))
        .or_else(|| first_text(xml, "faultstring"));
    let sub = first_text(xml, "Value")
        .into_iter()
        .find(|v| v.contains("Error") || v.contains("Client") || v.contains("Server"));
    match (sub, reason) {
        (Some(s), Some(r)) => Some(format!("{s}: {r}")),
        (Some(s), None) => Some(s),
        (None, Some(r)) => Some(r),
        (None, None) => None,
    }
}

pub fn is_no_images_fault(err: &str) -> bool {
    let n = err.to_ascii_lowercase();
    n.contains("noimagesavailable") || n.contains("no images")
}

pub fn parse_job_ticket(xml: &str) -> Option<ScanRequest> {
    let source = first_text(xml, "InputSource")?;
    let color = first_text(xml, "ColorProcessing").unwrap_or_else(|| "RGB24".into());
    let widths = texts_named(xml, "Width");
    // First Width in the ticket is media; resolution Width comes later.
    let dpi = widths
        .iter()
        .rev()
        .find_map(|w| w.parse().ok())
        .filter(|&d| (50..=1200).contains(&d))
        .or_else(|| {
            texts_named(xml, "Height")
                .into_iter()
                .filter_map(|s| s.parse().ok())
                .find(|&d| (50..=1200).contains(&d))
        })
        .unwrap_or(300);
    Some(ScanRequest {
        source: ScanSource::parse(&source).unwrap_or(ScanSource::Platen),
        color: {
            let n = color.to_ascii_lowercase();
            if n.contains("gray") {
                ColorMode::Gray
            } else if n.contains("black") || n.contains("bw") || n.contains("lineart") {
                ColorMode::Lineart
            } else {
                ColorMode::Color
            }
        },
        dpi,
        format: crate::model::OutputFormat::Jpeg,
        output: None,
        region: {
            let x = first_text(xml, "ScanRegionXOffset").and_then(|s| s.parse().ok());
            let y = first_text(xml, "ScanRegionYOffset").and_then(|s| s.parse().ok());
            let w = first_text(xml, "ScanRegionWidth").and_then(|s| s.parse().ok());
            let h = first_text(xml, "ScanRegionHeight").and_then(|s| s.parse().ok());
            match (x, y, w, h) {
                (Some(x), Some(y), Some(width), Some(height)) if width > 0 && height > 0 => {
                    Some(crate::model::ScanRegion {
                        x,
                        y,
                        width,
                        height,
                    })
                }
                _ => None,
            }
        },
    })
}

fn is_true(s: &str) -> bool {
    matches!(s.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// SOAP XML record placed in front of the JPEG inside a DIME body.
pub fn retrieve_image_soap_stub() -> &'static [u8] {
    br#"<?xml version="1.0" encoding="UTF-8"?>
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope">
<SOAP-ENV:Body><RetrieveImageRequestResponse><Response href="cid:image"/></RetrieveImageRequestResponse></SOAP-ENV:Body>
</SOAP-ENV:Envelope>"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::OutputFormat;

    #[test]
    fn create_job_xml_carries_user_options() {
        let req = ScanRequest {
            source: ScanSource::Adf,
            color: ColorMode::Gray,
            dpi: 300,
            format: OutputFormat::Jpeg,
            output: None,
            region: None,
        };
        let xml = create_scan_job_xml(&req, "job-1");
        assert!(xml.contains("CreateScanJobRequest"));
        assert!(xml.contains("<InputSource"));
        assert!(xml.contains(">ADF<"));
        assert!(xml.contains("GrayScale8"));
        assert!(xml.contains(">300<"));
        assert!(xml.contains("jfif"));
        let parsed = parse_job_ticket(&xml).unwrap();
        assert_eq!(parsed.source, ScanSource::Adf);
        assert_eq!(parsed.color, ColorMode::Gray);
        assert_eq!(parsed.dpi, 300);
    }

    #[test]
    fn parse_live_get_scanner_elements_fixture() {
        let xml = include_str!("../fixtures/live/soap-GetScannerElements.xml");
        let caps = parse_capabilities(xml).unwrap();
        assert!(caps.platen);
        assert!(caps.adf);
        assert!(!caps.adf_duplex);
        assert!(caps.supports_color(ColorMode::Color));
        assert!(caps.supports_color(ColorMode::Gray));
        assert!(caps.formats.iter().any(|f| f == "jfif"));
        assert_eq!(caps.state, "Idle");
    }
}
