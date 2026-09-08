//! Hotkeys for the presentation.

use crate::keyboard::binding::{Hotkey, ModifierMask};

pub(crate) fn format_modifier(modifier: ModifierMask) -> String {
    modifier
        .parts()
        .into_iter()
        .filter(|(_, part)| modifier.contains(*part))
        .map(|(name, _)| name)
        .collect::<Vec<_>>()
        .join(" + ")
}

pub(crate) fn format_desktop_modifier(modifier: ModifierMask) -> String {
    let formatted = format_modifier(modifier);
    if formatted.is_empty() {
        "Choose a modifier".into()
    } else {
        format!("{formatted} + 1–9")
    }
}

pub(crate) fn format_hotkey(hotkey: Hotkey) -> String {
    let key = friendly_key_name(&hotkey.key.name());
    let modifier = format_modifier(hotkey.modifiers);
    if modifier.is_empty() {
        key
    } else {
        format!("{modifier} + {key}")
    }
}

pub(crate) fn format_optional_hotkey(hotkey: Option<Hotkey>) -> String {
    hotkey.map_or_else(|| "Not assigned".into(), format_hotkey)
}

fn friendly_key_name(name: &str) -> String {
    match name {
        "Left" => "←".into(),
        "Up" => "↑".into(),
        "Right" => "→".into(),
        "Down" => "↓".into(),
        "PageUp" => "Page Up".into(),
        "PageDown" => "Page Down".into(),
        "CapsLock" => "Caps Lock".into(),
        "NumLock" => "Num Lock".into(),
        "ScrollLock" => "Scroll Lock".into(),
        value if value.starts_with("Numpad") => format!("Num {}", &value[6..]),
        value if value.starts_with("Oem") => value
            .strip_prefix("Oem")
            .unwrap_or(value)
            .replace("OpenBrackets", "[")
            .replace("CloseBrackets", "]")
            .replace("Semicolon", ";")
            .replace("Comma", ",")
            .replace("Period", ".")
            .replace("Slash", "/")
            .replace("Tilde", "`")
            .replace("Pipe", "\\")
            .replace("Quotes", "'"),
        value => value.to_string(),
    }
}
