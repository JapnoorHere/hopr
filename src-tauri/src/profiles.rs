//! Profiles: fully separate data folders, each with its own database and vault password.
//!
//! `profiles.json` in the data root lists them. The original data (the root folder itself)
//! is the profile `default`, so nothing moves; new profiles live in `profiles/<id>/`.
//! Folder names come from random ids, never from the name people type.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use hopr_core::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

pub const REGISTRY: &str = "profiles.json";
/// The profile whose data is the data root (everything from before profiles existed).
pub const DEFAULT_ID: &str = "default";
const NAME_MAX: usize = 40;

/// A profile's colours, so the picker can draw it in its own theme before it is opened.
#[derive(Serialize, Deserialize, Clone, Type, Debug, PartialEq)]
pub struct ProfileLook {
    pub bg: String,
    pub text: String,
    pub accent: String,
}

#[derive(Serialize, Deserialize, Clone, Type, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    /// What the person called it. Empty for the original data until they name it.
    pub name: String,
    /// #rrggbb, the avatar colour.
    pub color: String,
    /// Unix seconds.
    #[specta(type = u32)]
    pub created_at: u64,
    pub look: Option<ProfileLook>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Registry {
    pub profiles: Vec<Profile>,
    pub last: String,
    /// Show the profile picker every time Hopr opens.
    #[serde(default = "yes")]
    pub ask_at_start: bool,
    /// Set by a switch: the restart that follows opens the chosen profile without asking again.
    #[serde(default)]
    pub skip_ask_once: bool,
}

fn yes() -> bool {
    true
}

/// What the UI sees.
#[derive(Serialize, Clone, Type, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProfilesView {
    pub profiles: Vec<Profile>,
    /// The open profile.
    pub current: String,
    pub ask_at_start: bool,
    /// Show the picker now (this launch).
    pub ask_now: bool,
}

/// Managed state: where the data root is, which profile is open, and the registry.
pub struct Profiles {
    root: PathBuf,
    current: String,
    ask_now: bool,
    registry: Mutex<Registry>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn invalid(msg: &str) -> AppError {
    AppError::new(ErrorCode::InvalidInput, msg)
}

fn io(e: std::io::Error) -> AppError {
    AppError::new(ErrorCode::Storage, format!("Profiles: {e}"))
}

fn original(created_at: u64) -> Profile {
    Profile {
        id: DEFAULT_ID.to_owned(),
        name: String::new(),
        color: "#7aa2f7".to_owned(),
        created_at,
        look: None,
    }
}

/// A profile id is `default` or a v4 UUID: safe to use as a folder name.
fn valid_id(id: &str) -> bool {
    id == DEFAULT_ID || uuid::Uuid::parse_str(id).is_ok_and(|u| u.hyphenated().to_string() == id)
}

fn valid_color(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit())
}

impl Registry {
    fn fresh() -> Self {
        Self {
            profiles: vec![original(now())],
            last: DEFAULT_ID.to_owned(),
            ask_at_start: true,
            skip_ask_once: false,
        }
    }

    /// Drop anything malformed and make sure the original data is always listed.
    fn repaired(mut self) -> Self {
        self.profiles.retain(|p| valid_id(&p.id) && valid_color(&p.color));
        let mut seen = std::collections::HashSet::new();
        self.profiles.retain(|p| seen.insert(p.id.clone()));
        if !self.profiles.iter().any(|p| p.id == DEFAULT_ID) {
            self.profiles.insert(0, original(now()));
        }
        if !self.profiles.iter().any(|p| p.id == self.last) {
            self.last = DEFAULT_ID.to_owned();
        }
        self
    }

    pub fn get(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    /// A trimmed, unique (ignoring case) name of 1–40 characters.
    pub fn check_name(&self, name: &str, except: Option<&str>) -> Result<String, AppError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(invalid("Give the profile a name"));
        }
        if name.chars().count() > NAME_MAX {
            return Err(invalid("Keep the name to 40 characters or fewer"));
        }
        let taken = self
            .profiles
            .iter()
            .any(|p| Some(p.id.as_str()) != except && p.name.to_lowercase() == name.to_lowercase());
        if taken {
            return Err(invalid("Another profile already has that name"));
        }
        Ok(name.to_owned())
    }

    pub fn create(&mut self, name: &str, color: &str) -> Result<Profile, AppError> {
        let name = self.check_name(name, None)?;
        if !valid_color(color) {
            return Err(invalid("Pick a colour"));
        }
        let p = Profile {
            id: uuid::Uuid::new_v4().hyphenated().to_string(),
            name,
            color: color.to_owned(),
            created_at: now(),
            look: None,
        };
        self.profiles.push(p.clone());
        Ok(p)
    }

    pub fn update(
        &mut self,
        id: &str,
        name: Option<&str>,
        color: Option<&str>,
    ) -> Result<Profile, AppError> {
        let name = name.map(|n| self.check_name(n, Some(id))).transpose()?;
        if color.is_some_and(|c| !valid_color(c)) {
            return Err(invalid("Pick a colour"));
        }
        let p = self
            .profiles
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| invalid("That profile no longer exists"))?;
        if let Some(n) = name {
            p.name = n;
        }
        if let Some(c) = color {
            p.color = c.to_owned();
        }
        Ok(p.clone())
    }

    /// Remove a profile from the list. The open one and the only one can't go.
    pub fn remove(&mut self, id: &str, current: &str) -> Result<(), AppError> {
        if id == current {
            return Err(invalid("Switch to another profile before deleting this one"));
        }
        if self.profiles.len() <= 1 {
            return Err(invalid("Hopr needs at least one profile"));
        }
        let before = self.profiles.len();
        self.profiles.retain(|p| p.id != id);
        if self.profiles.len() == before {
            return Err(invalid("That profile no longer exists"));
        }
        if self.last == id {
            self.last = current.to_owned();
        }
        Ok(())
    }
}

/// Read the registry. Missing or unreadable: just the original data, so nothing is lost.
pub fn load(root: &Path) -> Registry {
    std::fs::read(root.join(REGISTRY))
        .ok()
        .and_then(|b| serde_json::from_slice::<Registry>(&b).ok())
        .map_or_else(Registry::fresh, Registry::repaired)
}

/// Write the registry atomically (temp file, then rename).
pub fn save(root: &Path, reg: &Registry) -> Result<(), AppError> {
    std::fs::create_dir_all(root).map_err(io)?;
    let tmp = root.join(format!("{REGISTRY}.tmp"));
    let json = serde_json::to_vec_pretty(reg)
        .map_err(|e| AppError::new(ErrorCode::Internal, e.to_string()))?;
    std::fs::write(&tmp, json).map_err(io)?;
    std::fs::rename(&tmp, root.join(REGISTRY)).map_err(io)
}

/// A profile's data folder.
pub fn dir(root: &Path, id: &str) -> PathBuf {
    if id == DEFAULT_ID {
        root.to_path_buf()
    } else {
        root.join("profiles").join(id)
    }
}

/// Delete a profile's data. The original profile's folder is the data root, so only its own
/// files go (never `profiles/` or the registry).
fn delete_data(root: &Path, id: &str) -> Result<(), AppError> {
    let d = dir(root, id);
    if id != DEFAULT_ID {
        return match std::fs::remove_dir_all(&d) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(io(e)),
            _ => Ok(()),
        };
    }
    for f in ["hopr.db", "hopr.db-wal", "hopr.db-shm"] {
        let _ = std::fs::remove_file(d.join(f));
    }
    for sub in ["backups", "recordings"] {
        let _ = std::fs::remove_dir_all(d.join(sub));
    }
    Ok(())
}

impl Profiles {
    /// Pick the profile to open: the last used one. Consumes a pending "skip asking".
    pub fn open(root: PathBuf) -> Self {
        let mut reg = load(&root);
        let ask_now = reg.ask_at_start && !reg.skip_ask_once;
        if reg.skip_ask_once {
            reg.skip_ask_once = false;
        }
        let _ = save(&root, &reg);
        Self {
            current: reg.last.clone(),
            root,
            ask_now,
            registry: Mutex::new(reg),
        }
    }

    pub fn current(&self) -> &str {
        &self.current
    }

    pub fn data_dir(&self) -> PathBuf {
        dir(&self.root, &self.current)
    }

    fn view(&self, reg: &Registry) -> ProfilesView {
        ProfilesView {
            profiles: reg.profiles.clone(),
            current: self.current.clone(),
            ask_at_start: reg.ask_at_start,
            ask_now: self.ask_now,
        }
    }

    /// Change the registry and save it; nothing is kept if saving fails.
    fn edit<T>(
        &self,
        f: impl FnOnce(&mut Registry) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut reg = self.registry.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = reg.clone();
        let out = f(&mut next)?;
        save(&self.root, &next)?;
        *reg = next;
        Ok(out)
    }
}

type S<'a> = State<'a, Profiles>;

/// Every profile, the open one, and whether to show the picker now.
#[tauri::command]
#[specta::specta]
pub fn profiles_get(state: S<'_>) -> ProfilesView {
    let reg = state.registry.lock().unwrap_or_else(|e| e.into_inner());
    state.view(&reg)
}

/// A new, empty profile (its vault is set up the first time it opens).
#[tauri::command]
#[specta::specta]
pub fn profile_create(state: S<'_>, name: String, color: String) -> Result<Profile, AppError> {
    let p = state.edit(|r| r.create(&name, &color))?;
    std::fs::create_dir_all(dir(&state.root, &p.id)).map_err(io)?;
    Ok(p)
}

/// Rename a profile or change its colour.
#[tauri::command]
#[specta::specta]
pub fn profile_update(
    state: S<'_>,
    id: String,
    name: Option<String>,
    color: Option<String>,
) -> Result<Profile, AppError> {
    state.edit(|r| r.update(&id, name.as_deref(), color.as_deref()))
}

/// Remember the open profile's theme colours for the picker.
#[tauri::command]
#[specta::specta]
pub fn profile_set_look(state: S<'_>, look: ProfileLook) -> Result<(), AppError> {
    if ![&look.bg, &look.text, &look.accent].iter().all(|c| valid_color(c)) {
        return Err(invalid("Colours must be #rrggbb"));
    }
    let current = state.current.clone();
    state.edit(|r| {
        if let Some(p) = r.profiles.iter_mut().find(|p| p.id == current) {
            p.look = Some(look);
        }
        Ok(())
    })
}

/// Show the picker every time Hopr opens, or open the last profile straight away.
#[tauri::command]
#[specta::specta]
pub fn profiles_set_ask_at_start(state: S<'_>, ask: bool) -> Result<(), AppError> {
    state.edit(|r| {
        r.ask_at_start = ask;
        Ok(())
    })
}

/// Delete a profile and all its data (hosts, keys, history, backups). Not the open one.
#[tauri::command]
#[specta::specta]
pub fn profile_delete(state: S<'_>, id: String) -> Result<(), AppError> {
    let current = state.current.clone();
    state.edit(|r| r.remove(&id, &current))?;
    crate::state::keychain::forget_for(&id);
    delete_data(&state.root, &id)
}

/// Open another profile: Hopr restarts into it (without asking again).
#[tauri::command]
#[specta::specta]
pub fn profile_switch(app: tauri::AppHandle, state: S<'_>, id: String) -> Result<(), AppError> {
    state.edit(|r| {
        if r.get(&id).is_none() {
            return Err(invalid("That profile no longer exists"));
        }
        r.last = id.clone();
        r.skip_ask_once = true;
        Ok(())
    })?;
    app.restart()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("hopr-profiles-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn missing_or_broken_registry_keeps_the_original_data() {
        let root = temp();
        let reg = load(&root);
        assert_eq!(reg.profiles.len(), 1);
        assert_eq!(reg.last, DEFAULT_ID);
        assert_eq!(dir(&root, DEFAULT_ID), root);
        std::fs::write(root.join(REGISTRY), b"{not json").unwrap();
        assert_eq!(load(&root).profiles[0].id, DEFAULT_ID);
    }

    #[test]
    fn names_are_required_unique_and_short() {
        let mut reg = Registry::fresh();
        assert!(reg.create("", "#112233").is_err());
        let a = reg.create("  Side projects ", "#112233").unwrap();
        assert_eq!(a.name, "Side projects");
        assert!(reg.create("side PROJECTS", "#112233").is_err());
        assert!(reg.create(&"x".repeat(41), "#112233").is_err());
        assert!(reg.create("Ok", "red").is_err());
        // Renaming to its own name is fine.
        assert!(reg.update(&a.id, Some("Side projects"), None).is_ok());
    }

    #[test]
    fn ids_are_safe_folder_names() {
        let mut reg = Registry::fresh();
        let p = reg.create("A", "#112233").unwrap();
        assert!(valid_id(&p.id));
        assert!(!valid_id("../evil"));
        assert!(!valid_id("C:\\x"));
        let root = PathBuf::from("data");
        assert_eq!(dir(&root, &p.id), root.join("profiles").join(&p.id));
    }

    #[test]
    fn the_open_and_the_only_profile_cannot_be_deleted() {
        let mut reg = Registry::fresh();
        assert!(reg.remove(DEFAULT_ID, "other").is_err());
        let p = reg.create("B", "#112233").unwrap();
        assert!(reg.remove(&p.id, &p.id).is_err());
        reg.last = p.id.clone();
        reg.remove(&p.id, DEFAULT_ID).unwrap();
        assert_eq!(reg.last, DEFAULT_ID);
        assert!(reg.remove("nope", DEFAULT_ID).is_err());
    }

    #[test]
    fn repair_drops_bad_entries_and_restores_the_original() {
        let root = temp();
        std::fs::write(
            root.join(REGISTRY),
            r##"{"profiles":[{"id":"../x","name":"Bad","color":"#112233","createdAt":1,"look":null}],"last":"../x"}"##,
        )
        .unwrap();
        let reg = load(&root);
        assert_eq!(reg.profiles.len(), 1);
        assert_eq!(reg.profiles[0].id, DEFAULT_ID);
        assert_eq!(reg.last, DEFAULT_ID);
        assert!(reg.ask_at_start);
    }

    #[test]
    fn a_switch_skips_the_picker_once() {
        let root = temp();
        let mut reg = Registry::fresh();
        let p = reg.create("C", "#112233").unwrap();
        reg.last = p.id.clone();
        reg.skip_ask_once = true;
        save(&root, &reg).unwrap();
        let opened = Profiles::open(root.clone());
        assert_eq!(opened.current(), p.id);
        assert!(!opened.ask_now);
        assert!(Profiles::open(root).ask_now);
    }

    #[test]
    fn deleting_the_original_keeps_other_profiles() {
        let root = temp();
        std::fs::write(root.join("hopr.db"), b"x").unwrap();
        let other = dir(&root, &uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&other).unwrap();
        delete_data(&root, DEFAULT_ID).unwrap();
        assert!(!root.join("hopr.db").exists());
        assert!(other.exists());
    }
}
