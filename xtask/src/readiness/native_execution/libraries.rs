use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

use bray_target::{NativeTarget, TargetOutputKind, TargetOutputName};

use super::core::{native_output, product_output, standard_library_root};

pub(super) fn audit_native_libraries(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
) -> Result<(), String> {
    let directory = native_output("bray-native-libraries-")?;
    let toolchain = directory.path().join("toolchain");
    let workspace = directory.path().join("workspace");

    crate::native_toolchain::assemble_from_bundles(
        target,
        runtime,
        &standard_library_root(root),
        &toolchain,
    )?;

    crate::native_toolchain::copy_directory(
        &root.join("xtask/fixtures/native-libraries"),
        &workspace,
    )?;

    for (configuration, release) in [("debug", false), ("release", true)] {
        for (case, expected) in [("cross", b"MF".as_slice()), ("generic", b"AB".as_slice())] {
            let profiles = workspace.join(format!("profiles-{configuration}-{case}"));

            let mut build =
                Command::new(crate::native_toolchain::compiler_executable(root, "bray"));

            build
                .current_dir(root)
                .arg("--workspace")
                .arg(&workspace)
                .arg("--toolchain-root")
                .arg(&toolchain)
                .args(["--profile", "summary", "--profile-output"])
                .arg(&profiles)
                .args([
                    "build",
                    "--package",
                    "fixture.application",
                    "--product",
                    case,
                    "--target",
                    target.as_str(),
                ]);

            if release {
                build.arg("--release");
            }

            crate::command::require_success(
                build,
                "building transitive native library fixtures through Tack",
            )?;

            let output = workspace
                .join("build")
                .join(target.as_str())
                .join(configuration);

            let executable_name =
                TargetOutputName::for_native(target.object_format(), TargetOutputKind::Executable)
                    .file_name(case)
                    .expect("fixture product must form an executable name");

            let result = product_output(
                &output.join("fixture.application").join(executable_name),
                "running native library lifecycle fixture",
            )?;

            if !result.status.success() || result.stdout != expected || !result.stderr.is_empty() {
                return Err(format!(
                    "native library {case}/{configuration} expected {:?} and exit 0, got {result:?}",
                    String::from_utf8_lossy(expected)
                ));
            }

            for (provider, consumer) in [("base", "middle"), ("middle", "application")] {
                require_native_reuse(&output, &profiles, target, case, provider, consumer)?;
            }
        }
    }

    Ok(())
}

fn require_native_reuse(
    output: &Path,
    profiles: &Path,
    target: NativeTarget,
    product: &str,
    provider: &str,
    consumer: &str,
) -> Result<(), String> {
    let artifact = bray_package_interface::PackageArtifactInput::file(
        output
            .join(format!("fixture.{provider}"))
            .join(format!("{product}.brayimpl")),
        None,
    )
    .load_implementation()
    .map_err(|error| format!("could not read native library fixture: {error:?}"))?;

    let symbols = artifact
        .native_bindings()
        .map_err(|error| format!("could not read native fixture bindings: {error:?}"))?
        .into_iter()
        .map(|binding| {
            target
                .codegen_symbol_name(binding.symbol())
                .expect("fixture binding must carry the target symbol prefix")
                .to_owned()
        })
        .collect::<BTreeSet<_>>();

    let profile_path = profiles.join(format!(
        "fixture.{consumer}-{product}-{}-build.json",
        target.as_str()
    ));

    let bytes = fs::read(&profile_path)
        .map_err(|error| crate::workspace::io_error("read", &profile_path, error))?;

    let profile: bray_profile::CompilationProfileReport = serde_json::from_slice(&bytes)
        .map_err(|error| format!("{}: {error}", profile_path.display()))?;

    let plan = profile
        .native_codegen
        .ok_or_else(|| format!("{} has no native plan", profile_path.display()))?;

    if !plan.instances.iter().any(|instance| {
        instance.external
            && instance
                .symbol
                .as_ref()
                .is_some_and(|symbol| symbols.contains(symbol))
    }) {
        return Err(format!(
            "fixture.{consumer}/{product} did not reuse a native binding from fixture.{provider}"
        ));
    }

    Ok(())
}
