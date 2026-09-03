use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use super::contract;

pub(crate) fn build(manifest: &Path) {
    let provider = manifest.join("native/dynamic");

    let out = PathBuf::from(
        env::var_os("OUT_DIR").unwrap_or_else(|| panic!("Cargo must provide OUT_DIR")),
    );

    let header = out.join("bray_dynamic_contract.h");

    fs::write(&header, contract::declarations()).unwrap_or_else(|error| {
        panic!("could not write dynamic-library ABI declarations: {error}")
    });

    let mut native = cc::Build::new();

    native
        .cpp(true)
        .std("c++17")
        .include(provider.join("include"))
        .include(&out)
        .define("NOMINMAX", None)
        .file(provider.join("src/provider.cpp"))
        .warnings(false);

    crate::core::configure_discardable_sections(&mut native);
    native.compile("bray_dynamic_provider");

    if env::var("CARGO_CFG_TARGET_OS").is_ok_and(|target| target == "linux") {
        println!("cargo:rustc-link-lib=dl");
    }

    println!("cargo:rerun-if-changed={}", provider.display());
}
