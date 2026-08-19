use crate::error::{Error, Result};
use crate::model::DeviceRecord;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const ENV_HOME: &str = "HP_M177_HOME";
const FILENAME: &str = "devices.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceStoreFile {
    pub devices: Vec<DeviceRecord>,
    #[serde(default)]
    pub default_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Store {
    pub path: PathBuf,
    data: DeviceStoreFile,
}

impl Store {
    pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref();
        fs::create_dir_all(dir)?;
        let path = dir.join(FILENAME);
        let data = if path.exists() {
            let raw = fs::read_to_string(&path)?;
            serde_json::from_str(&raw)?
        } else {
            DeviceStoreFile::default()
        };
        Ok(Self { path, data })
    }

    pub fn from_env_or_default() -> Result<Self> {
        Self::open(config_dir())
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let raw = serde_json::to_string_pretty(&self.data)?;
        fs::write(&self.path, raw)?;
        Ok(())
    }

    pub fn upsert(&mut self, device: DeviceRecord) -> Result<DeviceRecord> {
        if let Some(existing) = self
            .data
            .devices
            .iter_mut()
            .find(|d| d.id == device.id || d.host == device.host)
        {
            *existing = device.clone();
        } else {
            self.data.devices.push(device.clone());
        }
        self.data.default_id = Some(device.id.clone());
        self.save()?;
        Ok(device)
    }

    pub fn list(&self) -> &[DeviceRecord] {
        &self.data.devices
    }

    pub fn get(&self, id_or_host: &str) -> Result<DeviceRecord> {
        self.data
            .devices
            .iter()
            .find(|d| d.id == id_or_host || d.host == id_or_host || d.name == id_or_host)
            .cloned()
            .ok_or_else(|| Error::UnknownDevice(id_or_host.to_string()))
    }

    pub fn default_device(&self) -> Result<DeviceRecord> {
        if let Some(id) = &self.data.default_id {
            if let Ok(d) = self.get(id) {
                return Ok(d);
            }
        }
        self.data
            .devices
            .first()
            .cloned()
            .ok_or_else(|| Error::msg("no scanners added yet; run `hp-m177 add <host>`"))
    }
}

pub fn config_dir() -> PathBuf {
    if let Ok(p) = std::env::var(ENV_HOME) {
        return PathBuf::from(p);
    }
    if let Some(base) = std::env::var_os("HOME") {
        return PathBuf::from(base)
            .join("Library")
            .join("Application Support")
            .join("hp-m177");
    }
    PathBuf::from(".").join(".hp-m177")
}
