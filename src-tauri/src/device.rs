//! Settings that belong to this computer, not to a profile: closing to the tray, the global
//! hotkey and transfer limits. Kept in `device.json` next to `profiles.json`, so switching
//! profile doesn't change how the window or the hotkey behave.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use hopr_core::AppError;
use serde::{Deserialize, Serialize};

const FILE: &str = "device.json";

/// Unset values fall back to the defaults (or, once, to what the open profile had saved).
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSettings {
    pub close_to_tray: Option<bool>,
    pub hotkey: Option<String>,
    pub transfer_concurrency: Option<u32>,
    pub transfer_limit_kbps: Option<u32>,
}

pub struct Device {
    root: PathBuf,
    settings: Mutex<DeviceSettings>,
}

impl Device {
    /// Read `device.json` in the data root; missing or unreadable means all defaults.
    pub fn open(root: PathBuf) -> Self {
        let settings = load(&root);
        Self {
            root,
            settings: Mutex::new(settings),
        }
    }

    pub fn get(&self) -> DeviceSettings {
        self.settings.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Change the settings and write them at once.
    pub fn update(&self, f: impl FnOnce(&mut DeviceSettings)) -> Result<(), AppError> {
        let next = {
            let mut s = self
                .settings
                .lock()
                .map_err(|e| AppError::internal("Couldn't save this device's settings", e))?;
            f(&mut s);
            s.clone()
        };
        save(&self.root, &next)
    }
}

fn load(root: &Path) -> DeviceSettings {
    std::fs::read(root.join(FILE))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Write atomically (temp file, then rename).
fn save(root: &Path, s: &DeviceSettings) -> Result<(), AppError> {
    let fail =
        |e: &dyn std::fmt::Display| AppError::internal("Couldn't save this device's settings", e);
    std::fs::create_dir_all(root).map_err(|e| fail(&e))?;
    let tmp = root.join(format!("{FILE}.tmp"));
    let json = serde_json::to_vec_pretty(s).map_err(|e| fail(&e))?;
    std::fs::write(&tmp, json).map_err(|e| fail(&e))?;
    std::fs::rename(&tmp, root.join(FILE)).map_err(|e| fail(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hopr-device-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn starts_empty_and_round_trips() {
        let root = temp();
        let device = Device::open(root.clone());
        assert_eq!(device.get(), DeviceSettings::default());
        device
            .update(|s| {
                s.close_to_tray = Some(false);
                s.hotkey = Some("Ctrl+Alt+T".into());
            })
            .unwrap();
        let again = Device::open(root);
        assert_eq!(again.get().close_to_tray, Some(false));
        assert_eq!(again.get().hotkey.as_deref(), Some("Ctrl+Alt+T"));
    }

    #[test]
    fn a_broken_file_means_defaults() {
        let root = temp();
        std::fs::write(root.join(FILE), b"{not json").unwrap();
        assert_eq!(Device::open(root).get(), DeviceSettings::default());
    }
}
