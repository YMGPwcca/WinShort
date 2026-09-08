use super::*;

#[test]
fn unavailable_explicit_device_is_not_exposed_as_a_system_target() {
    let mut config = Config::default();
    config.audio.input_device = DeviceSelection::Endpoint("missing-endpoint".into());
    let devices = crate::audio::devices::DeviceLists {
        inputs: vec![crate::audio::DeviceId {
            endpoint: "current-endpoint".into(),
            name: "Current microphone".into(),
        }],
        outputs: Vec::new(),
        input_defaults: Default::default(),
        output_defaults: Default::default(),
        warnings: Vec::new(),
    };

    let model = picker_model(PickerKind::InputDevice, &config, &devices, &[], None)
        .expect("input device picker model");

    assert_eq!(model.choices().len(), 1);
    assert_eq!(model.current(), None);
    assert_eq!(model.choices()[0].label(), "Current microphone");
    assert_eq!(
        model.choices()[0].commit_value(),
        Some(&PickerCommit::InputDevice(DeviceSelection::Endpoint(
            "current-endpoint".into()
        )))
    );
    assert_eq!(
        config.audio.input_device,
        DeviceSelection::Endpoint("missing-endpoint".into())
    );
}

#[test]
fn value_for_uses_cached_devices_without_reacquiring_app() {
    let devices = crate::audio::devices::DeviceLists {
        inputs: vec![crate::audio::DeviceId {
            endpoint: "input".into(),
            name: "Cached microphone".into(),
        }],
        outputs: vec![crate::audio::DeviceId {
            endpoint: "output".into(),
            name: "Cached speakers".into(),
        }],
        input_defaults: crate::audio::devices::DefaultDevices {
            console: Some(crate::audio::DeviceId {
                endpoint: "input".into(),
                name: "Cached microphone".into(),
            }),
            ..Default::default()
        },
        output_defaults: Default::default(),
        warnings: Vec::new(),
    };
    let mut ui = SettingsUi::new(
        96,
        devices,
        Config::default(),
        None,
        super::super::config_access::ConfigAccess::unavailable(),
        super::super::config_access::ControlCenterAccess::system(),
    );
    ui.draft.audio.input_device = DeviceSelection::Endpoint("input".into());
    match ui.value_for(ElementId::InputDevice) {
        ControlValue::Text(value) => assert_eq!(value, "Cached microphone"),
        _ => panic!("unexpected control value variant"),
    }
}
