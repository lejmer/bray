use std::path::PathBuf;

use bray_target::{NativeTarget, TargetOutputKind, TargetOutputName};

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let triple = std::env::var("TARGET").expect("Cargo supplies the target triple");

    let target = NativeTarget::ALL
        .into_iter()
        .find(|target| target.as_str() == triple)
        .expect("the runtime requires a supported native target");

    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo supplies OUT_DIR"));

    let archive =
        TargetOutputName::for_native(target.object_format(), TargetOutputKind::StaticLibrary)
            .file_name("bray_runtime_bootstrap")
            .expect("the bootstrap archive has a valid name");

    println!("cargo:rerun-if-changed={}", root.join("runtime").display());

    println!(
        "cargo:rerun-if-changed={}",
        root.join("standard-library").display()
    );

    xtask::build_bootstrap(&root, target, &output.join(archive))
        .unwrap_or_else(|error| panic!("could not build the runtime report provider: {error}"));

    println!("cargo:rustc-link-search=native={}", output.display());
    println!("cargo:rustc-link-lib=static:-bundle=bray_runtime_bootstrap");
}
