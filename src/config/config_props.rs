//! Property + fuzz-strategy tests for config load/save/validate/repair (#37).
//!
//! Tests touching the process-global read-only latch (#15) serialize via
//! `crate::config::latch_guard` — parallel tests must not poison each other.

use crate::config::load::load;
use crate::config::model::*;
use crate::config::{clear_config_readonly, config_readonly, latch_guard};
use crate::keyboard::binding::{Hotkey, VirtualKey};
use proptest::prelude::*;
use std::collections::HashMap;

fn opaque_id_strategy() -> impl Strategy<Value = String> {
    // Realistic opaque endpoint-ID alphabet (no NUL; TOML/Win32 reject it).
    r"[{0-9a-fA-F.\-_ }{1,120}]".prop_filter("non-empty", |s| !s.trim().is_empty())
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(256))]

    /// #15/#37: arbitrary realistic opaque endpoint IDs survive a TOML
    /// round-trip byte-for-byte.
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

    /// #37: repair is idempotent over generated boundary values.
    #[test]
    fn repair_is_idempotent(duration_ms in 0u32..=20_000u32, scale in -10.0f32..=20.0f32) {
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

    /// #58 fuzz strategy: arbitrary TOML docs parse or error cleanly; save
    /// stays functional afterwards; future schemas never enable writes.
    #[test]
    fn toml_fuzz_never_enables_writes(doc in "[^\\x00]{0,2000}") {
        let _guard = latch_guard();
        clear_config_readonly();
        let dir = std::env::temp_dir().join(format!(
            "ws_fuzz_{}_{:x}",
            std::process::id(),
            md5ish(doc.as_bytes())
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(crate::config::load::config_path(&dir), &doc).unwrap();
        let _ = load(&dir);
        // A latching doc must keep save blocked; anything else saves fine.
        if doc.contains("schema_version = ") && !doc.contains("schema_version = 1") {
            prop_assert!(config_readonly() || !doc.contains("schema_version"),
                "future schema must latch reads");
        }
        clear_config_readonly();
        assert!(crate::config::save::save(&dir, &Config::default()).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

fn md5ish(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// #58 deterministic regression: parse("") used to panic via unwrap.
#[test]
fn empty_token_returns_none_deterministically() {
    assert_eq!(VirtualKey::parse(""), None);
}
