from pathlib import Path
import re


def sub_once(text, pattern, repl, label):
    out, count = re.subn(pattern, repl, text, count=1, flags=re.S)
    if count != 1:
        raise SystemExit(f"{label}: expected 1 match, got {count}")
    return out


# The dedicated-workspace rewrite uses GUID-addressed operations directly.
# Remove superseded index/count helpers instead of carrying dead API surface.
p = Path("src/desktop/internal_api.rs")
s = p.read_text(encoding="utf-8")
s = sub_once(
    s,
    r"\nfn desktops_to_create\(.*?(?=\nimpl InternalBackend \{)",
    "\n",
    "remove obsolete count helpers",
)
s = sub_once(
    s,
    r"\n    pub fn ensure_desktop_count\(.*?(?=\n    fn desktop_id_for_index)",
    "\n",
    "remove obsolete ensure_desktop_count",
)
s = sub_once(
    s,
    r"\n    fn desktop_id_for_index\(.*?(?=\n    pub fn window_desktop_id)",
    "\n",
    "remove obsolete desktop_id_for_index",
)
s = sub_once(
    s,
    r"\n    pub fn move_window_to_desktop\(.*?(?=\n\}\n\nimpl VirtualDesktopBackend)",
    "\n",
    "remove obsolete indexed HWND move",
)
s = sub_once(
    s,
    r"\n#\[cfg\(test\)\]\nmod tests \{.*\Z",
    "\n",
    "remove obsolete internal_api helper tests",
)
p.write_text(s, encoding="utf-8")

# Preserve the controller's one bounded Shell-proxy recovery attempt. Special
# workspace operations never fall back to injected keyboard navigation.
p = Path("src/desktop/service.rs")
s = p.read_text(encoding="utf-8")
old = r'''    fn native_desktop_ids(&mut self) -> std::result::Result<Vec<GUID>, DesktopError> {
        let ids = self
            .native
            .as_ref()
            .ok_or_else(|| self.native_unavailable_error("desktop identity"))?
            .desktop_ids()?;
        self.reconcile_special_workspace_ids(&ids);
        Ok(ids)
    }
'''
new = r'''    fn native_desktop_ids(&mut self) -> std::result::Result<Vec<GUID>, DesktopError> {
        let first = match self.native.as_ref() {
            Some(native) => native.desktop_ids(),
            None => Err(self.native_unavailable_error("desktop identity")),
        };
        let ids = match first {
            Ok(ids) => ids,
            Err(error) if error.permits_fallback() => {
                crate::warn_!(
                    "native desktop operation failed: {error}; rebuilding Shell proxy"
                );
                self.recreate_native()?;
                self.native
                    .as_ref()
                    .expect("native backend recreated")
                    .desktop_ids()?
            }
            Err(error) => return Err(error),
        };
        self.reconcile_special_workspace_ids(&ids);
        Ok(ids)
    }
'''
if s.count(old) != 1:
    raise SystemExit(f"native_desktop_ids: expected 1 match, got {s.count(old)}")
s = s.replace(old, new, 1)
p.write_text(s, encoding="utf-8")

print("obsolete helpers removed; native recovery retained")
