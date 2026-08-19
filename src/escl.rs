//! eSCL (AirScan) XML used by the local Mac-facing facade and, if a device
//! actually implements ScanJobs, by the device client.
//!
//! The live M177fw serves ScannerCapabilities / ScannerStatus (old 2011/02/08
//! schema) but POST /eSCL/ScanJobs returns 404. The facade therefore speaks
//! modern eSCL (2011/05/03) toward the Mac and drives HP SOAP toward the MFP.

use crate::error::Result;
use crate::model::{
    ColorMode, OutputFormat, ScanRequest, ScanSource, PRODUCT_NAME,
};
use crate::xmlutil::first_text;

pub const ESCL_NS: &str = "http://schemas.hp.com/imaging/escl/2011/05/03";
pub const PWG_NS: &str = "http://www.pwg.org/schemas/2010/12/sm";

pub fn capabilities_xml(uuid: &str, make_and_model: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<scan:ScannerCapabilities xmlns:scan="{ESCL_NS}" xmlns:pwg="{PWG_NS}">
  <pwg:Version>2.63</pwg:Version>
  <pwg:MakeAndModel>{model}</pwg:MakeAndModel>
  <pwg:SerialNumber>M177FW</pwg:SerialNumber>
  <scan:UUID>{uuid}</scan:UUID>
  <scan:Platen>
    <scan:PlatenInputCaps>
      {caps_300}
      <scan:MaxOpticalXResolution>1200</scan:MaxOpticalXResolution>
      <scan:MaxOpticalYResolution>1200</scan:MaxOpticalYResolution>
    </scan:PlatenInputCaps>
  </scan:Platen>
  <scan:Adf>
    <scan:AdfSimplexInputCaps>
      {caps_300}
      <scan:MaxOpticalXResolution>300</scan:MaxOpticalXResolution>
      <scan:MaxOpticalYResolution>300</scan:MaxOpticalYResolution>
    </scan:AdfSimplexInputCaps>
    <scan:FeederCapacity>35</scan:FeederCapacity>
    <scan:AdfOptions>
      <scan:AdfOption>DetectPaperLoaded</scan:AdfOption>
    </scan:AdfOptions>
  </scan:Adf>
</scan:ScannerCapabilities>
"#,
        model = xml_escape(make_and_model),
        uuid = xml_escape(uuid),
        caps_300 = input_caps_block()
    )
}

fn input_caps_block() -> &'static str {
    r#"<scan:MinWidth>300</scan:MinWidth>
      <scan:MaxWidth>2550</scan:MaxWidth>
      <scan:MinHeight>300</scan:MinHeight>
      <scan:MaxHeight>3508</scan:MaxHeight>
      <scan:MaxScanRegions>1</scan:MaxScanRegions>
      <scan:SettingProfiles>
        <scan:SettingProfile>
          <scan:ColorModes>
            <scan:ColorMode>RGB24</scan:ColorMode>
            <scan:ColorMode>Grayscale8</scan:ColorMode>
            <scan:ColorMode>BlackAndWhite1</scan:ColorMode>
          </scan:ColorModes>
          <scan:DocumentFormats>
            <pwg:DocumentFormat>image/jpeg</pwg:DocumentFormat>
            <pwg:DocumentFormat>application/pdf</pwg:DocumentFormat>
            <scan:DocumentFormatExt>image/jpeg</scan:DocumentFormatExt>
            <scan:DocumentFormatExt>application/pdf</scan:DocumentFormatExt>
          </scan:DocumentFormats>
          <scan:SupportedResolutions>
            <scan:DiscreteResolutions>
              <scan:DiscreteResolution>
                <scan:XResolution>100</scan:XResolution>
                <scan:YResolution>100</scan:YResolution>
              </scan:DiscreteResolution>
              <scan:DiscreteResolution>
                <scan:XResolution>300</scan:XResolution>
                <scan:YResolution>300</scan:YResolution>
              </scan:DiscreteResolution>
              <scan:DiscreteResolution>
                <scan:XResolution>600</scan:XResolution>
                <scan:YResolution>600</scan:YResolution>
              </scan:DiscreteResolution>
            </scan:DiscreteResolutions>
          </scan:SupportedResolutions>
          <scan:ColorSpaces>
            <scan:ColorSpace>sRGB</scan:ColorSpace>
          </scan:ColorSpaces>
        </scan:SettingProfile>
      </scan:SettingProfiles>
      <scan:SupportedIntents>
        <scan:Intent>Document</scan:Intent>
        <scan:Intent>Photo</scan:Intent>
        <scan:Intent>Preview</scan:Intent>
      </scan:SupportedIntents>"#
}

pub fn status_xml(adf_empty: bool, jobs: &[(String, &str)]) -> String {
    let adf = if adf_empty {
        "ScannerAdfEmpty"
    } else {
        "ScannerAdfLoaded"
    };
    let mut jobs_xml = String::new();
    for (id, state) in jobs {
        jobs_xml.push_str(&format!(
            "<scan:JobInfo><pwg:JobUri>/eSCL/ScanJobs/{id}</pwg:JobUri>\
             <pwg:JobUuid>{id}</pwg:JobUuid><scan:Age>1</scan:Age>\
             <pwg:ImagesCompleted>0</pwg:ImagesCompleted>\
             <pwg:ImagesToTransfer>1</pwg:ImagesToTransfer>\
             <scan:JobState>{state}</scan:JobState></scan:JobInfo>"
        ));
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<scan:ScannerStatus xmlns:scan="{ESCL_NS}" xmlns:pwg="{PWG_NS}">
  <pwg:Version>2.63</pwg:Version>
  <pwg:State>Idle</pwg:State>
  <scan:AdfState>{adf}</scan:AdfState>
  <scan:Jobs>{jobs_xml}</scan:Jobs>
</scan:ScannerStatus>
"#
    )
}

pub fn parse_scan_settings(xml: &str) -> Result<ScanRequest> {
    let source_raw = first_text(xml, "InputSource").unwrap_or_else(|| "Platen".into());
    let source = if source_raw.eq_ignore_ascii_case("feeder")
        || source_raw.eq_ignore_ascii_case("adf")
    {
        ScanSource::Adf
    } else {
        ScanSource::Platen
    };
    let color_raw = first_text(xml, "ColorMode").unwrap_or_else(|| "RGB24".into());
    let color = {
        let n = color_raw.to_ascii_lowercase();
        if n.contains("gray") {
            ColorMode::Gray
        } else if n.contains("black") || n.contains("bw") || n.contains("lineart") {
            ColorMode::Lineart
        } else {
            ColorMode::Color
        }
    };
    let dpi = first_text(xml, "XResolution")
        .and_then(|s| s.parse().ok())
        .or_else(|| first_text(xml, "YResolution").and_then(|s| s.parse().ok()))
        .unwrap_or(300);
    let fmt_raw = first_text(xml, "DocumentFormat")
        .or_else(|| first_text(xml, "DocumentFormatExt"))
        .unwrap_or_else(|| "image/jpeg".into());
    let format = if fmt_raw.to_ascii_lowercase().contains("pdf") {
        OutputFormat::Pdf
    } else if fmt_raw.to_ascii_lowercase().contains("tif") {
        OutputFormat::Tiff
    } else {
        OutputFormat::Jpeg
    };
    Ok(ScanRequest {
        source,
        color,
        dpi,
        format,
        output: None,
        region: None,
    })
}

pub fn scan_settings_xml(req: &ScanRequest) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<scan:ScanSettings xmlns:scan="{ESCL_NS}" xmlns:pwg="{PWG_NS}">
  <pwg:Version>2.63</pwg:Version>
  <scan:Intent>Document</scan:Intent>
  <pwg:InputSource>{src}</pwg:InputSource>
  <scan:ColorMode>{color}</scan:ColorMode>
  <scan:XResolution>{dpi}</scan:XResolution>
  <scan:YResolution>{dpi}</scan:YResolution>
  <pwg:DocumentFormat>{fmt}</pwg:DocumentFormat>
  <scan:DocumentFormatExt>{fmt}</scan:DocumentFormatExt>
</scan:ScanSettings>
"#,
        src = req.source.escl_name(),
        color = req.color.escl_name(),
        dpi = req.dpi,
        fmt = req.format.mime()
    )
}

/// True if the XML looks like eSCL ScannerCapabilities and lists platen + ADF.
pub fn looks_like_capabilities(xml: &str) -> bool {
    xml.contains("ScannerCapabilities")
        && (xml.contains("Platen") || xml.contains("platen"))
}

pub fn parse_device_caps_summary(xml: &str) -> (bool, bool, bool) {
    let platen = xml.contains("Platen");
    let adf = xml.contains("<scc:Adf")
        || xml.contains("<scan:Adf")
        || xml.contains("<Adf")
        || xml.contains("AdfSimplex");
    let jobs_mentioned = xml.contains("ScanJobs") || xml.contains("DocumentFormat");
    (platen, adf, jobs_mentioned)
}

pub fn default_capabilities_xml() -> String {
    capabilities_xml(
        "8adb6a9a-f28e-31fc-15de-50e0c4efdd92",
        PRODUCT_NAME,
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facade_capabilities_name_required_sources_and_formats() {
        let xml = default_capabilities_xml();
        assert!(xml.contains("Platen"));
        assert!(xml.contains("Adf"));
        assert!(xml.contains("RGB24"));
        assert!(xml.contains("Grayscale8"));
        assert!(xml.contains("image/jpeg"));
        assert!(xml.contains("application/pdf"));
    }

    #[test]
    fn parse_settings_roundtrip() {
        let req = ScanRequest {
            source: ScanSource::Adf,
            color: ColorMode::Gray,
            dpi: 300,
            format: OutputFormat::Pdf,
            output: None,
            region: None,
        };
        let parsed = parse_scan_settings(&scan_settings_xml(&req)).unwrap();
        assert_eq!(parsed.source, ScanSource::Adf);
        assert_eq!(parsed.color, ColorMode::Gray);
        assert_eq!(parsed.dpi, 300);
        assert_eq!(parsed.format, OutputFormat::Pdf);
    }

    #[test]
    fn live_device_caps_fixture_is_escl() {
        let xml = include_str!("../fixtures/live/escl-ScannerCapabilities.xml");
        assert!(looks_like_capabilities(xml));
        let (platen, adf, _) = parse_device_caps_summary(xml);
        assert!(platen && adf);
        assert!(crate::xmlutil::texts_named(xml, "DocumentFormat")
            .iter()
            .any(|f| f.contains("jpeg") || f.contains("pdf")));
        assert!(first_text(xml, "MakeAndModel")
            .unwrap()
            .contains("M177fw"));
    }
}
