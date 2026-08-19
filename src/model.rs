use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Model this product is written against.
pub const PRODUCT_NAME: &str = "HP Color LaserJet Pro MFP M177fw";
pub const PRODUCT_SHORT: &str = "M177fw";
pub const DEFAULT_SOAP_PORT: u16 = 8289;
pub const DEFAULT_ESCL_PORT: u16 = 80;
pub const DEFAULT_WSD_PORT: u16 = 3911;
pub const DEFAULT_IPP_PORT: u16 = 631;
pub const DEFAULT_BRIDGE_PORT: u16 = 8087;

/// Where the original is placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanSource {
    Platen,
    Adf,
}

impl ScanSource {
    pub fn parse(raw: &str) -> crate::error::Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "platen" | "flatbed" | "glass" => Ok(Self::Platen),
            "adf" | "feeder" => Ok(Self::Adf),
            other => Err(crate::error::Error::InvalidRequest(format!(
                "unknown source '{other}' (use platen or adf)"
            ))),
        }
    }

    /// Value used in the HP SOAP `InputSource` element.
    pub fn soap_name(self) -> &'static str {
        match self {
            Self::Platen => "Platen",
            Self::Adf => "ADF",
        }
    }

    /// Value used in eSCL / PWG `InputSource`.
    pub fn escl_name(self) -> &'static str {
        match self {
            Self::Platen => "Platen",
            Self::Adf => "Feeder",
        }
    }
}

impl fmt::Display for ScanSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Platen => "platen",
            Self::Adf => "adf",
        })
    }
}

/// Color processing requested of the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorMode {
    Color,
    Gray,
    Lineart,
}

impl ColorMode {
    pub fn parse(raw: &str) -> crate::error::Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "color" | "rgb" | "rgb24" | "colour" => Ok(Self::Color),
            "gray" | "grey" | "grayscale" | "greyscale" | "grayscale8" => Ok(Self::Gray),
            "lineart" | "bw" | "b/w" | "black" | "binary" | "blackandwhite1"
            | "blackandwhite" => Ok(Self::Lineart),
            other => Err(crate::error::Error::InvalidRequest(format!(
                "unknown color mode '{other}' (use color, gray, or lineart)"
            ))),
        }
    }

    /// HP SOAP / WSD `ColorProcessing` value as returned by this firmware.
    pub fn soap_name(self) -> &'static str {
        match self {
            Self::Color => "RGB24",
            Self::Gray => "GrayScale8",
            Self::Lineart => "BlackandWhite1",
        }
    }

    /// eSCL `ColorMode` value.
    pub fn escl_name(self) -> &'static str {
        match self {
            Self::Color => "RGB24",
            Self::Gray => "Grayscale8",
            Self::Lineart => "BlackAndWhite1",
        }
    }
}

impl fmt::Display for ColorMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Color => "color",
            Self::Gray => "gray",
            Self::Lineart => "lineart",
        })
    }
}

/// File the user asked to receive. Device jobs return JPEG or BMP/`dib`;
/// PDF and TIFF are produced locally from that raster.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Jpeg,
    Pdf,
    Tiff,
}

impl OutputFormat {
    pub fn parse(raw: &str) -> crate::error::Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" | "jfif" | "image/jpeg" => Ok(Self::Jpeg),
            "pdf" | "application/pdf" => Ok(Self::Pdf),
            "tif" | "tiff" | "image/tiff" => Ok(Self::Tiff),
            other => Err(crate::error::Error::InvalidRequest(format!(
                "unknown format '{other}' (use jpeg, pdf, or tiff)"
            ))),
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Pdf => "pdf",
            Self::Tiff => "tiff",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Pdf => "application/pdf",
            Self::Tiff => "image/tiff",
        }
    }
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Jpeg => "jpeg",
            Self::Pdf => "pdf",
            Self::Tiff => "tiff",
        })
    }
}

/// Scan window in the firmware's 1/1000 inch units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl ScanRegion {
    pub fn parse(raw: &str) -> crate::error::Result<Self> {
        let parts: Vec<&str> = raw.split(',').map(str::trim).collect();
        if parts.len() != 4 {
            return Err(crate::error::Error::InvalidRequest(
                "region must be x,y,width,height in 1/1000 inch".into(),
            ));
        }
        let nums: Result<Vec<u32>, _> = parts.iter().map(|p| p.parse()).collect();
        let nums = nums.map_err(|_| {
            crate::error::Error::InvalidRequest("region values must be integers".into())
        })?;
        if nums[2] == 0 || nums[3] == 0 {
            return Err(crate::error::Error::InvalidRequest(
                "region width and height must be > 0".into(),
            ));
        }
        Ok(Self {
            x: nums[0],
            y: nums[1],
            width: nums[2],
            height: nums[3],
        })
    }
}

/// Which job API actually produces images on this host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum JobProtocol {
    /// HP SOAP/DIME cycle on port 8289 (`http://tempuri.org/wscn.xsd`).
    Soap { port: u16 },
    /// Native eSCL ScanJobs (present on some firmware; this M177fw returns 404).
    Escl { port: u16 },
    /// Microsoft WSD Scan on port 3911 (`/scanner`, document format `dib`).
    Wsd { port: u16 },
}

impl JobProtocol {
    pub fn soap(port: u16) -> Self {
        Self::Soap { port }
    }
}

/// A persisted scanner the user added.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRecord {
    pub id: String,
    pub name: String,
    pub host: String,
    pub job: JobProtocol,
    pub has_escl_caps: bool,
    pub has_platen: bool,
    pub has_adf: bool,
    pub uuid: Option<String>,
}

impl DeviceRecord {
    pub fn soap_url(&self) -> Option<String> {
        match self.job {
            JobProtocol::Soap { port } => Some(format!("http://{}:{}/", self.host, port)),
            JobProtocol::Escl { .. } | JobProtocol::Wsd { .. } => None,
        }
    }

    pub fn escl_base(&self) -> Option<String> {
        match self.job {
            JobProtocol::Escl { port } => Some(format!("http://{}:{}/eSCL", self.host, port)),
            JobProtocol::Soap { .. } | JobProtocol::Wsd { .. } if self.has_escl_caps => {
                Some(format!("http://{}:{}/eSCL", self.host, DEFAULT_ESCL_PORT))
            }
            _ => None,
        }
    }
}

/// Parameters for one scan job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanRequest {
    pub source: ScanSource,
    pub color: ColorMode,
    pub dpi: u32,
    pub format: OutputFormat,
    pub output: Option<PathBuf>,
    pub region: Option<ScanRegion>,
}

impl Default for ScanRequest {
    fn default() -> Self {
        Self {
            source: ScanSource::Platen,
            color: ColorMode::Color,
            dpi: 300,
            format: OutputFormat::Jpeg,
            output: None,
            region: None,
        }
    }
}

impl ScanRequest {
    pub fn validate(&self) -> crate::error::Result<()> {
        if self.dpi == 0 || self.dpi > 1200 {
            return Err(crate::error::Error::InvalidRequest(format!(
                "resolution {} dpi is out of range (1–1200)",
                self.dpi
            )));
        }
        Ok(())
    }

    /// Firmware media box in 1/1000 inch (Letter / A4-class).
    pub fn media_size(&self) -> (u32, u32) {
        match self.source {
            ScanSource::Platen => (8500, 11690),
            ScanSource::Adf => (8500, 14000),
        }
    }

    pub fn region_or_full(&self) -> ScanRegion {
        let (width, height) = self.media_size();
        self.region.unwrap_or(ScanRegion {
            x: 0,
            y: 0,
            width,
            height,
        })
    }
}

/// macOS Documents folder, or the current directory if HOME is unset.
pub fn default_scan_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join("Documents"))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// `~/Documents/scan-<unix-seconds>.<ext>`
pub fn default_scan_path(format: OutputFormat) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    default_scan_dir().join(format!("scan-{ts}.{}", format.extension()))
}

/// Bytes returned by a completed job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanOutput {
    pub bytes: Vec<u8>,
    pub format: OutputFormat,
    pub source: ScanSource,
    pub color: ColorMode,
    pub dpi: u32,
}

/// Result of probing a host for scan endpoints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeResult {
    pub host: String,
    pub name: String,
    pub soap: Option<SoapCapabilities>,
    pub escl_caps: bool,
    pub escl_jobs: bool,
    pub preferred: Option<JobProtocol>,
}

impl ProbeResult {
    pub fn into_device(self) -> crate::error::Result<DeviceRecord> {
        let job = self.preferred.clone().ok_or_else(|| {
            crate::error::Error::NoScanProtocol {
                host: self.host.clone(),
                soap_port: DEFAULT_SOAP_PORT,
            }
        })?;
        let (has_platen, has_adf) = match &self.soap {
            Some(c) => (c.platen, c.adf),
            None => (true, true),
        };
        Ok(DeviceRecord {
            id: slug_id(&self.host),
            name: self.name,
            host: self.host,
            job,
            has_escl_caps: self.escl_caps,
            has_platen,
            has_adf,
            uuid: None,
        })
    }
}

/// Subset of SOAP `GetScannerElements` we care about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoapCapabilities {
    pub formats: Vec<String>,
    pub colors: Vec<String>,
    pub platen: bool,
    pub adf: bool,
    pub adf_duplex: bool,
    pub state: String,
    pub paper_in_adf: bool,
    pub platen_max: (u32, u32),
    pub adf_max: (u32, u32),
    pub platen_optical_dpi: u32,
    pub adf_optical_dpi: u32,
}

impl SoapCapabilities {
    pub fn supports_color(&self, mode: ColorMode) -> bool {
        let needle = mode.soap_name().to_ascii_lowercase();
        self.colors
            .iter()
            .any(|c| c.to_ascii_lowercase().replace('-', "") == needle.to_ascii_lowercase())
            || self.colors.iter().any(|c| {
                let n = c.to_ascii_lowercase();
                match mode {
                    ColorMode::Color => n.contains("rgb24"),
                    ColorMode::Gray => n.contains("gray"),
                    ColorMode::Lineart => n.contains("black") || n.contains("bw"),
                }
            })
    }
}

/// A device seen on the LAN (mDNS) before it is probed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredDevice {
    pub name: String,
    pub host: String,
    pub service: String,
    pub port: u16,
}

pub fn slug_id(host: &str) -> String {
    let cleaned: String = host
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "scanner".into()
    } else {
        cleaned
    }
}

/// Outcome of the optional AirPrint add-printer helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrintAddOutcome {
    LeftExisting { queue: String },
    Added { queue: String, uri: String },
    Skipped { reason: String },
}
