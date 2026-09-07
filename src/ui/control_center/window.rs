//! Window for the control center.

use super::appearance::settings_theme;
use super::chrome::apply_chrome;
use super::messages::settings_wndproc;
use super::native::invalidate;
use super::picker_choices::{picker_element, picker_model};
use super::placement::{
    client_rect_from_dip, client_work_rect, fixed_window_size, load_settings_rect,
    picker_height_px, picker_width_dip, window_geometry,
};
use super::state::{ControlCenterRuntimeSnapshot, SettingsUi};
use crate::config::validate::Violation;
use crate::error::{Error, Result};
use crate::platform::window as win;
use crate::ui::controls;
use crate::ui::layout::OnboardingStep;
use crate::ui::picker::{PickerCommit, PickerKind, PickerPopup};
use crate::ui::prompt::{PromptAction, TextPrompt};
use std::path::PathBuf;
use std::sync::OnceLock;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, ShowWindow, SW_HIDE, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WS_CLIPCHILDREN,
    WS_POPUP,
};

pub(crate) const CLASS_NAME: &str = "WinShort.ControlCenter";
pub(crate) const DESIGN_WIDTH: f32 = 960.0;
pub(crate) const DESIGN_HEIGHT: f32 = 660.0;
pub(super) const UI_TIMER: usize = 1;
pub(super) const UI_TIMER_MS: u32 = 16;
pub(super) const CONTROL_CENTER_STYLE: WINDOW_STYLE = WINDOW_STYLE(WS_POPUP.0 | WS_CLIPCHILDREN.0);
static REGISTERED: OnceLock<u16> = OnceLock::new();
pub(super) const WM_MOUSELEAVE: u32 = 0x02A3;

#[derive(Debug, Clone, Copy)]
pub(super) struct SavedSettingsRect {
    pub(super) rect: RECT,
    pub(super) dpi: u32,
}

pub(crate) struct ControlCenterWindow {
    pub hwnd: HWND,
    pub(super) picker: Option<PickerPopup>,
    pub(super) rename_prompt: Option<TextPrompt>,
    pub(super) last_rect: Option<SavedSettingsRect>,
    pub(super) position_path: PathBuf,
}

impl ControlCenterWindow {
    pub(crate) fn create(
        devices: crate::audio::devices::DeviceLists,
        config_access: super::config_access::ConfigAccess,
        access: super::config_access::ControlCenterAccess,
    ) -> Result<Self> {
        let _atom = win::register_class_once(&REGISTERED, CLASS_NAME, Some(settings_wndproc))?;

        let primary = crate::platform::monitor::primary();
        let primary_dpi = primary.as_ref().map_or(96, |m| m.dpi);
        let (default_width, default_height) = fixed_window_size(primary_dpi);
        let position_path = access.data_dir().join("settings-window.txt");
        let saved = load_settings_rect(&position_path);
        let (x, y, w, h, dpi) = window_geometry(
            primary.as_ref().map(|monitor| monitor.work),
            primary_dpi,
            saved,
            default_width,
            default_height,
        );

        let draft = (*config_access.current()).clone();
        let onboarding_step =
            crate::ui::first_run::should_show(&access.data_dir()).then_some(OnboardingStep::Setup);
        let mut state = win::WindowCreation::new(SettingsUi::new(
            dpi,
            devices,
            draft,
            onboarding_step,
            config_access,
            access,
        ));
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                windows::core::PCWSTR(windows::core::HSTRING::from(CLASS_NAME).as_ptr()),
                windows::core::PCWSTR(windows::core::HSTRING::from("WinShort").as_ptr()),
                CONTROL_CENTER_STYLE,
                x,
                y,
                w,
                h,
                None,
                None,
                None,
                Some(state.parameter()),
            )
        }
        .map_err(|e| Error::win("CreateWindowExW(settings)", &e))?;
        // SAFETY: this constructor exclusively owns the newly created HWND.
        let construction = unsafe { win::WindowConstructionGuard::new(hwnd) };

        let actual_dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) }.max(96);
        if actual_dpi != dpi {
            if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(hwnd) } {
                cell.borrow_mut().dpi = actual_dpi;
            }
        }
        apply_chrome(hwnd, settings_theme());
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(hwnd) } {
            let mut ui = cell.borrow_mut();
            ui.refresh_overlay_preview_aspect();
            ui.rebuild_layout(hwnd);
            ui.publish_automation_snapshot(hwnd);
        }
        let hwnd = construction.complete();
        Ok(Self {
            hwnd,
            picker: None,
            last_rect: saved,
            position_path,
            rename_prompt: None,
        })
    }

    pub(crate) fn set_display_rollback_state(&mut self, active: bool, keep_available: bool) {
        let Some(cell) = (unsafe { win::state_cell::<SettingsUi>(self.hwnd) }) else {
            return;
        };
        let changed = {
            let mut ui = cell.borrow_mut();
            let before = ui.display_rollback_status();
            ui.set_display_rollback_state(active, keep_available);
            let changed = ui.display_rollback_status() != before;
            if changed {
                ui.rebuild_layout(self.hwnd);
            }
            changed
        };
        if changed {
            invalidate(self.hwnd);
            if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
                cell.borrow_mut().publish_automation_snapshot(self.hwnd);
            }
        }
    }

    pub(crate) fn set_runtime_snapshot(&mut self, snapshot: ControlCenterRuntimeSnapshot) {
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            let mut ui = cell.borrow_mut();
            ui.set_runtime_snapshot(snapshot);
            ui.rebuild_layout(self.hwnd);
            ui.publish_automation_snapshot(self.hwnd);
        }
        invalidate(self.hwnd);
    }

    pub(crate) fn open_display_profile_rename(
        &mut self,
        profile_id: String,
        current_name: String,
    ) -> Result<()> {
        self.close_rename_prompt();
        self.rename_prompt = Some(TextPrompt::create(
            self.hwnd,
            PromptAction::RenameProfile { profile_id },
            "Rename display profile",
            "Rename",
            &current_name,
        )?);
        Ok(())
    }

    pub(crate) fn open_display_route_edit(
        &mut self,
        profile_id: String,
        route_index: usize,
        initial: String,
    ) -> Result<()> {
        self.close_rename_prompt();
        self.rename_prompt = Some(TextPrompt::create(
            self.hwnd,
            PromptAction::EditRoute {
                profile_id,
                route_index,
            },
            "Edit advanced display output",
            "Apply",
            &initial,
        )?);
        Ok(())
    }

    pub(crate) fn edit_display_route(&mut self, profile_id: &str, route_index: usize, value: &str) {
        self.close_rename_prompt();
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            let mut ui = cell.borrow_mut();
            ui.edit_display_route(profile_id, route_index, value);
            ui.publish_automation_snapshot(self.hwnd);
        }
        invalidate(self.hwnd);
    }

    pub(crate) fn rename_display_profile(&mut self, profile_id: &str, name: &str) {
        self.close_rename_prompt();
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            let mut ui = cell.borrow_mut();
            let before = ui.draft.clone();
            ui.rename_display_profile(profile_id, name);
            if ui.draft != before && !ui.display.is_dirty() {
                ui.commit_local_change(self.hwnd, before);
            }
            ui.publish_automation_snapshot(self.hwnd);
        }
        invalidate(self.hwnd);
    }

    pub(crate) fn cancel_display_profile_rename(&mut self) {
        self.close_rename_prompt();
    }

    pub(crate) fn update_display_profile_after_keep(
        &mut self,
        confirmed_profile: &crate::display::DisplayProfile,
    ) {
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            let mut ui = cell.borrow_mut();
            if let Some(existing) = ui
                .draft
                .display_profiles
                .profiles
                .iter_mut()
                .find(|profile| profile.id.eq_ignore_ascii_case(&confirmed_profile.id))
            {
                *existing = confirmed_profile.clone();
            }
            ui.display.close();
            ui.rebuild_layout(self.hwnd);
        }
        invalidate(self.hwnd);
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            cell.borrow_mut().publish_automation_snapshot(self.hwnd);
        }
    }

    pub(super) fn close_rename_prompt(&mut self) {
        if let Some(mut prompt) = self.rename_prompt.take() {
            prompt.close();
        }
    }

    pub(crate) fn show(&mut self) -> Result<()> {
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            let mut ui = cell.borrow_mut();
            ui.interaction.reopen();
            ui.startup_enabled = ui.access.startup_enabled();
            ui.debug_logging_enabled = ui.access.debug_logging_enabled();
            if !ui.dirty() {
                let current = ui.config_access.current();
                ui.replace_draft((*current).clone());
            }
            ui.refresh_display_outputs();
            ui.refresh_overlay_preview_aspect();
            ui.rebuild_layout(self.hwnd);
            ui.validation.clear();
            invalidate(self.hwnd);
            ui.publish_automation_snapshot(self.hwnd);
        }
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(self.hwnd);
            let _ = SetFocus(Some(self.hwnd));
        }
        let actual = unsafe { GetFocus() };
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            cell.borrow_mut()
                .sync_focus_after_set_focus(self.hwnd, actual);
        }
        Ok(())
    }

    pub(crate) fn refresh_devices(&mut self, devices: crate::audio::devices::DeviceLists) {
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            let mut ui = cell.borrow_mut();
            ui.devices = devices;
            ui.rebuild_layout(self.hwnd);
            ui.publish_automation_snapshot(self.hwnd);
        }
        invalidate(self.hwnd);
    }

    pub(crate) fn remember_position(&mut self) {
        let mut rect = RECT::default();
        let ok =
            unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowRect(self.hwnd, &mut rect) };
        if ok.is_ok() {
            let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(self.hwnd) }.max(96);
            self.last_rect = Some(SavedSettingsRect { rect, dpi });
        }
    }

    pub(crate) fn persist_position(&mut self) {
        self.remember_position();
        let Some(saved) = self.last_rect else {
            return;
        };
        let path = &self.position_path;
        if let Some(parent) = path.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                crate::warn_!("create settings position directory failed: {error}");
                return;
            }
        }
        let contents = format!(
            "left={}\ntop={}\nright={}\nbottom={}\ndpi={}\n",
            saved.rect.left, saved.rect.top, saved.rect.right, saved.rect.bottom, saved.dpi
        );
        if let Err(error) = std::fs::write(path, contents) {
            crate::warn_!("persist settings position failed: {error}");
        }
    }

    pub(crate) fn open_picker(
        &mut self,
        kind: PickerKind,
        devices: crate::audio::devices::DeviceLists,
    ) -> Result<()> {
        let Some(cell) = (unsafe { win::state_cell::<SettingsUi>(self.hwnd) }) else {
            return Err(Error::internal("settings state missing"));
        };
        if !cell.borrow().picker_activation_allowed() {
            return Ok(());
        }
        self.cancel_picker();
        if matches!(kind, PickerKind::DisplayOutputs | PickerKind::DisplayRoute) {
            cell.borrow_mut().refresh_display_outputs();
        }
        let owner = picker_element(kind);
        let (draft, display_outputs, control_rect, dpi, selected_display_route) = {
            let ui = cell.borrow();
            let element = ui
                .layout
                .element(owner)
                .ok_or_else(|| Error::internal("settings picker row missing"))?;
            (
                ui.draft.clone(),
                ui.inventory.outputs().to_vec(),
                controls::value_control_rect_for(element.rect, element.id, element.kind),
                ui.dpi,
                ui.display.selected_route(),
            )
        };
        let anchor = client_rect_from_dip(control_rect, dpi);
        let model = picker_model(
            kind,
            &draft,
            &devices,
            &display_outputs,
            selected_display_route,
        )
        .map_err(Error::internal)?;
        if model.choices().is_empty() {
            return Err(Error::config("no choices available"));
        }
        let work = client_work_rect(self.hwnd)?;
        let scale = dpi.max(96) as f32 / 96.0;
        let width = (picker_width_dip(control_rect.w, model.choices()) * scale).round() as i32;
        let height = picker_height_px(model.choices().len(), scale);
        let geometry = crate::ui::picker::place_popup(anchor, work, width, height);
        let picker = match PickerPopup::create(self.hwnd, model, geometry) {
            Ok(picker) => picker,
            Err(error) => {
                self.cancel_picker();
                return Err(error);
            }
        };
        let picker_hwnd = picker.hwnd;
        let picker_list_hwnd = picker.list;
        self.picker = Some(picker);
        let actual = unsafe { GetFocus() };
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            cell.borrow_mut().set_picker_open(
                self.hwnd,
                owner,
                picker_hwnd,
                picker_list_hwnd,
                actual,
            );
        }
        let can_activate = if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            cell.borrow().picker_activation_allowed()
        } else {
            false
        };
        if !can_activate {
            self.cancel_picker_without_focus();
            return Ok(());
        }
        if let Some(picker) = self.picker.as_ref() {
            picker.activate();
        }
        let actual = unsafe { GetFocus() };
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            cell.borrow_mut().sync_picker_focus(self.hwnd, actual);
        }
        Ok(())
    }

    pub(crate) fn commit_picker(&mut self, commit: PickerCommit) {
        let kind = commit.kind();
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            let mut ui = cell.borrow_mut();
            if kind == PickerKind::DisplayProfile && ui.display.is_dirty() {
                ui.validation = vec![Violation {
                    field: "Displays".into(),
                    message:
                        "Test or discard the current display edits before selecting another profile"
                            .into(),
                }];
            } else {
                let before = ui.draft.clone();
                ui.apply_picker(commit);
                let risky_display_edit = matches!(
                    kind,
                    PickerKind::DisplayOutputs | PickerKind::DisplayTopology
                );
                if ui.draft != before && !risky_display_edit {
                    ui.commit_local_change(self.hwnd, before);
                }
            }
            ui.publish_automation_snapshot(self.hwnd);
        }
        invalidate(self.hwnd);
        self.cancel_picker();
    }

    pub(crate) fn cancel_picker(&mut self) {
        self.cancel_picker_impl(true);
    }

    pub(crate) fn cancel_picker_without_focus(&mut self) {
        self.cancel_picker_impl(false);
    }

    pub(crate) fn discard_uncommitted_draft(&mut self) {
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            let mut ui = cell.borrow_mut();
            ui.discard_uncommitted_draft();
            ui.publish_automation_snapshot(self.hwnd);
        }
        invalidate(self.hwnd);
    }

    pub(crate) fn close_for_hide(&mut self) {
        self.close_rename_prompt();
        self.cancel_picker_without_focus();
        self.discard_uncommitted_draft();
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    pub(super) fn cancel_picker_impl(&mut self, restore_focus: bool) {
        let _ = self.picker.take();
        if restore_focus {
            unsafe {
                let _ = SetFocus(Some(self.hwnd));
            }
        }
        let actual = unsafe { GetFocus() };
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            cell.borrow_mut().set_picker_closed(self.hwnd, actual);
        }
    }

    pub(crate) fn focus_next_from_picker(&mut self, reverse: bool) {
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            cell.borrow_mut().focus_next(self.hwnd, reverse);
        }
        unsafe {
            let _ = SetFocus(Some(self.hwnd));
        }
        let actual = unsafe { GetFocus() };
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            cell.borrow_mut()
                .sync_focus_after_set_focus(self.hwnd, actual);
        }
    }

    pub(crate) fn picker_hwnd(&self) -> Option<HWND> {
        self.picker.as_ref().map(|picker| picker.hwnd)
    }
}
