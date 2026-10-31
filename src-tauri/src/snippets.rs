//! Snippets (§9.12, R1) and secret injection (R10). `{{secret:name}}` is filled in here,
//! right before the text is typed into the session, so a secret never reaches the UI.

use std::sync::Arc;

use hopr_core::AppError;
use hopr_core::model::{Snippet, SnippetInput};
use hopr_store::snippets::valid_secret_name;
use secrecy::ExposeSecret;
use tauri::State;
use zeroize::Zeroizing;

use crate::sessions::SessionManager;
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub async fn snippets_list(state: State<'_, Arc<AppState>>) -> Result<Vec<Snippet>, AppError> {
    state.require_unlocked()?;
    Ok(state.store.list_snippets().await?)
}

#[tauri::command]
#[specta::specta]
pub async fn snippet_save(
    state: State<'_, Arc<AppState>>,
    id: Option<String>,
    input: SnippetInput,
) -> Result<Snippet, AppError> {
    state.require_unlocked()?;
    Ok(state
        .store
        .save_snippet(id.as_deref(), &input.normalized()?)
        .await?)
}

#[tauri::command]
#[specta::specta]
pub async fn snippet_delete(state: State<'_, Arc<AppState>>, id: String) -> Result<(), AppError> {
    state.require_unlocked()?;
    Ok(state.store.delete_snippet(&id).await?)
}

/// Names of the secrets snippets can use (values never leave the backend).
#[tauri::command]
#[specta::specta]
pub async fn secrets_list(state: State<'_, Arc<AppState>>) -> Result<Vec<String>, AppError> {
    state.require_unlocked()?;
    Ok(state.store.secret_names().await?)
}

#[tauri::command]
#[specta::specta]
pub async fn secret_set(
    state: State<'_, Arc<AppState>>,
    name: String,
    value: String,
) -> Result<(), AppError> {
    let value = Zeroizing::new(value);
    let vault = state.vault()?;
    let name = name.trim().to_owned();
    if !valid_secret_name(&name) {
        return Err(AppError::invalid(
            "Secret names use letters, digits, '_', '-' and '.' (up to 64)",
        ));
    }
    if value.is_empty() || value.len() > 8192 {
        return Err(AppError::invalid("Enter a value (up to 8 KB)"));
    }
    state.store.set_secret(&vault, &name, &value).await?;
    state.audit("secret.set", None, format!("Saved snippet secret {name}"));
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn secret_delete(state: State<'_, Arc<AppState>>, name: String) -> Result<(), AppError> {
    state.require_unlocked()?;
    state.store.delete_secret(&name).await?;
    state.audit(
        "secret.delete",
        None,
        format!("Removed snippet secret {name}"),
    );
    Ok(())
}

/// The names in `{{secret:name}}` placeholders, in order.
fn secret_refs(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("{{secret:") {
        let after = &rest[i + "{{secret:".len()..];
        let Some(end) = after.find("}}") else { break };
        out.push(after[..end].trim());
        rest = &after[end + 2..];
    }
    out
}

/// Replace every `{{secret:name}}` (spacing inside the braces allowed) with `value`.
fn fill(text: &str, name: &str, value: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("{{secret:") {
        let after = &rest[i + "{{secret:".len()..];
        let Some(end) = after.find("}}") else { break };
        out.push_str(&rest[..i]);
        if after[..end].trim() == name {
            out.push_str(value);
        } else {
            out.push_str(&rest[i..i + "{{secret:".len() + end + 2]);
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// Type `text` into a session, with `{{secret:name}}` filled in from the vault; `enter`
/// presses Enter after it. Returns how many secrets were used.
#[tauri::command]
#[specta::specta]
pub async fn session_type(
    state: State<'_, Arc<AppState>>,
    manager: State<'_, SessionManager>,
    id: String,
    text: String,
    enter: bool,
) -> Result<u32, AppError> {
    if text.len() > 64 * 1024 {
        return Err(AppError::invalid("That is too much text to type at once"));
    }
    let names: Vec<String> = secret_refs(&text).into_iter().map(str::to_owned).collect();
    let mut filled = Zeroizing::new(text);
    if !names.is_empty() {
        let vault = state.vault()?;
        for name in &names {
            let value = state.store.get_secret(&vault, name).await?.ok_or_else(|| {
                AppError::invalid(format!(
                    "No secret named \"{name}\"; add it under Snippets → Secrets"
                ))
            })?;
            *filled = fill(&filled, name, value.expose_secret());
        }
    }
    let mut bytes = Zeroizing::new(filled.as_bytes().to_vec());
    if enter {
        bytes.push(b'\r');
    }
    manager.write(&id, bytes.to_vec())?;
    Ok(u32::try_from(names.len()).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    use super::{fill, secret_refs};

    #[test]
    fn finds_and_fills_secret_placeholders() {
        let text = "mysql -p{{secret:db}} -u {{user}} {{secret: api.key }} {{secret:db}}";
        assert_eq!(secret_refs(text), ["db", "api.key", "db"]);
        assert_eq!(
            fill(text, "db", "pw"),
            "mysql -ppw -u {{user}} {{secret: api.key }} pw"
        );
        assert_eq!(
            fill(&fill(text, "db", "pw"), "api.key", "K"),
            "mysql -ppw -u {{user}} K pw"
        );
        assert!(secret_refs("echo {{secret:unterminated").is_empty());
        assert_eq!(fill("echo {{secret:x", "x", "v"), "echo {{secret:x");
    }
}
