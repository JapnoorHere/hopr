//! Global hotkey notation: the app's chord strings ("Ctrl+Shift+`") to the accelerator
//! syntax the OS-level shortcut registry parses ("Control+Shift+Backquote").

/// Convert a chord, or `None` if it can't be a global hotkey. A bare key is refused:
/// registered system-wide it would swallow normal typing.
pub fn to_accelerator(chord: &str) -> Option<String> {
    let tokens: Vec<&str> = chord.split('+').map(str::trim).collect();
    let (key, mods) = tokens.split_last()?;
    if mods.is_empty() {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    for m in mods {
        parts.push(
            match m.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => "Control",
                "alt" | "option" => "Alt",
                "shift" => "Shift",
                "meta" | "cmd" | "super" | "win" => "Super",
                _ => return None,
            }
            .to_owned(),
        );
    }
    let k = match *key {
        "`" => "Backquote".to_owned(),
        "Space" => "Space".to_owned(),
        k if k.len() == 1 && k.chars().all(|c| c.is_ascii_alphabetic()) => {
            format!("Key{}", k.to_ascii_uppercase())
        }
        k if k.len() == 1 && k.chars().all(|c| c.is_ascii_digit()) => format!("Digit{k}"),
        k if k.starts_with('F') && k[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n)) => {
            k.to_owned()
        }
        _ => return None,
    };
    parts.push(k);
    Some(parts.join("+"))
}

#[cfg(test)]
mod tests {
    use super::to_accelerator;

    #[test]
    fn chords_map_to_accelerators() {
        assert_eq!(
            to_accelerator("Ctrl+`").as_deref(),
            Some("Control+Backquote")
        );
        assert_eq!(
            to_accelerator("Ctrl+Shift+t").as_deref(),
            Some("Control+Shift+KeyT")
        );
        assert_eq!(to_accelerator("Alt+F12").as_deref(), Some("Alt+F12"));
        assert_eq!(to_accelerator("Ctrl+1").as_deref(), Some("Control+Digit1"));
        assert_eq!(to_accelerator("T"), None, "a bare key would swallow typing");
        assert_eq!(to_accelerator("Hyper+T"), None);
        assert_eq!(to_accelerator("Ctrl+F99"), None);
    }
}
