//! Property + fuzz-strategy tests for config load/save/validate/repair (#37).
//!
//! Tests touching the process-global read-only latch (#15) are serialized via
//! `LATCH_LOCK` — parallel tests must not poison each other (see #48/#47
//! history for why globals need explicit serialization).

use crate::config::load::load;
use crate::config::model::*;
use crate::config::{clear_config_readonly, config_readonly};
use crate::keyboard::binding::{Hotkey, VirtualKey};
use proptest::prelude::*;
use std::collections::HashMap;

/// Serializes tests touching the process-global read-only latch (#15).
static LATCH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn latch_guard() -> std::sync::MutexGuard<'static, ()> {
    match LATCH_LOCK.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

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
}

fn md5ish(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[test]
fn future_schema_never_writable() {
    let _guard = latch_guard();
    clear_config_readonly();
    let dir = std::env::temp_dir().join(format!("ws_prop_ro_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        crate::config::load::config_path(&dir),
        "schema_version = 9\n",
    )
    .unwrap();

    let _ = crate::config::load::load(&dir);
    assert!(config_readonly(), "v9 must trip read-only latch");
    assert!(
        crate::config::save::save(&dir, &Config::default()).is_err(),
        "future schema must not be overwritten"
    );
    clear_config_readonly();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn toml_fuzz_never_panics() {
    let _guard = latch_guard();
    clear_config_readonly();
    let dir = std::env::temp_dir().join(format!("ws_fuzz_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Deterministic adversarial inputs (bounded fuzz corpus): future schema,
    // unknown nested fields, malformed hotkeys, opaque IDs, huge values.
    let corpus = [
        "schema_version = 9\n",
        "[general]\nnope = 1\n[deep]\n[nested]\nunknown_key = 'x'\n",
        "[hotkeys]\ntoggle_microphone = 'Ctrl+++'\n",
        "[audio]\noutput_device = '{0.0.0.00000000}.{ABC-DEF}'\n",
        "schema_version = 1\n[overlay]\nduration_ms = 999999\nscale = 99.0\n",
        "not = [even = [valid = [toml]]]]\n",
    ];
    for doc in corpus {
        std::fs::write(crate::config::load::config_path(&dir), doc).unwrap();
        let _ = load(&dir); // must not panic; may warn or error internally
        clear_config_readonly(); // load of future schemas latches; reset per case
                                 // save stays blocked only while latched; after clearing it works.
    }
    clear_config_readonly();
    let _ = std::fs::remove_dir_all(&dir);
}
