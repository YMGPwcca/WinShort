from pathlib import Path


def replace_once(path, old, new, label):
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected 1 match in {path}, got {count}")
    p.write_text(text.replace(old, new, 1), encoding="utf-8")


# Both Special Workspace actions make the runtime workspace meaningful. Keeping
# only the send/move hotkey must not make an unrelated config publication tear
# the workspace down.
replace_once(
    "src/app.rs",
    """        service.configure_scratchpad(
            config.virtual_desktops.enabled && config.virtual_desktops.scratchpad_toggle.is_some(),
        );
""",
    """        service.configure_scratchpad(
            config.virtual_desktops.enabled
                && (config.virtual_desktops.scratchpad_assign.is_some()
                    || config.virtual_desktops.scratchpad_toggle.is_some()),
        );
""",
    "install managed policy",
)
replace_once(
    "src/app.rs",
    """                    desktop.configure_scratchpad(
                        config.virtual_desktops.enabled
                            && config.virtual_desktops.scratchpad_toggle.is_some(),
                    );
""",
    """                    desktop.configure_scratchpad(
                        config.virtual_desktops.enabled
                            && (config.virtual_desktops.scratchpad_assign.is_some()
                                || config.virtual_desktops.scratchpad_toggle.is_some()),
                    );
""",
    "config managed policy",
)
replace_once(
    "src/app.rs",
    '            self.report_desktop_failure("assign scratchpad", "desktop subsystem unavailable");\n',
    '            self.report_desktop_failure(\n                "send to special workspace",\n                "desktop subsystem unavailable",\n            );\n',
    "assign unavailable wording",
)
replace_once(
    "src/app.rs",
    '                "assign scratchpad",\n                "no eligible foreground application window",\n',
    '                "send to special workspace",\n                "no eligible foreground application window",\n',
    "assign eligibility wording",
)
replace_once(
    "src/app.rs",
    '            self.report_desktop_failure("toggle scratchpad", "desktop subsystem unavailable");\n',
    '            self.report_desktop_failure(\n                "toggle special workspace",\n                "desktop subsystem unavailable",\n            );\n',
    "toggle unavailable wording",
)

# Keep missing-desktop arithmetic explicit and tested after removing the old
# indexed InternalBackend helper surface.
replace_once(
    "src/desktop/service.rs",
    """        let missing = target_count.saturating_sub(current.len());
        for _ in 0..missing {
""",
    """        let missing = missing_normal_desktops(current.len(), target_count);
        for _ in 0..missing {
""",
    "use missing-normal helper",
)
replace_once(
    "src/desktop/service.rs",
    """fn numbered_desktop_ids(ids: &[GUID], special_workspace: Option<GUID>) -> Vec<GUID> {
""",
    """fn missing_normal_desktops(current_count: usize, target_count: usize) -> usize {
    target_count.saturating_sub(current_count)
}

fn numbered_desktop_ids(ids: &[GUID], special_workspace: Option<GUID>) -> Vec<GUID> {
""",
    "insert missing-normal helper",
)
replace_once(
    "src/desktop/service.rs",
    """    #[test]
    fn special_workspace_is_excluded_from_numbered_desktop_ordinals() {
""",
    """    #[test]
    fn normal_desktop_creation_counts_only_the_missing_target() {
        assert_eq!(missing_normal_desktops(3, 9), 6);
        assert_eq!(missing_normal_desktops(9, 9), 0);
        assert_eq!(missing_normal_desktops(10, 9), 0);
    }

    #[test]
    fn special_workspace_is_excluded_from_numbered_desktop_ordinals() {
""",
    "add missing-normal test",
)

print("special workspace review fixes staged")
