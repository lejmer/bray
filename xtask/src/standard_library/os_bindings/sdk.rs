use std::path::{Path, PathBuf};
use std::process::Command;

use bray_target::NativeTarget;

use super::model::{SdkRevision, TargetDescription};
use crate::command::require_success;

pub(super) fn configure(
    command: &mut Command,
    compiler: &Path,
    target: NativeTarget,
    description: &TargetDescription,
    root: &Path,
    compiler_root: Option<&Path>,
) -> Result<(), String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("could not resolve SDK root {}: {error}", root.display()))?;

    match &description.sdk.revision {
        SdkRevision::Linux {
            distribution,
            release,
            ..
        } => configure_linux(command, compiler, target, &root, distribution, release),
        SdkRevision::MacOs { version } => configure_macos(command, &root, version),
        SdkRevision::Windows { version } => configure_windows(
            command,
            compiler,
            target,
            &root,
            version,
            compiler_root,
            description,
        ),
    }
}

fn configure_linux(
    command: &mut Command,
    compiler: &Path,
    target: NativeTarget,
    root: &Path,
    distribution: &str,
    release: &str,
) -> Result<(), String> {
    verify_linux_release(root, distribution, release)?;

    let resource_include = compiler_resource_include(compiler)?;
    let system_include = required_directory(root.join("usr/include"), "Linux SDK include root")?;

    let architecture = match target {
        NativeTarget::X86_64LinuxGnu => "x86_64-linux-gnu",
        NativeTarget::Aarch64LinuxGnu => "aarch64-linux-gnu",
        _ => return Err(format!("{} is not a Linux SDK target", target.as_str())),
    };

    let architecture_include = required_directory(
        system_include.join(architecture),
        "Linux target include root",
    )?;

    command
        .arg(format!("--sysroot={}", root.display()))
        .arg("-nostdinc")
        .arg("-isystem")
        .arg(resource_include)
        .arg("-isystem")
        .arg(architecture_include)
        .arg("-isystem")
        .arg(system_include);

    Ok(())
}

fn verify_linux_release(root: &Path, distribution: &str, release: &str) -> Result<(), String> {
    let os_release = root.join("etc/os-release");

    let contents = std::fs::read_to_string(&os_release)
        .map_err(|error| format!("could not read {}: {error}", os_release.display()))?;

    let expected_id = distribution.to_ascii_lowercase();
    let actual_id = release_field(&contents, "ID");
    let actual_version = release_field(&contents, "VERSION_ID");

    if actual_id.as_deref() != Some(expected_id.as_str())
        || actual_version.as_deref() != Some(release)
    {
        return Err(format!(
            "{} is not a {distribution} {release} sysroot",
            root.display()
        ));
    }

    Ok(())
}

fn release_field(contents: &str, name: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let value = line.strip_prefix(name)?.strip_prefix('=')?;

        Some(value.trim_matches('"').to_owned())
    })
}

fn configure_macos(command: &mut Command, root: &Path, version: &str) -> Result<(), String> {
    let settings = root.join("SDKSettings.json");

    let bytes = std::fs::read(&settings)
        .map_err(|error| format!("could not read {}: {error}", settings.display()))?;

    let settings: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("could not parse {}: {error}", settings.display()))?;

    if settings.get("Version").and_then(serde_json::Value::as_str) != Some(version) {
        return Err(format!(
            "{} is not macOS SDK revision {version}",
            root.display()
        ));
    }

    command.arg("-isysroot").arg(root);

    Ok(())
}

fn configure_windows(
    command: &mut Command,
    compiler: &Path,
    target: NativeTarget,
    root: &Path,
    version: &str,
    compiler_root: Option<&Path>,
    description: &TargetDescription,
) -> Result<(), String> {
    let architecture = match target {
        NativeTarget::X86_64WindowsMsvc => "x64",
        NativeTarget::Aarch64WindowsMsvc => "arm64",
        _ => return Err(format!("{} is not a Windows SDK target", target.as_str())),
    };

    let include = root.join("Include").join(version);
    let library = root.join("Lib").join(version);

    // Model validation requires one compiler runtime for every Windows SDK.
    let runtime = description
        .sdk
        .compiler_runtime
        .as_ref()
        .expect("validated Windows SDKs identify a compiler runtime");

    let compiler_root = compiler_root
        .ok_or_else(|| "Windows SDK probes require --compiler-root".to_owned())?
        .canonicalize()
        .map_err(|error| format!("could not resolve compiler root: {error}"))?;

    if compiler_root.file_name().and_then(|name| name.to_str()) != Some(&runtime.revision) {
        return Err(format!(
            "{} is not compiler runtime revision {}",
            compiler_root.display(),
            runtime.revision
        ));
    }

    let resource_include = compiler_resource_include(compiler)?;

    let runtime_include =
        required_directory(compiler_root.join("include"), "Visual C++ include root")?;

    let shared = required_directory(include.join("shared"), "Windows shared include root")?;
    let ucrt = required_directory(include.join("ucrt"), "Windows UCRT include root")?;
    let um = required_directory(include.join("um"), "Windows UM include root")?;
    let winrt = required_directory(include.join("winrt"), "Windows WinRT include root")?;

    let ucrt_library = required_directory(
        library.join("ucrt").join(architecture),
        "Windows UCRT library root",
    )?;

    let um_library = required_directory(
        library.join("um").join(architecture),
        "Windows UM library root",
    )?;

    let runtime_library = required_directory(
        compiler_root.join("lib").join(architecture),
        "Visual C++ library root",
    )?;

    command
        .arg("-nostdinc")
        .arg("-isystem")
        .arg(resource_include)
        .arg("-isystem")
        .arg(runtime_include)
        .arg("-isystem")
        .arg(ucrt)
        .arg("-isystem")
        .arg(shared)
        .arg("-isystem")
        .arg(um)
        .arg("-isystem")
        .arg(winrt)
        .arg("-L")
        .arg(runtime_library)
        .arg("-L")
        .arg(ucrt_library)
        .arg("-L")
        .arg(um_library);

    Ok(())
}

fn compiler_resource_include(compiler: &Path) -> Result<PathBuf, String> {
    let output = require_success(
        {
            let mut command = Command::new(compiler);

            command.arg("--print-resource-dir");

            command
        },
        "locating the provisioned C compiler resource directory",
    )?;

    let directory = String::from_utf8(output.stdout)
        .map_err(|error| format!("the C compiler resource directory is not UTF-8: {error}"))?;

    required_directory(
        PathBuf::from(directory.trim()).join("include"),
        "C compiler resource include root",
    )
}

fn required_directory(path: PathBuf, role: &str) -> Result<PathBuf, String> {
    if path.is_dir() {
        return Ok(path);
    }

    Err(format!("{role} {} does not exist", path.display()))
}
