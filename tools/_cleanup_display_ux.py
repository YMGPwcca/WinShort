from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def replace_once(text, old, new, label):
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, got {count}")
    return text.replace(old, new, 1)


# Remove the old picker-only topology constant; the normal UI now deliberately
# exposes only Extend / Duplicate when multiple outputs are selected.
path = ROOT / "src/display/mod.rs"
text = path.read_text(encoding="utf-8")
old = '''    pub const ALL: [Self; 5] = [
        Self::Internal,
        Self::Clone,
        Self::Extend,
        Self::External,
        Self::Custom,
    ];

'''
text = replace_once(text, old, "", "DisplayTopology::ALL")

# A newly selected inactive output is allowed to have *no* captured mode yet.
# A partially populated mode is still malformed and must never become valid just
# because the profile is unconfirmed.
needle = '''pub fn route_has_complete_mode(route: &DisplayRoute) -> bool {
    route.source_width > 0
        && route.source_height > 0
        && route.active_width > 0
        && route.active_height > 0
        && route.refresh_numerator > 0
        && route.refresh_denominator > 0
        && matches!(route.rotation, 1..=4)
}

'''
replacement = needle + '''pub fn route_has_any_mode(route: &DisplayRoute) -> bool {
    route.source_width != 0
        || route.source_height != 0
        || route.active_width != 0
        || route.active_height != 0
        || route.refresh_numerator != 0
        || route.refresh_denominator != 0
        || route.rotation != 0
}

'''
text = replace_once(text, needle, replacement, "route mode helpers")

old = '''        // New/edited profiles may intentionally contain an inactive output.
        // Such a route has identity only until Test Apply lets Windows choose a
        // valid mode and WinShort captures it back. Confirmed profiles remain
        // strict because normal hotkey activation must be deterministic.
        if profile.confirmed && !route_has_complete_mode(route) {
            return Err(Error::config(
                "confirmed display profile contains an unresolved output mode",
            ));
        }
    }
    if profile.confirmed && profile.routes.len() > 1 {
'''
new = '''        // New/edited profiles may intentionally contain an inactive output.
        // Such a route has identity only until Test Apply lets Windows choose a
        // valid mode and WinShort captures it back. A partially populated mode,
        // however, is malformed even before confirmation.
        if route_has_any_mode(route) && !route_has_complete_mode(route) {
            return Err(Error::config(
                "display profile contains a partially resolved output mode",
            ));
        }
        if profile.confirmed && !route_has_complete_mode(route) {
            return Err(Error::config(
                "confirmed display profile contains an unresolved output mode",
            ));
        }
    }
    if profile.routes.len() > 1 && profile.routes.iter().all(route_has_complete_mode) {
'''
text = replace_once(text, old, new, "display profile mode/topology validation")
path.write_text(text, encoding="utf-8")

# Mirror the same rule at the top-level config validator. Identity-only draft
# outputs are legal; partial modes are not. Fully resolved drafts still receive
# clone/extend shape validation immediately.
path = ROOT / "src/config/validate.rs"
text = path.read_text(encoding="utf-8")
old = '''            if profile.confirmed && !crate::display::route_has_complete_mode(route) {
                v.push(Violation::new(
                    &format!("{field}.routes[{route_index}].mode"),
                    "confirmed display output must have a resolved mode",
                ));
            }
'''
new = '''            if crate::display::route_has_any_mode(route)
                && !crate::display::route_has_complete_mode(route)
            {
                v.push(Violation::new(
                    &format!("{field}.routes[{route_index}].mode"),
                    "display output mode must be either unresolved or complete",
                ));
            } else if profile.confirmed && !crate::display::route_has_complete_mode(route) {
                v.push(Violation::new(
                    &format!("{field}.routes[{route_index}].mode"),
                    "confirmed display output must have a resolved mode",
                ));
            }
'''
text = replace_once(text, old, new, "config route mode validation")
text = replace_once(
    text,
    "        if profile.confirmed && profile.routes.len() > 1 {\n",
    "        if profile.routes.len() > 1\n            && profile\n                .routes\n                .iter()\n                .all(crate::display::route_has_complete_mode)\n        {\n",
    "config topology validation",
)
path.write_text(text, encoding="utf-8")

# Repair must not preserve a half-populated unconfirmed mode. It may preserve an
# identity-only inactive output so the user can test-activate it later.
path = ROOT / "src/config/model.rs"
text = path.read_text(encoding="utf-8")
old = '''                        !route.target_path.trim().is_empty()
                            && (!profile.confirmed
                                || crate::display::route_has_complete_mode(route))
                            && seen_routes.insert(format!(
'''
new = '''                        !route.target_path.trim().is_empty()
                            && (!crate::display::route_has_any_mode(route)
                                || crate::display::route_has_complete_mode(route))
                            && (!profile.confirmed
                                || crate::display::route_has_complete_mode(route))
                            && seen_routes.insert(format!(
'''
text = replace_once(text, old, new, "config repair draft route modes")
path.write_text(text, encoding="utf-8")

print("cleaned generated display UX and preserved strict draft validation")
