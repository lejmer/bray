use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use bray_base::lowercase_hex;
use bray_target::{NativeTarget, ObjectFormat};
use sha2::{Digest as _, Sha256};

use super::super::model::{PeerBuildConfiguration, PeerLanguage, RuntimeLinkage};
use super::source::{PeerSource, sources};

pub(in crate::standard_library::performance) struct BuiltPeer {
    pub language: PeerLanguage,
    pub toolchain: String,
    pub build_configuration: PeerBuildConfiguration,
    pub source_sha256: String,
    pub production_compile_link_nanoseconds: u64,
    pub executable: PathBuf,
    pub linker_map: PathBuf,
    pub timed_executable: PathBuf,
}

pub(in crate::standard_library::performance) fn build(
    root: &Path,
    output: &Path,
    target: NativeTarget,
    workload: &str,
) -> Result<Vec<BuiltPeer>, String> {
    let sources = sources(workload)?;

    let mut peers = Vec::with_capacity(sources.len());

    for source in sources {
        peers.push(match source.language {
            PeerLanguage::Rust => build_rust(root, output, target, source)?,
            PeerLanguage::Cpp => build_cpp(root, output, target, source)?,
        });
    }

    Ok(peers)
}

fn build_rust(
    root: &Path,
    output: &Path,
    target: NativeTarget,
    source: PeerSource,
) -> Result<BuiltPeer, String> {
    let directory = output.join("rust");

    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("could not create {}: {error}", directory.display()))?;

    let source_path = directory.join("peer.rs");

    std::fs::write(&source_path, source.contents)
        .map_err(|error| format!("could not write {}: {error}", source_path.display()))?;

    let executable = directory.join(crate::native_toolchain::executable_name("peer"));
    let linker_map = directory.join("peer.map");
    let linker = rust_linker(root, target);
    let production_arguments = rust_arguments(target, source.selector, &linker, false);
    let timed_arguments = rust_arguments(target, source.selector, &linker, true);
    let started = Instant::now();

    run_rustc(
        &source_path,
        &executable,
        &linker_map,
        target,
        source.selector,
        &linker,
        false,
    )?;

    let production_compile_link_nanoseconds = elapsed_nanoseconds(started);
    let timed_executable = directory.join(crate::native_toolchain::executable_name("peer-timed"));
    let timed_linker_map = directory.join("peer-timed.map");

    run_rustc(
        &source_path,
        &timed_executable,
        &timed_linker_map,
        target,
        source.selector,
        &linker,
        true,
    )?;

    Ok(BuiltPeer {
        language: PeerLanguage::Rust,
        toolchain: command_identity(Command::new("rustc").arg("--version"), "rustc")?,
        build_configuration: PeerBuildConfiguration {
            target: target.as_str().to_owned(),
            production_arguments,
            timed_arguments,
            linker: linker.display().to_string(),
            runtime_linkage: runtime_linkage(target)?,
            post_link_actions: vec!["rustc strips symbols during linking".to_owned()],
        },
        source_sha256: source_digest(source.selector, source.contents),
        production_compile_link_nanoseconds,
        executable,
        linker_map,
        timed_executable,
    })
}

fn run_rustc(
    source: &Path,
    executable: &Path,
    linker_map: &Path,
    target: NativeTarget,
    workload: &str,
    linker: &Path,
    timed: bool,
) -> Result<(), String> {
    let mut command = Command::new("rustc");

    command
        .arg(source)
        .args(["--edition", "2024", "--target", target.as_str()])
        .args(["-C", "opt-level=3", "-C", "debuginfo=0"])
        .args(["-C", "panic=abort", "-C", "codegen-units=1"])
        .args(["-C", "lto=off", "-C", "strip=symbols"])
        .arg("-C")
        .arg(format!("linker={}", linker.display()))
        .arg("--cfg")
        .arg(format!("peer_workload=\"{workload}\""));

    if target.object_format() == ObjectFormat::Coff {
        command.args(["-C", "target-feature=+crt-static"]);
    }

    if timed {
        command.args(["--cfg", "peer_timing"]);
    }

    append_rust_linker_map(&mut command, target.object_format(), linker_map)?;
    command.arg("-o").arg(executable);

    crate::command::require_success(command, "building Rust performance peer").map(|_| ())
}

fn rust_linker(root: &Path, target: NativeTarget) -> PathBuf {
    let name = match target.object_format() {
        ObjectFormat::Coff => "lld-link",
        ObjectFormat::Elf => "ld.lld",
        ObjectFormat::MachO => "ld64.lld",
        ObjectFormat::WebAssembly | ObjectFormat::Xcoff => "ld.lld",
    };

    bray_llvm_toolchain::tool_path(root, name)
}

fn rust_arguments(
    target: NativeTarget,
    workload: &str,
    linker: &Path,
    timed: bool,
) -> Vec<String> {
    let mut arguments = vec![
        "--edition=2024".to_owned(),
        format!("--target={}", target.as_str()),
        "-C opt-level=3".to_owned(),
        "-C debuginfo=0".to_owned(),
        "-C panic=abort".to_owned(),
        "-C codegen-units=1".to_owned(),
        "-C lto=off".to_owned(),
        "-C strip=symbols".to_owned(),
        format!("-C linker={}", linker.display()),
        format!("--cfg peer_workload=\"{workload}\""),
    ];

    if target.object_format() == ObjectFormat::Coff {
        arguments.push("-C target-feature=+crt-static".to_owned());
    }

    if timed {
        arguments.push("--cfg peer_timing".to_owned());
    }

    arguments
}

fn build_cpp(
    root: &Path,
    output: &Path,
    target: NativeTarget,
    source: PeerSource,
) -> Result<BuiltPeer, String> {
    let directory = output.join("cpp");

    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("could not create {}: {error}", directory.display()))?;

    let source_path = directory.join("peer.cpp");

    std::fs::write(&source_path, source.contents)
        .map_err(|error| format!("could not write {}: {error}", source_path.display()))?;

    let clang = bray_tooling::llvm_tool_path(bray_diagnostics::DiagnosticLlvmToolRole::CompilerDriver)
        .unwrap_or_else(|_| bray_llvm_toolchain::tool_path(root, "clang"));

    let executable = directory.join(crate::native_toolchain::executable_name("peer"));
    let linker_map = directory.join("peer.map");
    let production_arguments = cpp_arguments(target, source.selector, false);
    let timed_arguments = cpp_arguments(target, source.selector, true);
    let started = Instant::now();

    run_cpp(
        &clang,
        &source_path,
        &executable,
        &linker_map,
        target,
        source.selector,
        false,
    )?;

    strip_cpp_artifact(root, &executable)?;

    let production_compile_link_nanoseconds = elapsed_nanoseconds(started);
    let timed_executable = directory.join(crate::native_toolchain::executable_name("peer-timed"));
    let timed_linker_map = directory.join("peer-timed.map");

    run_cpp(
        &clang,
        &source_path,
        &timed_executable,
        &timed_linker_map,
        target,
        source.selector,
        true,
    )?;

    strip_cpp_artifact(root, &timed_executable)?;

    Ok(BuiltPeer {
        language: PeerLanguage::Cpp,
        toolchain: command_identity(Command::new(&clang).arg("--version"), "clang")?,
        build_configuration: PeerBuildConfiguration {
            target: target.as_str().to_owned(),
            production_arguments,
            timed_arguments,
            linker: "lld selected through the clang++ driver".to_owned(),
            runtime_linkage: runtime_linkage(target)?,
            post_link_actions: vec!["llvm-strip --strip-all".to_owned()],
        },
        source_sha256: source_digest(source.selector, source.contents),
        production_compile_link_nanoseconds,
        executable,
        linker_map,
        timed_executable,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "the direct C++ build keeps every recorded build input explicit"
)]
fn run_cpp(
    clang: &Path,
    source: &Path,
    executable: &Path,
    linker_map: &Path,
    target: NativeTarget,
    selector: &str,
    timed: bool,
) -> Result<(), String> {
    let mut command = Command::new(clang);

    command
        .args(["--driver-mode=g++", "-std=c++20", "-O3", "-DNDEBUG"])
        .args(["-fno-exceptions", "-fno-rtti", "-fuse-ld=lld"])
        .arg(format!("--target={}", target.as_str()))
        .arg(format!("-DBRAY_WORKLOAD={selector}"));

    if target.object_format() == ObjectFormat::Coff {
        command.arg("-fms-runtime-lib=static");

        if target.as_str().starts_with("x86_64-") {
            command.arg("-D_AMD64_");
        }
    } else if target.object_format() == ObjectFormat::Elf {
        command.args(["-static-libstdc++", "-static-libgcc"]);
    }

    if timed {
        command.arg("-DBRAY_PEER_TIMING");
    }

    append_cpp_linker_map(&mut command, target.object_format(), linker_map)?;
    command.arg(source).arg("-o").arg(executable);

    crate::command::require_success(command, "building C++ performance peer").map(|_| ())
}

fn cpp_arguments(target: NativeTarget, selector: &str, timed: bool) -> Vec<String> {
    let mut arguments = vec![
        "--driver-mode=g++".to_owned(),
        "-std=c++20".to_owned(),
        "-O3".to_owned(),
        "-DNDEBUG".to_owned(),
        "-fno-exceptions".to_owned(),
        "-fno-rtti".to_owned(),
        "-fuse-ld=lld".to_owned(),
        format!("--target={}", target.as_str()),
        format!("-DBRAY_WORKLOAD={selector}"),
    ];

    if target.object_format() == ObjectFormat::Coff {
        arguments.push("-fms-runtime-lib=static".to_owned());

        if target.as_str().starts_with("x86_64-") {
            arguments.push("-D_AMD64_".to_owned());
        }
    } else if target.object_format() == ObjectFormat::Elf {
        arguments.extend(["-static-libstdc++".to_owned(), "-static-libgcc".to_owned()]);
    }

    if timed {
        arguments.push("-DBRAY_PEER_TIMING".to_owned());
    }

    arguments
}

fn strip_cpp_artifact(root: &Path, executable: &Path) -> Result<(), String> {
    let strip = bray_llvm_toolchain::tool_path(root, "llvm-strip");
    let mut command = Command::new(strip);

    command.args(["--strip-all"]).arg(executable);

    crate::command::require_success(command, "stripping C++ performance peer").map(|_| ())
}

pub(in crate::standard_library::performance) fn runtime_linkage(
    target: NativeTarget,
) -> Result<RuntimeLinkage, String> {
    match target.object_format() {
        ObjectFormat::Coff | ObjectFormat::Elf => Ok(RuntimeLinkage::StaticApplicationRuntime),
        ObjectFormat::MachO => Err(
            "performance comparison requires static application runtimes, which the C++ peer does not yet provide on Mach-O"
                .to_owned(),
        ),
        ObjectFormat::WebAssembly | ObjectFormat::Xcoff => {
            Err("performance peers do not support the selected object format".to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_target::NativeTarget;

    use super::{cpp_arguments, rust_arguments, rust_linker};

    #[test]
    fn windows_peers_both_embed_the_static_msvc_runtime() {
        let target = NativeTarget::X86_64WindowsMsvc;
        let linker = rust_linker(std::path::Path::new("workspace"), target);
        let rust = rust_arguments(target, "small_output", &linker, false);
        let cpp = cpp_arguments(target, "1", false);

        assert!(rust.iter().any(|argument| argument == "-C target-feature=+crt-static"));
        assert!(cpp.iter().any(|argument| argument == "-fms-runtime-lib=static"));
        assert!(!rust.iter().any(|argument| argument.contains("-crt-static")));
        assert!(!cpp.iter().any(|argument| argument.contains("runtime-lib=dll")));
    }
}

fn append_rust_linker_map(
    command: &mut Command,
    format: ObjectFormat,
    map: &Path,
) -> Result<(), String> {
    let argument = match format {
        ObjectFormat::Coff => format!("/MAP:{}", map.display()),
        ObjectFormat::Elf => format!("-Wl,-Map,{}", map.display()),
        ObjectFormat::MachO => format!("-Wl,-map,{}", map.display()),
        ObjectFormat::WebAssembly | ObjectFormat::Xcoff => {
            return Err("performance peers do not support the selected object format".to_owned());
        }
    };

    command.arg("-C").arg(format!("link-arg={argument}"));

    Ok(())
}

fn append_cpp_linker_map(
    command: &mut Command,
    format: ObjectFormat,
    map: &Path,
) -> Result<(), String> {
    match format {
        ObjectFormat::Coff => command.arg(format!("-Wl,/MAP:{}", map.display())),
        ObjectFormat::Elf => command.arg(format!("-Wl,-Map,{}", map.display())),
        ObjectFormat::MachO => command.arg(format!("-Wl,-map,{}", map.display())),
        ObjectFormat::WebAssembly | ObjectFormat::Xcoff => {
            return Err("performance peers do not support the selected object format".to_owned());
        }
    };

    Ok(())
}

fn command_identity(command: &mut Command, tool: &str) -> Result<String, String> {
    let output = command
        .output()
        .map_err(|error| format!("could not run {tool}: {error}"))?;

    if !output.status.success() {
        return Err(format!("{tool} did not report its identity"));
    }

    let identity = String::from_utf8_lossy(&output.stdout);

    Ok(identity.lines().next().unwrap_or(&identity).trim().to_owned())
}

fn source_digest(selector: &str, source: &str) -> String {
    let mut digest = Sha256::new();

    digest.update(selector.as_bytes());
    digest.update([0]);
    digest.update(source.as_bytes());

    lowercase_hex(&digest.finalize())
}

fn elapsed_nanoseconds(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}
