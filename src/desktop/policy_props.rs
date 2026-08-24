//! Property tests for desktop fallback policy and SendInput cleanup (#37).

use crate::desktop::backend::DesktopError;
use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(128))]

    /// #20 invariant: ONLY RPC/backend-unavailable classes permit fallback.
    /// Semantic refusals (target out of range, unsupported build, ABI
    /// mismatch, switch failure) never inject synthetic input.
    #[test]
    fn semantic_errors_never_permit_fallback(
        requested in 0usize..64,
        count in 0usize..64,
        hr in any::<i32>(),
        reason in ".*",
    ) {
        let semantic = [
            DesktopError::TargetOutOfRange { requested, count },
            DesktopError::UnsupportedBuild(19041),
            DesktopError::AbiMismatch(reason.clone()),
            DesktopError::SwitchFailed(hr),
        ];
        for e in semantic {
            prop_assert!(!e.permits_fallback(), "{e:?} must never fall back");
        }
        let transient = [
            DesktopError::RpcDisconnected,
            DesktopError::BackendUnavailable(reason),
        ];
        for e in transient {
            prop_assert!(e.permits_fallback(), "{e:?} should permit fallback");
        }
    }
}
