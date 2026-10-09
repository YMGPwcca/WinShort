//! Embedded, copyable identity of the actual compiled build.

pub(crate) fn summary() -> String {
    format!(
        "WinShort {} · {}",
        env!("CARGO_PKG_VERSION"),
        env!("WINSHORT_BUILD_REVISION")
    )
}

pub(crate) fn info() -> String {
    format!(
        "WinShort {}\nRevision: {}\nBuilt: {}\nBuild ID: {}",
        env!("CARGO_PKG_VERSION"),
        env!("WINSHORT_BUILD_REVISION"),
        env!("WINSHORT_BUILD_DATE"),
        env!("WINSHORT_BUILD_ID")
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn copied_version_identifies_the_compiled_artifact() {
        let info = super::info();
        assert!(info.contains(env!("WINSHORT_BUILD_REVISION")));
        assert!(info.contains(env!("WINSHORT_BUILD_DATE")));
        assert!(info.contains(env!("WINSHORT_BUILD_ID")));
        assert!(env!("WINSHORT_BUILD_ID").parse::<u128>().is_ok());
    }
}
