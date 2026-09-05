use super::display_inventory::DisplayInventory;
use super::*;

fn empty_settings_ui() -> SettingsUi {
    let mut ui = SettingsUi::new(
        96,
        crate::audio::devices::DeviceLists {
            inputs: Vec::new(),
            outputs: Vec::new(),
            input_defaults: Default::default(),
            output_defaults: Default::default(),
            warnings: Vec::new(),
        },
        Config::default(),
        None,
        super::config_access::ConfigAccess::unavailable(),
    );
    ui.layout = SettingsLayout::build(DESIGN_WIDTH, DESIGN_HEIGHT, 0.0);
    ui
}

fn search_settings_ui() -> SettingsUi {
    let mut ui = empty_settings_ui();
    ui.layout =
        SettingsLayout::build_shell(DESIGN_WIDTH, DESIGN_HEIGHT, 0.0, Page::Home, "", 0, None);
    {
        let value = AutomationFocusOwner::Settings;
        ui.focus.set_owner(value);
    };
    ui
}

fn sample_profile(id: &str, name: &str, confirmed: bool) -> crate::display::DisplayProfile {
    crate::display::DisplayProfile {
        id: id.into(),
        name: name.into(),
        topology: crate::display::DisplayTopology::Extend,
        confirmed,
        routes: vec![crate::display::DisplayRoute {
            target_path: format!("target-{id}"),
            source_width: 1920,
            source_height: 1080,
            refresh_numerator: 60,
            refresh_denominator: 1,
            ..Default::default()
        }],
    }
}

mod audio;
mod displays;
mod focus;
mod navigation;
mod overlay;
mod pickers;
mod session;
mod shortcuts;
mod window;
