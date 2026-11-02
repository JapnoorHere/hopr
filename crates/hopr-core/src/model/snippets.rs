//! Saved commands and how they run.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::AppError;

use super::*;

/// What clicking a snippet does (§9.12).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    /// Type it at the prompt, without Enter.
    #[default]
    Paste,
    /// Type it and press Enter.
    PasteEnter,
    /// Show it, run on confirmation.
    Confirm,
}

impl RunMode {
    /// Stable database value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Paste => "paste",
            Self::PasteEnter => "paste_enter",
            Self::Confirm => "confirm",
        }
    }

    /// Parse a database value.
    pub fn parse(s: &str) -> Self {
        match s {
            "paste_enter" => Self::PasteEnter,
            "confirm" => Self::Confirm,
            _ => Self::Paste,
        }
    }
}

/// A saved command or script with optional `{{variables}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    /// Id.
    pub id: String,
    /// Name.
    pub name: String,
    /// Folder path ("Ops/Postgres"); "" for the top level.
    pub folder: String,
    /// Tags.
    pub tags: Vec<String>,
    /// What it does.
    pub description: String,
    /// The text, with `{{name}}`, `{{name:default}}`, `{{name:a|b|c}}`, `{{secret:x}}`
    /// and built-ins `{{host}} {{user}} {{date}} {{clipboard}}`.
    pub body: String,
    /// Paste, paste + Enter, or confirm first.
    pub run_mode: RunMode,
    /// Updated (RFC 3339).
    pub updated_at: String,
}

/// Create/update payload for a snippet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SnippetInput {
    /// Name.
    pub name: String,
    /// Folder path.
    pub folder: String,
    /// Tags.
    pub tags: Vec<String>,
    /// Description.
    pub description: String,
    /// Body.
    pub body: String,
    /// Run mode.
    pub run_mode: RunMode,
}

impl SnippetInput {
    /// Validate and normalize.
    pub fn normalized(mut self) -> Result<Self, AppError> {
        self.name = self.name.trim().to_owned();
        self.folder = self
            .folder
            .split('/')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join("/");
        if self.name.is_empty() || self.name.len() > 120 {
            return Err(AppError::invalid(
                "Give the snippet a name (up to 120 characters)",
            ));
        }
        if self.body.trim().is_empty() {
            return Err(AppError::invalid("The snippet is empty"));
        }
        if self.body.len() > 64 * 1024 || self.description.len() > 2000 || self.folder.len() > 200 {
            return Err(AppError::invalid("The snippet is too long"));
        }
        self.tags = normalize_tags(&self.tags);
        Ok(self)
    }
}
