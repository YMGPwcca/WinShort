from pathlib import Path
import re


def replace_once(text, old, new, label):
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected 1 match, got {count}")
    return text.replace(old, new, 1)


def sub_once(text, pattern, repl, label):
    out, count = re.subn(pattern, repl, text, count=1, flags=re.S)
    if count != 1:
        raise SystemExit(f"{label}: expected 1 match, got {count}")
    return out

# --- internal_api.rs: GUID-addressed native operations for the dedicated workspace ---
p = Path("src/desktop/internal_api.rs")
s = p.read_text(encoding="utf-8")
anchor = "    pub fn ensure_desktop_count(\n"
insert = r'''    fn desktop_for_id(
        &self,
        id: GUID,
    ) -> std::result::Result<IVirtualDesktop, DesktopError> {
        let inner: crate::error::Result<Option<IVirtualDesktop>> = (|| unsafe {
            let array = desktop_array(&self.manager)?;
            let count = array
                .GetCount()
                .map_err(|e| Error::win("IObjectArray::GetCount", &e))?;
            for index in 0..count {
                let desktop: IVirtualDesktop = array
                    .GetAt(index)
                    .map_err(|e| Error::win("IObjectArray::GetAt", &e))?;
                if desktop_id(&desktop)? == id {
                    return Ok(Some(desktop));
                }
            }
            Ok(None)
        })();
        inner.classify()?.ok_or_else(|| {
            DesktopError::NavigationUnavailable(
                "desktop identity was not found in Shell ordering".into(),
            )
        })
    }

    pub fn create_desktop(&self) -> std::result::Result<GUID, DesktopError> {
        let inner: crate::error::Result<GUID> = (|| unsafe {
            let mut desktop = None;
            self.manager
                .create_desktop(&mut desktop)
                .ok()
                .map_err(|e| Error::win("IVirtualDesktopManagerInternal::CreateDesktop", &e))?;
            let desktop = desktop.ok_or_else(|| Error::desktop("CreateDesktop returned null"))?;
            desktop_id(&desktop)
        })();
        inner.map_err(|error| {
            DesktopError::CreationUnavailable(format!("CreateDesktop failed: {error}"))
        })
    }

    pub fn switch_to_id(&self, id: GUID) -> std::result::Result<(), DesktopError> {
        if self.current_desktop_id()? == id {
            return Ok(());
        }
        let desktop = self.desktop_for_id(id)?;
        unsafe {
            self.manager
                .switch_desktop(ComIn::new(&desktop))
                .ok()
                .map_err(|e| classify(&Error::win(
                    "IVirtualDesktopManagerInternal::SwitchDesktop",
                    &e,
                )))
        }
    }

    pub fn move_window_to_desktop_id(
        &self,
        hwnd: windows::Win32::Foundation::HWND,
        desktop_id: GUID,
    ) -> std::result::Result<(), DesktopError> {
        let Some(window_manager) = &self.window_manager else {
            return Err(DesktopError::MoveUnavailable(
                "public VirtualDesktopManager is unavailable".into(),
            ));
        };
        unsafe {
            window_manager
                .MoveWindowToDesktop(hwnd, &desktop_id)
                .map_err(|e| classify(&Error::win(
                    "IVirtualDesktopManager::MoveWindowToDesktop",
                    &e,
                )))
        }
    }

    pub fn remove_desktop_id(
        &self,
        id: GUID,
        fallback_id: GUID,
    ) -> std::result::Result<(), DesktopError> {
        if id == fallback_id {
            return Err(DesktopError::NavigationUnavailable(
                "special workspace fallback cannot be the workspace itself".into(),
            ));
        }
        let desktop = self.desktop_for_id(id)?;
        let fallback = self.desktop_for_id(fallback_id)?;
        unsafe {
            self.manager
                .remove_desktop(ComIn::new(&desktop), ComIn::new(&fallback))
                .ok()
                .map_err(|e| classify(&Error::win(
                    "IVirtualDesktopManagerInternal::RemoveDesktop",
                    &e,
                )))
        }
    }

'''
s = replace_once(s, anchor, insert + anchor, "insert GUID native methods")
old = r'''        unsafe {
            window_manager
                .MoveWindowToDesktop(hwnd, &desktop_id)
                .map_err(|e| {
                    classify(&Error::win(
                        "IVirtualDesktopManager::MoveWindowToDesktop",
                        &e,
                    ))
                })
        }
'''
new = r'''        self.move_window_to_desktop_id(hwnd, desktop_id)
'''
s = replace_once(s, old, new, "delegate indexed HWND move")
p.write_text(s, encoding="utf-8")

# --- state.rs: expose last normal identity and allow workspace cleanup ---
p = Path("src/desktop/state.rs")
s = p.read_text(encoding="utf-8")
anchor = r'''    pub(crate) fn previous(&self) -> Option<GUID> {
        self.previous_desktop
    }

'''
insert = r'''    pub(crate) fn current(&self) -> Option<GUID> {
        self.current_desktop
    }

    pub(crate) fn forget_desktop(&mut self, desktop: GUID) {
        self.last_focused.remove(&desktop);
        if self.current_desktop == Some(desktop) {
            self.current_desktop = None;
        }
        if self.previous_desktop == Some(desktop) {
            self.previous_desktop = None;
        }
    }

'''
s = replace_once(s, anchor, anchor + insert, "state workspace helpers")
p.write_text(s, encoding="utf-8")

# --- service.rs: replace hidden-window scratchpad with a dedicated VD ---
p = Path("src/desktop/service.rs")
s = p.read_text(encoding="utf-8")
s = replace_once(
    s,
    "    SW_HIDE, SW_RESTORE, SW_SHOWNA, WS_DISABLED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,\n",
    "    SW_RESTORE, WS_DISABLED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,\n",
    "remove hide/show constants",
)
s = sub_once(
    s,
    r"\n#\[derive\(Debug, Clone, Copy\)\]\nstruct ScratchpadState \{.*?\n\}\n",
    "\n",
    "remove ScratchpadState",
)
s = replace_once(
    s,
    r'''    /// Runtime-only scratchpad ownership; never persisted.
    scratchpad: Option<ScratchpadState>,
''',
    r'''    /// Runtime-only identity of WinShort's dedicated special workspace.
    special_workspace: Option<GUID>,
    /// Normal desktop to return to when leaving the special workspace.
    special_return: Option<GUID>,
''',
    "controller workspace fields",
)
s = replace_once(
    s,
    "            scratchpad: None,\n",
    "            special_workspace: None,\n            special_return: None,\n",
    "controller workspace init",
)

switch_to = r'''    fn switch_to(&mut self, index: usize) {
        self.remember_current_foreground();
        let previous = self
            .native
            .as_ref()
            .and_then(|native| native.current_desktop_id().ok());
        match self.native_ensure_switch(index) {
            Ok(target) => {
                if previous == self.special_workspace {
                    self.special_return = None;
                    self.history.observe_desktop(target);
                } else {
                    self.history.note_numbered_switch(previous, target);
                }
                self.last_served = Some(BackendKind::NativeShell);
                if let Err(error) = self.restore_focus(target) {
                    self.publish_failure("restore desktop focus", error);
                }
                self.publish_status();
            }
            Err(error)
                if self.special_workspace.is_none()
                    && error.permits_fallback()
                    && self.known_count.is_some_and(|count| index < count) =>
            {
                match self.fallback.switch_to(index) {
                    Ok(()) => {
                        crate::info!(
                            "switched to virtual desktop {} via keyboard fallback (target existed)",
                            index + 1
                        );
                        self.history.clear_identity();
                        self.last_served = Some(BackendKind::KeyboardFallback);
                        self.publish_status();
                    }
                    Err(fallback_error) => self.publish_failure("switch desktop", fallback_error),
                }
            }
            Err(error) => self.publish_failure("switch desktop", error),
        }
    }
'''
s = sub_once(s, r"    fn switch_to\(&mut self, index: usize\) \{.*?(?=    fn move_foreground)", switch_to, "switch_to")

move_foreground = r'''    fn move_foreground(&mut self, index: usize, hwnd_raw: isize, follow: bool) {
        let hwnd = raw_hwnd(hwnd_raw);
        if !eligible_window(hwnd) {
            self.publish_failure(
                if follow {
                    "move and follow foreground window"
                } else {
                    "move foreground window silently"
                },
                DesktopError::WindowUnavailable("foreground HWND is not eligible".into()),
            );
            return;
        }
        self.remember_current_foreground();
        let previous = self
            .native
            .as_ref()
            .and_then(|native| native.current_desktop_id().ok());
        let result = (|| {
            self.native_ensure_count(index + 1)?;
            let ids = self.normal_desktop_ids()?;
            let target = ids
                .get(index)
                .copied()
                .ok_or(DesktopError::TargetOutOfRange {
                    requested: index,
                    count: ids.len(),
                })?;
            let native = self.native.as_ref().ok_or_else(|| {
                DesktopError::MoveUnavailable("native backend is unavailable".into())
            })?;
            let current = native.window_desktop_id(hwnd)?;
            if current != target {
                native.move_window_to_desktop_id(hwnd, target)?;
            }
            self.history.remember(target, hwnd.0 as isize);

            if follow {
                native.switch_to_id(target)?;
                self.last_served = Some(BackendKind::NativeShell);
                if previous == self.special_workspace {
                    self.special_return = None;
                    self.history.observe_desktop(target);
                } else {
                    self.history.note_numbered_switch(previous, target);
                }
                if !activate_window(hwnd) {
                    return Err(DesktopError::Partial {
                        completed: format!(
                            "window moved to Desktop {} and the desktop switch completed",
                            index + 1
                        ),
                        failure: "SetForegroundWindow rejected the moved window".into(),
                    });
                }
            } else if previous.is_some() && previous != Some(target) {
                if let Err(error) = self.restore_focus(previous.expect("checked above")) {
                    return Err(DesktopError::Partial {
                        completed: format!("window moved silently to Desktop {}", index + 1),
                        failure: format!("source desktop focus restoration failed: {error}"),
                    });
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.last_served = Some(BackendKind::NativeShell);
                self.publish_status();
            }
            Err(error) => {
                if follow && matches!(&error, DesktopError::Partial { .. }) {
                    self.publish_status();
                }
                self.publish_failure(
                    if follow {
                        "move and follow foreground window"
                    } else {
                        "move foreground window silently"
                    },
                    error,
                );
            }
        }
    }
'''
s = sub_once(s, r"    fn move_foreground\(&mut self, index: usize, hwnd_raw: isize, follow: bool\) \{.*?(?=    fn switch_previous)", move_foreground, "move_foreground")

switch_previous = r'''    fn switch_previous(&mut self) {
        self.remember_current_foreground();
        let result = (|| {
            let current = self
                .native
                .as_ref()
                .ok_or_else(|| {
                    DesktopError::NavigationUnavailable(
                        "native desktop identity is unavailable".into(),
                    )
                })?
                .current_desktop_id()?;
            let ids = self.normal_desktop_ids()?;

            if Some(current) == self.special_workspace {
                let target = choose_special_return_target(
                    self.special_return,
                    self.history.current(),
                    &ids,
                )
                .ok_or_else(|| {
                    DesktopError::NavigationUnavailable(
                        "no normal desktop is available to leave the special workspace".into(),
                    )
                })?;
                self.native
                    .as_ref()
                    .expect("native backend was checked above")
                    .switch_to_id(target)?;
                self.special_return = None;
                self.history.observe_desktop(target);
                return Ok(target);
            }

            self.history.observe_desktop(current);
            if self.history.previous().is_none() {
                return Err(DesktopError::NavigationUnavailable(
                    "no previous desktop is remembered".into(),
                ));
            }
            let target = self.history.previous_if_present(&ids).ok_or_else(|| {
                DesktopError::NavigationUnavailable("remembered desktop was deleted".into())
            })?;
            if target == current {
                return Err(DesktopError::NavigationUnavailable(
                    "remembered desktop is already active".into(),
                ));
            }
            self.native
                .as_ref()
                .expect("native backend was checked above")
                .switch_to_id(target)?;
            self.history.note_previous_switch(current, target);
            Ok(target)
        })();
        match result {
            Ok(target) => {
                self.last_served = Some(BackendKind::NativeShell);
                if let Err(error) = self.restore_focus(target) {
                    self.publish_failure("restore previous desktop focus", error);
                }
                self.publish_status();
            }
            Err(error) => {
                if matches!(error, DesktopError::NavigationUnavailable(_))
                    && self.special_workspace.is_none()
                {
                    self.history.clear_previous();
                }
                self.publish_failure("switch previous desktop", error);
            }
        }
    }
'''
s = sub_once(s, r"    fn switch_previous\(&mut self\) \{.*?(?=    fn remember_foreground)", switch_previous, "switch_previous")

s = replace_once(s, "        self.clear_stale_scratchpad();\n", "", "remember_foreground stale clear")
old_observe = r'''    fn observe_current_desktop(&mut self) {
        if let Some(native) = &self.native {
            if let Ok(current) = native.current_desktop_id() {
                self.history.observe_desktop(current);
            }
        }
    }
'''
new_observe = r'''    fn observe_current_desktop(&mut self) {
        if let Some(native) = &self.native {
            if let Ok(current) = native.current_desktop_id() {
                if Some(current) != self.special_workspace {
                    self.history.observe_desktop(current);
                }
            }
        }
    }
'''
s = replace_once(s, old_observe, new_observe, "ignore special workspace in normal history")

special_block = r'''    fn configure_scratchpad(&mut self, managed: bool) {
        if managed {
            return;
        }
        if let Err(error) = self.release_special_workspace() {
            self.publish_failure("release special workspace", error);
        }
    }

    fn reconcile_special_workspace_ids(&mut self, ids: &[GUID]) {
        if self
            .special_workspace
            .is_some_and(|workspace| !ids.contains(&workspace))
        {
            crate::info!("special workspace was removed outside WinShort; clearing runtime identity");
            self.special_workspace = None;
            self.special_return = None;
        }
        if self.special_return.is_some_and(|return_to| {
            !ids.contains(&return_to) || Some(return_to) == self.special_workspace
        }) {
            self.special_return = None;
        }
    }

    fn native_desktop_ids(&mut self) -> std::result::Result<Vec<GUID>, DesktopError> {
        let ids = self
            .native
            .as_ref()
            .ok_or_else(|| self.native_unavailable_error("desktop identity"))?
            .desktop_ids()?;
        self.reconcile_special_workspace_ids(&ids);
        Ok(ids)
    }

    fn normal_desktop_ids(&mut self) -> std::result::Result<Vec<GUID>, DesktopError> {
        let ids = self.native_desktop_ids()?;
        Ok(numbered_desktop_ids(&ids, self.special_workspace))
    }

    fn ensure_special_workspace(&mut self) -> std::result::Result<GUID, DesktopError> {
        let ids = self.native_desktop_ids()?;
        if let Some(workspace) = self.special_workspace {
            return Ok(workspace);
        }
        if ids.len() >= 256 {
            return Err(DesktopError::CreationUnavailable(
                "cannot create special workspace because Windows already has 256 desktops".into(),
            ));
        }
        let workspace = self
            .native
            .as_ref()
            .ok_or_else(|| self.native_unavailable_error("special workspace creation"))?
            .create_desktop()?;
        self.special_workspace = Some(workspace);
        self.special_return = None;
        self.known_count = Some(ids.len());
        crate::info!("created dedicated special workspace {workspace:?}");
        Ok(workspace)
    }

    fn release_special_workspace(&mut self) -> std::result::Result<(), DesktopError> {
        let Some(workspace) = self.special_workspace else {
            self.special_return = None;
            return Ok(());
        };
        let ids = self.native_desktop_ids()?;
        if !ids.contains(&workspace) {
            self.special_workspace = None;
            self.special_return = None;
            self.history.forget_desktop(workspace);
            return Ok(());
        }
        let normal = numbered_desktop_ids(&ids, Some(workspace));
        let fallback = choose_special_return_target(
            self.special_return,
            self.history.current(),
            &normal,
        )
        .ok_or_else(|| {
            DesktopError::NavigationUnavailable(
                "no normal desktop is available for special workspace cleanup".into(),
            )
        })?;
        let current = self
            .native
            .as_ref()
            .expect("native backend was checked above")
            .current_desktop_id()?;
        self.native
            .as_ref()
            .expect("native backend was checked above")
            .remove_desktop_id(workspace, fallback)?;
        self.history.forget_desktop(workspace);
        if current == workspace {
            self.history.observe_desktop(fallback);
        }
        self.special_workspace = None;
        self.special_return = None;
        self.known_count = Some(normal.len());
        crate::info!("removed dedicated special workspace");
        Ok(())
    }

    fn assign_scratchpad(&mut self, hwnd_raw: isize) {
        let hwnd = raw_hwnd(hwnd_raw);
        if !eligible_window(hwnd) {
            self.publish_failure(
                "send to special workspace",
                DesktopError::WindowUnavailable("foreground HWND is not eligible".into()),
            );
            return;
        }
        self.remember_current_foreground();
        let result = (|| {
            let workspace = self.ensure_special_workspace()?;
            let native = self.native.as_ref().ok_or_else(|| {
                DesktopError::MoveUnavailable(
                    "special workspace requires the native desktop backend".into(),
                )
            })?;
            if native.window_desktop_id(hwnd)? != workspace {
                native.move_window_to_desktop_id(hwnd, workspace)?;
            }
            self.history.remember(workspace, hwnd.0 as isize);
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.last_served = Some(BackendKind::NativeShell);
                self.publish_status();
                crate::info!("moved foreground window into the special workspace");
            }
            Err(error) => self.publish_failure("send to special workspace", error),
        }
    }

    fn toggle_scratchpad(&mut self) {
        self.remember_current_foreground();
        let result = (|| {
            let workspace = self.ensure_special_workspace()?;
            let current = self
                .native
                .as_ref()
                .ok_or_else(|| {
                    DesktopError::NavigationUnavailable(
                        "special workspace requires the native desktop backend".into(),
                    )
                })?
                .current_desktop_id()?;
            let ids = self.normal_desktop_ids()?;
            if current == workspace {
                let target = choose_special_return_target(
                    self.special_return,
                    self.history.current(),
                    &ids,
                )
                .ok_or_else(|| {
                    DesktopError::NavigationUnavailable(
                        "no normal desktop is available to leave the special workspace".into(),
                    )
                })?;
                self.native
                    .as_ref()
                    .expect("native backend was checked above")
                    .switch_to_id(target)?;
                self.special_return = None;
                self.history.observe_desktop(target);
                Ok((target, false))
            } else {
                self.native
                    .as_ref()
                    .expect("native backend was checked above")
                    .switch_to_id(workspace)?;
                self.special_return = Some(current);
                Ok((workspace, true))
            }
        })();
        match result {
            Ok((target, entering)) => {
                self.last_served = Some(BackendKind::NativeShell);
                if let Err(error) = self.restore_focus(target) {
                    self.publish_failure(
                        if entering {
                            "restore special workspace focus"
                        } else {
                            "restore normal desktop focus"
                        },
                        error,
                    );
                }
                self.publish_status();
                crate::info!(
                    "{} special workspace",
                    if entering { "entered" } else { "left" }
                );
            }
            Err(error) => self.publish_failure("toggle special workspace", error),
        }
    }

'''
s = sub_once(s, r"    fn configure_scratchpad\(&mut self, managed: bool\) \{.*?(?=    fn restore_focus)", special_block, "special workspace block")

native_helpers = r'''    fn native_ensure_count(
        &mut self,
        target_count: usize,
    ) -> std::result::Result<(), DesktopError> {
        let current = self.normal_desktop_ids()?;
        let reserved = usize::from(self.special_workspace.is_some());
        if target_count.saturating_add(reserved) > 256 {
            return Err(DesktopError::CreationUnavailable(format!(
                "requested {target_count} normal desktops plus the special workspace exceeds the 256-desktop safety limit"
            )));
        }
        let missing = target_count.saturating_sub(current.len());
        for _ in 0..missing {
            self.native
                .as_ref()
                .ok_or_else(|| self.native_unavailable_error("desktop creation"))?
                .create_desktop()?;
        }
        let final_ids = self.normal_desktop_ids()?;
        if final_ids.len() < target_count {
            return Err(DesktopError::CreationUnavailable(format!(
                "CreateDesktop stopped at {} normal desktops; target was {target_count}",
                final_ids.len()
            )));
        }
        self.known_count = Some(final_ids.len());
        Ok(())
    }

    fn native_ensure_switch(&mut self, index: usize) -> std::result::Result<GUID, DesktopError> {
        self.native_ensure_count(index + 1)?;
        let ids = self.normal_desktop_ids()?;
        let target = ids
            .get(index)
            .copied()
            .ok_or(DesktopError::TargetOutOfRange {
                requested: index,
                count: ids.len(),
            })?;
        self.native
            .as_ref()
            .ok_or_else(|| self.native_unavailable_error("desktop switching"))?
            .switch_to_id(target)?;
        Ok(target)
    }

'''
s = sub_once(s, r"    fn native_ensure_count\(.*?(?=    fn recreate_native)", native_helpers, "native normal helpers")

recreate = r'''    fn recreate_native(&mut self) -> std::result::Result<(), DesktopError> {
        match InternalBackend::create(self.build) {
            Ok(native) => {
                self.native = Some(native);
                self.native_availability = BackendAvailability::Available;
                if let Ok(ids) = self
                    .native
                    .as_ref()
                    .expect("native backend was just recreated")
                    .desktop_ids()
                {
                    self.reconcile_special_workspace_ids(&ids);
                    self.known_count = Some(numbered_desktop_ids(&ids, self.special_workspace).len());
                }
                Ok(())
            }
            Err(error) => {
                self.native = None;
                self.native_availability = BackendAvailability::Failed {
                    reason: error.to_string(),
                };
                Err(DesktopError::BackendUnavailable(error.to_string()))
            }
        }
    }

'''
s = sub_once(s, r"    fn recreate_native\(&mut self\).*?(?=    fn native_unavailable_error)", recreate, "recreate native")

status = r'''    fn status(&self) -> BackendStatus {
        let count = self.native.as_ref().and_then(|native| {
            native
                .desktop_ids()
                .ok()
                .map(|ids| numbered_desktop_ids(&ids, self.special_workspace).len())
        });
        BackendStatus {
            native: self.native_availability.clone(),
            fallback: BackendAvailability::Available,
            active: if self.native.is_some() {
                BackendKind::NativeShell
            } else {
                BackendKind::KeyboardFallback
            },
            desktop_count: count,
            last_served: self.last_served,
        }
    }

'''
s = sub_once(s, r"    fn status\(&self\) -> BackendStatus \{.*?(?=    fn publish_failure)", status, "status normal count")

helpers = r'''fn numbered_desktop_ids(ids: &[GUID], special_workspace: Option<GUID>) -> Vec<GUID> {
    ids.iter()
        .copied()
        .filter(|id| Some(*id) != special_workspace)
        .collect()
}

fn choose_special_return_target(
    explicit_return: Option<GUID>,
    last_normal: Option<GUID>,
    normal_desktops: &[GUID],
) -> Option<GUID> {
    explicit_return
        .filter(|id| normal_desktops.contains(id))
        .or_else(|| last_normal.filter(|id| normal_desktops.contains(id)))
        .or_else(|| normal_desktops.first().copied())
}

'''
s = replace_once(s, "fn is_shell_surface_class(class: &str) -> bool {\n", helpers + "fn is_shell_surface_class(class: &str) -> bool {\n", "insert special helpers")
s = sub_once(s, r"\nfn scratchpad_window\(hwnd: HWND, owner_pid: u32\) -> bool \{.*?(?=\nfn window_process_id)", "\n", "remove retained scratchpad HWND helper")

old_shutdown = r'''    // The worker owns Scratchpad hide state. Always make a best-effort reveal
    // before releasing that ownership, including receiver-disconnect shutdown.
    if let Err(error) = controller.release_scratchpad() {
        crate::error_!("failed to restore hidden scratchpad during shutdown: {error}");
    }
'''
new_shutdown = r'''    // The special workspace is process-lifetime state. On graceful shutdown,
    // remove its dedicated desktop and let Shell move its windows to the
    // remembered normal fallback. A hard process crash can still leave that
    // desktop behind; WinShort deliberately does not guess at orphan identity.
    if let Err(error) = controller.release_special_workspace() {
        crate::error_!("failed to remove special workspace during shutdown: {error}");
    }
'''
s = replace_once(s, old_shutdown, new_shutdown, "shutdown special cleanup")

old_test_pattern = r'''    #\[test\]\n    fn stale_scratchpad_handle_is_cleared_without_shell_calls\(\) \{.*?\n    \}\n'''
new_tests = r'''    #[test]
    fn special_workspace_is_excluded_from_numbered_desktop_ordinals() {
        let first = GUID::from_u128(1);
        let special = GUID::from_u128(2);
        let second = GUID::from_u128(3);
        assert_eq!(
            numbered_desktop_ids(&[first, special, second], Some(special)),
            vec![first, second]
        );
    }

    #[test]
    fn special_workspace_return_prefers_explicit_then_history_then_first() {
        let first = GUID::from_u128(1);
        let second = GUID::from_u128(2);
        let stale = GUID::from_u128(9);
        let normal = [first, second];
        assert_eq!(
            choose_special_return_target(Some(second), Some(first), &normal),
            Some(second)
        );
        assert_eq!(
            choose_special_return_target(Some(stale), Some(first), &normal),
            Some(first)
        );
        assert_eq!(
            choose_special_return_target(Some(stale), Some(stale), &normal),
            Some(first)
        );
    }
'''
s = sub_once(s, old_test_pattern, new_tests, "replace scratchpad policy test")
p.write_text(s, encoding="utf-8")

# --- settings labels: keep schema keys, expose new semantics ---
p = Path("src/ui/layout.rs")
s = p.read_text(encoding="utf-8")
s = replace_once(
    s,
    '                    "Assign scratchpad",\n                    "Assign the current foreground window",\n',
    '                    "Send to special workspace",\n                    "Move the current foreground window into a dedicated virtual desktop",\n',
    "assign special label",
)
s = replace_once(
    s,
    '                    "Toggle scratchpad",\n                    "Show or hide the assigned window",\n',
    '                    "Toggle special workspace",\n                    "Switch between the dedicated workspace and your previous desktop",\n',
    "toggle special label",
)
p.write_text(s, encoding="utf-8")

print("special workspace rewrite staged")
