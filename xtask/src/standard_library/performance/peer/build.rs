use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use bray_base::lowercase_hex;
use bray_target::{NativeTarget, ObjectFormat};
use sha2::{Digest as _, Sha256};

use super::super::model::PeerLanguage;
use super::source::{PeerSource, sources};

pub(in crate::standard_library::performance) struct BuiltPeer {
    pub language: PeerLanguage,
    pub toolchain: String,
    pub build_configuration: String,
    pub source_sha256: String,
    pub compile_link_nanoseconds: u64,
    pub executable: PathBuf,
    pub linker_map: PathBuf,
    pub timed_executable: PathBuf,
}

pub(in crate::standard_library::performance) fn build(
    root: &Path,
    output: &Path,
    target: NativeTarget,
    workload: &str,
) -> Result<Result<Vec<BuiltPeer>, String>, String> {
    let sources = match sources(workload) {
        Ok(sources) => sources,
        Err(reason) => return Ok(Err(reason.to_owned())),
    };

    let mut peers = Vec::with_capacity(sources.len());

    for source in sources {
        peers.push(match source.language {
            PeerLanguage::Rust => build_rust(output, target, source)?,
            PeerLanguage::Cpp => build_cpp(root, output, target, source)?,
        });
    }

    Ok(Ok(peers))
}

fn build_rust(
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
    let started = Instant::now();

    run_rustc(
        &source_path,
        &executable,
        &linker_map,
        target,
        source.selector,
        false,
    )?;

    let compile_link_nanoseconds = elapsed_nanoseconds(started);
    let timed_executable = directory.join(crate::native_toolchain::executable_name("peer-timed"));
    let timed_linker_map = directory.join("peer-timed.map");

    run_rustc(
        &source_path,
        &timed_executable,
        &timed_linker_map,
        target,
        source.selector,
        true,
    )?;

    Ok(BuiltPeer {
        language: PeerLanguage::Rust,
        toolchain: command_identity(Command::new("rustc").arg("--version"), "rustc")?,
        build_configuration: "rustc opt-level=3, debuginfo=0, panic=abort, one codegen unit, no LTO, stripped symbols".to_owned(),
        source_sha256: source_digest(source.selector, source.contents),
        compile_link_nanoseconds,
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
    timed: bool,
) -> Result<(), String> {
    let mut command = Command::new("rustc");

    command
        .arg(source)
        .args(["--edition", "2024", "--target", target.as_str()])
        .args(["-C", "opt-level=3", "-C", "debuginfo=0"])
        .args(["-C", "panic=abort", "-C", "codegen-units=1"])
        .args(["-C", "lto=off", "-C", "strip=symbols"])
        .arg("--cfg")
        .arg(format!("peer_workload=\"{workload}\""));

    if timed {
        command.args(["--cfg", "peer_timing"]);
    }

    append_rust_linker_map(&mut command, target.object_format(), linker_map)?;
    command.arg("-o").arg(executable);

    crate::command::require_success(command, "building Rust performance peer").map(|_| ())
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

    let compile_link_nanoseconds = elapsed_nanoseconds(started);
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

    Ok(BuiltPeer {
        language: PeerLanguage::Cpp,
        toolchain: command_identity(Command::new(&clang).arg("--version"), "clang")?,
        build_configuration: "clang++ C++20, O3, no debug information, no exceptions, no RTTI, no LTO, stripped symbols".to_owned(),
        source_sha256: source_digest(source.selector, source.contents),
        compile_link_nanoseconds,
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

    if target.as_str().starts_with("x86_64-") && target.object_format() == ObjectFormat::Coff {
        command.arg("-D_AMD64_");
    }

    if timed {
        command.arg("-DBRAY_PEER_TIMING");
    }

    append_cpp_linker_map(&mut command, target.object_format(), linker_map)?;
    command.arg(source).arg("-o").arg(executable);

    crate::command::require_success(command, "building C++ performance peer").map(|_| ())
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
