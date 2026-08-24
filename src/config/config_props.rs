//! Property tests for config load/save/validate/repair (#37).

use crate::config::model::*;
use crate::config::{config_readonly, clear_config_readonly};
use proptest::prelude::*;

fn opaque_id_strategy() -> impl Strategy<Value = String> {
    // Realistic opaque endpoint-ID alphabet: braces, dots, dashes, digits,
    // hex letters, underscores, spaces. No NUL (TOML/Win32 reject it).
    r"[{0-9a-fA-F.\-_ }{1,120}]"
        .prop_filter("non-empty", |s| !s.trim().is_empty())
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(256))]

    /// #15/#37: arbitrary realistic opaque endpoint IDs survive a TOML
    /// round-trip byte-for-byte — no truncation, case change, or escaping.
    #[test]
    fn prop_endpoint_id_round_trips(id in opaque_id_strategy()) {
        let mut cfg = Config::default();
        cfg.audio.output_device = DeviceSelection::Endpoint(id.clone());
        let toml = cfg.to_toml();
        let text = toml::to_string_pretty(&toml).unwrap();
        let parsed: ConfigToml = toml::from_str(&text).unwrap();
        let (cfg2, warnings) = Config::from_toml(&parsed);
        prop_assert!(warnings.is_empty(), "{warnings:?}");
        match (&cfg.audio.output_device, &cfg2.audio.output_device) {
            (DeviceSelection::Endpoint(a), DeviceSelection::Endpoint(b)) => {
                prop_assert_eq!(a, b, "endpoint id corrupted");
            }
            other => panic!("expected Endpoint on both sides: {other:?}"),
        }
    }

    /// #37: validation repair is idempotent — repairing an already-repaired
    /// config changes nothing.
    #[test]
    fn repair_is_idempotent(duration_ms in 0u32..=20_000, scale in -10.0f32..=20.0) {
        let mut cfg = Config::default();
        cfg.overlay.duration_ms = duration_ms;
        cfg.overlay.scale = scale;
        cfg.repair(&crate::config::validate(&cfg));
        let once = cfg.clone();
        cfg.repair(&crate::config::validate(&cfg));
        prop_assert_eq!(once.overlay.duration_ms, cfg.overlay.duration_ms);
        prop_assert_eq!(once.overlay.scale, cfg.overlay.scale);
        prop_assert_eq!(once.overlay.opacity, cfg.overlay.opacity);
    }

    /// #15/#37: a config written with a future schema_version must never be
    /// silently loaded and re-saved by the normal Save path.
    #[test]
    fn future_schema_never_writable(ver in 2u8..=255u8) {
        crate::config::clear_config_readonly();
        let dir = std::env::temp_dir().join(format!("ws_prop_{}_{}", std::process::id(), ver));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(crate::config::load::config_path(&dir), format!("schema_version = {ver}\n")).unwrap();

        let _ = crate::config::load::load(&dir);
        if ver > 1 {
            prop_assert!(config_readonly(), "v{ver} must trip read-only latch");
            let save_err = crate::config::save::save(&dir, &Config::default());
            prop_assert!(save_err.is_err(), "future schema must not be overwritten");
        }
        crate::config::clear_config_readonly();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
