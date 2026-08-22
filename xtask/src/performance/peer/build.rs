use std::collections::BTreeMap;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use bray_target::{NativeTarget, ObjectFormat};

use super::super::model::{
    PeerBatching, PeerBuildConfiguration, PeerCompilerConfiguration, PeerLanguage, RuntimeLinkage,
    WorkloadCompilationReport,
};
use super::source::{PeerSource, sources};

const RUST_INNER_ITERATIONS_ENVIRONMENT: &str = "BRAY_PERFORMANCE_INNER_ITERATIONS";

pub(in crate::performance) struct BuiltPeer {
    pub language: PeerLanguage,
    pub toolchain: String,
    pub build_configuration: PeerBuildConfiguration,
    pub source_sha256: String,
    pub compilation: WorkloadCompilationReport,
    pub executable: PathBuf,
    pub linker_map: PathBuf,
    pub timed_executable: PathBuf,
}

pub(in crate::performance) fn build(
    root: &Path,
    output: &Path,
    target: NativeTarget,
    workload: &str,
    controlled_inner_iterations: NonZeroU64,
) -> Result<Vec<BuiltPeer>, String> {
    let sources = sources(workload)?;

    let mut peers = Vec::with_capacity(sources.len());

    for source in sources {
        peers.push(match source.language {
            PeerLanguage::Rust => {
                build_rust(root, output, target, source, controlled_inner_iterations)?
            }
            PeerLanguage::Cpp => {
                build_cpp(root, output, target, source, controlled_inner_iterations)?
            }
        });
    }

    Ok(peers)
}

fn build_rust(
    root: &Path,
    output: &Path,
    target: NativeTarget,
    source: PeerSource,
    controlled_inner_iterations: NonZeroU64,
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

    let production = rust_configuration(
        &source_path,
        &executable,
        &linker_map,
        target,
        source.selector,
        &linker,
        None,
    )?;

    let process = super::super::compiler_timing::process(
        "rustc",
        &production,
        "building Rust performance peer",
    )?;

    let profiled_executable =
        directory.join(crate::native_toolchain::executable_name("peer-profiled"));

    let profiled_linker_map = directory.join("peer-profiled.map");

    let mut profiled = rust_configuration(
        &source_path,
        &profiled_executable,
        &profiled_linker_map,
        target,
        source.selector,
        &linker,
        None,
    )?;

    profiled.arguments.extend([
        "-Z".to_owned(),
        "time-passes".to_owned(),
        "-Z".to_owned(),
        "time-passes-format=json".to_owned(),
    ]);

    profiled
        .environment
        .insert("RUSTC_BOOTSTRAP".to_owned(), "1".to_owned());

    let compiler = super::super::compiler_timing::rust(
        "rustc",
        &profiled,
        "profiling Rust performance peer compilation",
    )?;

    let compilation = super::super::compiler_timing::report(process, compiler);

    let timed_executable = directory.join(crate::native_toolchain::executable_name("peer-timed"));
    let timed_linker_map = directory.join("peer-timed.map");

    let timed = rust_configuration(
        &source_path,
        &timed_executable,
        &timed_linker_map,
        target,
        source.selector,
        &linker,
        Some(controlled_inner_iterations),
    )?;

    run_compiler("rustc", &timed, "building Rust performance peer")?;

    Ok(BuiltPeer {
        language: PeerLanguage::Rust,
        toolchain: command_identity(Command::new("rustc").arg("--version"), "rustc")?,
        build_configuration: PeerBuildConfiguration {
            target: target.as_str().to_owned(),
            compiler: "rustc".to_owned(),
            production,
            timed,
            linker: crate::path::slash_separated(&linker),
            runtime_linkage: runtime_linkage(target)?,
            post_link_actions: vec!["rustc strips symbols during linking".to_owned()],
        },
        source_sha256: super::super::compilation::source_digest(source.contents),
        compilation,
        executable,
        linker_map,
        timed_executable,
    })
}

fn rust_configuration(
    source: &Path,
    executable: &Path,
    linker_map: &Path,
    target: NativeTarget,
    workload: &str,
    linker: &Path,
    controlled_inner_iterations: Option<NonZeroU64>,
) -> Result<PeerCompilerConfiguration, String> {
    let mut arguments = rust_executable_arguments(source, target, linker);

    arguments.extend(["--cfg".to_owned(), format!("peer_workload=\"{workload}\"")]);

    let mut environment = BTreeMap::new();

    if let Some(inner_iterations) = controlled_inner_iterations {
        arguments.extend(["--cfg".to_owned(), "peer_timing".to_owned()]);

        environment.insert(
            RUST_INNER_ITERATIONS_ENVIRONMENT.to_owned(),
            inner_iterations.get().to_string(),
        );
    }

    append_rust_linker_map(&mut arguments, target.object_format(), linker_map)?;

    arguments.extend(["-o".to_owned(), crate::path::slash_separated(executable)]);

    Ok(PeerCompilerConfiguration {
        arguments,
        environment,
        batching: peer_batching(controlled_inner_iterations),
    })
}
pub(in crate::performance) fn rust_release_arguments(
    source: &Path,
    target: NativeTarget,
) -> Vec<String> {
    vec![
        crate::path::slash_separated(source),
        "--edition".to_owned(),
        "2024".to_owned(),
        "--target".to_owned(),
        target.as_str().to_owned(),
        "-C".to_owned(),
        "opt-level=3".to_owned(),
        "-C".to_owned(),
        "debuginfo=0".to_owned(),
        "-C".to_owned(),
        "panic=abort".to_owned(),
        "-C".to_owned(),
        "codegen-units=1".to_owned(),
        "-C".to_owned(),
        "lto=off".to_owned(),
    ]
}

pub(in crate::performance) fn rust_executable_arguments(
    source: &Path,
    target: NativeTarget,
    linker: &Path,
) -> Vec<String> {
    let mut arguments = rust_release_arguments(source, target);

    arguments.extend([
        "-C".to_owned(),
        "strip=symbols".to_owned(),
        "-C".to_owned(),
        format!("linker={}", crate::path::slash_separated(linker)),
    ]);

    if target.object_format() == ObjectFormat::Coff {
        arguments.extend(["-C".to_owned(), "target-feature=+crt-static".to_owned()]);
    }

    arguments
}

pub(in crate::performance) fn rust_linker(root: &Path, target: NativeTarget) -> PathBuf {
    let name = match target.object_format() {
        ObjectFormat::Coff => "lld-link",
        ObjectFormat::Elf => "ld.lld",
        ObjectFormat::MachO => "ld64.lld",
        ObjectFormat::WebAssembly | ObjectFormat::Xcoff => "ld.lld",
    };

    bray_llvm_toolchain::tool_path(root, name)
}

fn build_cpp(
    root: &Path,
    output: &Path,
    target: NativeTarget,
    source: PeerSource,
    controlled_inner_iterations: NonZeroU64,
) -> Result<BuiltPeer, String> {
    let directory = output.join("cpp");

    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("could not create {}: {error}", directory.display()))?;

    let source_path = directory.join("peer.cpp");

    std::fs::write(&source_path, source.contents)
        .map_err(|error| format!("could not write {}: {error}", source_path.display()))?;

    let clang =
        bray_tooling::llvm_tool_path(bray_diagnostics::DiagnosticLlvmToolRole::CompilerDriver)
            .unwrap_or_else(|_| bray_llvm_toolchain::tool_path(root, "clang"));

    let executable = directory.join(crate::native_toolchain::executable_name("peer"));
    let linker_map = directory.join("peer.map");

    let production = cpp_configuration(
        &source_path,
        &executable,
        &linker_map,
        target,
        source.selector,
        None,
    )?;

    let compiler = crate::path::slash_separated(&clang);

    let process = super::super::compiler_timing::process(
        &compiler,
        &production,
        "building C++ performance peer",
    )?;

    let profiled_executable =
        directory.join(crate::native_toolchain::executable_name("peer-profiled"));

    let profiled_linker_map = directory.join("peer-profiled.map");
    let compiler_trace = directory.join("clang.json");
    let linker_trace = directory.join("lld.json");

    let mut profiled = cpp_configuration(
        &source_path,
        &profiled_executable,
        &profiled_linker_map,
        target,
        source.selector,
        None,
    )?;

    profiled.arguments.extend([
        format!(
            "-ftime-trace={}",
            crate::path::slash_separated(&compiler_trace)
        ),
        format!(
            "-Wl,--time-trace={}",
            crate::path::slash_separated(&linker_trace)
        ),
    ]);

    let compiler_work = super::super::compiler_timing::cpp(
        &compiler,
        &profiled,
        &compiler_trace,
        &linker_trace,
        "profiling C++ performance peer compilation",
    )?;

    let compilation = super::super::compiler_timing::report(process, compiler_work);

    strip_cpp_artifact(root, &executable)?;

    let timed_executable = directory.join(crate::native_toolchain::executable_name("peer-timed"));
    let timed_linker_map = directory.join("peer-timed.map");

    let timed = cpp_configuration(
        &source_path,
        &timed_executable,
        &timed_linker_map,
        target,
        source.selector,
        Some(controlled_inner_iterations),
    )?;

    run_compiler(&compiler, &timed, "building C++ performance peer")?;

    strip_cpp_artifact(root, &timed_executable)?;

    Ok(BuiltPeer {
        language: PeerLanguage::Cpp,
        toolchain: command_identity(Command::new(&clang).arg("--version"), "clang")?,
        build_configuration: PeerBuildConfiguration {
            target: target.as_str().to_owned(),
            compiler,
            production,
            timed,
            linker: "lld selected through the clang++ driver".to_owned(),
            runtime_linkage: runtime_linkage(target)?,
            post_link_actions: vec!["llvm-strip --strip-all".to_owned()],
        },
        source_sha256: super::super::compilation::source_digest(source.contents),
        compilation,
        executable,
        linker_map,
        timed_executable,
    })
}

fn cpp_configuration(
    source: &Path,
    executable: &Path,
    linker_map: &Path,
    target: NativeTarget,
    selector: &str,
    controlled_inner_iterations: Option<NonZeroU64>,
) -> Result<PeerCompilerConfiguration, String> {
    let mut arguments = cpp_executable_arguments(target);

    arguments.push(format!("-DBRAY_WORKLOAD={selector}"));

    if selector == "14" && target.object_format() == ObjectFormat::Elf {
        arguments.push("-pthread".to_owned());
    }

    if let Some(inner_iterations) = controlled_inner_iterations {
        arguments.push("-DBRAY_PEER_TIMING".to_owned());

        arguments.push(format!(
            "-DBRAY_INNER_ITERATIONS={}ULL",
            inner_iterations.get()
        ));
    }

    append_cpp_linker_map(&mut arguments, target.object_format(), linker_map)?;

    arguments.extend([
        crate::path::slash_separated(source),
        "-o".to_owned(),
        crate::path::slash_separated(executable),
    ]);

    Ok(PeerCompilerConfiguration {
        arguments,
        environment: BTreeMap::new(),
        batching: peer_batching(controlled_inner_iterations),
    })
}

pub(in crate::performance) fn cpp_release_arguments(target: NativeTarget) -> Vec<String> {
    vec![
        "--driver-mode=g++".to_owned(),
        "-std=c++20".to_owned(),
        "-O3".to_owned(),
        "-DNDEBUG".to_owned(),
        "-fno-exceptions".to_owned(),
        "-fno-rtti".to_owned(),
        format!("--target={}", target.as_str()),
    ]
}

pub(in crate::performance) fn cpp_executable_arguments(target: NativeTarget) -> Vec<String> {
    let mut arguments = cpp_release_arguments(target);

    arguments.push("-fuse-ld=lld".to_owned());

    if target.object_format() == ObjectFormat::Coff {
        arguments.push("-fms-runtime-lib=static".to_owned());

        if target.as_str().starts_with("x86_64-") {
            arguments.push("-D_AMD64_".to_owned());
        }
    } else if target.object_format() == ObjectFormat::Elf {
        arguments.extend(["-static-libstdc++".to_owned(), "-static-libgcc".to_owned()]);
    }

    arguments
}

fn strip_cpp_artifact(root: &Path, executable: &Path) -> Result<(), String> {
    let strip = bray_llvm_toolchain::tool_path(root, "llvm-strip");
    let mut command = Command::new(strip);

    command.args(["--strip-all"]).arg(executable);

    crate::command::require_success(command, "stripping C++ performance peer").map(|_| ())
}

pub(in crate::performance) fn runtime_linkage(
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

pub(in crate::performance) fn rebuild_timed(
    root: &Path,
    target: NativeTarget,
    workload: &str,
    inner_iterations: NonZeroU64,
    built: &mut [BuiltPeer],
) -> Result<(), String> {
    let sources = sources(workload)?;

    for peer in built {
        let source = sources
            .iter()
            .find(|source| source.language == peer.language)
            .ok_or_else(|| format!("{workload} is missing its {:?} source", peer.language))?;

        let directory = peer
            .executable
            .parent()
            .ok_or_else(|| "peer executable has no parent directory".to_owned())?;

        let linker_map = directory.join("peer-timed.map");

        peer.build_configuration.timed = match peer.language {
            PeerLanguage::Rust => {
                let configuration = rust_configuration(
                    &directory.join("peer.rs"),
                    &peer.timed_executable,
                    &linker_map,
                    target,
                    source.selector,
                    Path::new(&peer.build_configuration.linker),
                    Some(inner_iterations),
                )?;

                run_compiler(
                    &peer.build_configuration.compiler,
                    &configuration,
                    "building Rust performance peer",
                )?;

                configuration
            }
            PeerLanguage::Cpp => {
                let configuration = cpp_configuration(
                    &directory.join("peer.cpp"),
                    &peer.timed_executable,
                    &linker_map,
                    target,
                    source.selector,
                    Some(inner_iterations),
                )?;

                run_compiler(
                    &peer.build_configuration.compiler,
                    &configuration,
                    "building C++ performance peer",
                )?;

                strip_cpp_artifact(root, &peer.timed_executable)?;

                configuration
            }
        };
    }

    Ok(())
}

pub(in crate::performance) fn build_configuration_matches(
    language: PeerLanguage,
    workload: &str,
    target: NativeTarget,
    production_executable: &Path,
    configuration: &PeerBuildConfiguration,
    inner_iterations: NonZeroU64,
) -> bool {
    let Ok(sources) = sources(workload) else {
        return false;
    };

    let Some(source) = sources.iter().find(|source| source.language == language) else {
        return false;
    };

    let Some(directory) = production_executable.parent() else {
        return false;
    };

    let timed_executable = directory.join(crate::native_toolchain::executable_name("peer-timed"));
    let production_map = directory.join("peer.map");
    let timed_map = directory.join("peer-timed.map");

    let expected = match language {
        PeerLanguage::Rust => {
            let linker = Path::new(&configuration.linker);

            let Ok(production) = rust_configuration(
                &directory.join("peer.rs"),
                production_executable,
                &production_map,
                target,
                source.selector,
                linker,
                None,
            ) else {
                return false;
            };

            let Ok(timed) = rust_configuration(
                &directory.join("peer.rs"),
                &timed_executable,
                &timed_map,
                target,
                source.selector,
                linker,
                Some(inner_iterations),
            ) else {
                return false;
            };

            (production, timed, "rustc strips symbols during linking")
        }
        PeerLanguage::Cpp => {
            let Ok(production) = cpp_configuration(
                &directory.join("peer.cpp"),
                production_executable,
                &production_map,
                target,
                source.selector,
                None,
            ) else {
                return false;
            };

            let Ok(timed) = cpp_configuration(
                &directory.join("peer.cpp"),
                &timed_executable,
                &timed_map,
                target,
                source.selector,
                Some(inner_iterations),
            ) else {
                return false;
            };

            (production, timed, "llvm-strip --strip-all")
        }
    };

    configuration.target == target.as_str()
        && configuration.production == expected.0
        && configuration.timed == expected.1
        && configuration.post_link_actions == [expected.2]
        && match language {
            PeerLanguage::Rust => configuration.compiler == "rustc",
            PeerLanguage::Cpp => configuration.linker == "lld selected through the clang++ driver",
        }
}

pub(in crate::performance) fn run_compiler(
    program: impl AsRef<std::ffi::OsStr>,
    configuration: &PeerCompilerConfiguration,
    description: &str,
) -> Result<(), String> {
    let mut command = Command::new(program);

    command
        .args(&configuration.arguments)
        .envs(&configuration.environment);

    crate::command::require_success(command, description).map(|_| ())
}

fn peer_batching(inner_iterations: Option<NonZeroU64>) -> PeerBatching {
    inner_iterations.map_or(PeerBatching::SingleExecution, |inner_iterations| {
        PeerBatching::Repeated {
            inner_iterations: inner_iterations.get(),
        }
    })
}

#[cfg(test)]
pub(in crate::performance) fn fixture_build_configuration(
    language: PeerLanguage,
    workload: &str,
    target: NativeTarget,
    production_executable: &Path,
    inner_iterations: NonZeroU64,
) -> PeerBuildConfiguration {
    let source = sources(workload)
        .unwrap_or_else(|error| panic!("fixture workload must have peer sources: {error}"))
        .into_iter()
        .find(|source| source.language == language)
        .unwrap_or_else(|| panic!("fixture workload must have a {language:?} source"));

    let directory = production_executable
        .parent()
        .unwrap_or_else(|| panic!("fixture peer executable must have a parent"));

    let timed_executable = directory.join(crate::native_toolchain::executable_name("peer-timed"));
    let production_map = directory.join("peer.map");
    let timed_map = directory.join("peer-timed.map");

    match language {
        PeerLanguage::Rust => {
            let linker = rust_linker(Path::new("workspace"), target);

            PeerBuildConfiguration {
                target: target.as_str().to_owned(),
                compiler: "rustc".to_owned(),
                production: rust_configuration(
                    &directory.join("peer.rs"),
                    production_executable,
                    &production_map,
                    target,
                    source.selector,
                    &linker,
                    None,
                )
                .unwrap_or_else(|error| panic!("fixture Rust configuration must build: {error}")),
                timed: rust_configuration(
                    &directory.join("peer.rs"),
                    &timed_executable,
                    &timed_map,
                    target,
                    source.selector,
                    &linker,
                    Some(inner_iterations),
                )
                .unwrap_or_else(|error| panic!("fixture Rust configuration must build: {error}")),
                linker: crate::path::slash_separated(&linker),
                runtime_linkage: RuntimeLinkage::StaticApplicationRuntime,
                post_link_actions: vec!["rustc strips symbols during linking".to_owned()],
            }
        }
        PeerLanguage::Cpp => PeerBuildConfiguration {
            target: target.as_str().to_owned(),
            compiler: "clang".to_owned(),
            production: cpp_configuration(
                &directory.join("peer.cpp"),
                production_executable,
                &production_map,
                target,
                source.selector,
                None,
            )
            .unwrap_or_else(|error| panic!("fixture C++ configuration must build: {error}")),
            timed: cpp_configuration(
                &directory.join("peer.cpp"),
                &timed_executable,
                &timed_map,
                target,
                source.selector,
                Some(inner_iterations),
            )
            .unwrap_or_else(|error| panic!("fixture C++ configuration must build: {error}")),
            linker: "lld selected through the clang++ driver".to_owned(),
            runtime_linkage: RuntimeLinkage::StaticApplicationRuntime,
            post_link_actions: vec!["llvm-strip --strip-all".to_owned()],
        },
    }
}

pub(in crate::performance) fn append_rust_linker_map(
    arguments: &mut Vec<String>,
    format: ObjectFormat,
    map: &Path,
) -> Result<(), String> {
    let map = crate::path::slash_separated(map);

    let argument = match format {
        ObjectFormat::Coff => format!("/MAP:{map}"),
        ObjectFormat::Elf => format!("-Wl,-Map,{map}"),
        ObjectFormat::MachO => format!("-Wl,-map,{map}"),
        ObjectFormat::WebAssembly | ObjectFormat::Xcoff => {
            return Err("performance peers do not support the selected object format".to_owned());
        }
    };

    arguments.extend(["-C".to_owned(), format!("link-arg={argument}")]);

    Ok(())
}

pub(in crate::performance) fn append_cpp_linker_map(
    arguments: &mut Vec<String>,
    format: ObjectFormat,
    map: &Path,
) -> Result<(), String> {
    let map = crate::path::slash_separated(map);

    match format {
        ObjectFormat::Coff => arguments.push(format!("-Wl,/MAP:{map}")),
        ObjectFormat::Elf => arguments.push(format!("-Wl,-Map,{map}")),
        ObjectFormat::MachO => arguments.push(format!("-Wl,-map,{map}")),
        ObjectFormat::WebAssembly | ObjectFormat::Xcoff => {
            return Err("performance peers do not support the selected object format".to_owned());
        }
    }

    Ok(())
}

pub(in crate::performance) fn command_identity(
    command: &mut Command,
    tool: &str,
) -> Result<String, String> {
    let output = command
        .output()
        .map_err(|error| format!("could not run {tool}: {error}"))?;

    if !output.status.success() {
        return Err(format!("{tool} did not report its identity"));
    }

    let identity = String::from_utf8_lossy(&output.stdout);

    Ok(identity
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned())
}

pub(in crate::performance) fn elapsed_nanoseconds(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use bray_target::NativeTarget;

    use super::{
        PeerBatching, PeerLanguage, build_configuration_matches, cpp_configuration,
        fixture_build_configuration, rust_configuration, rust_linker,
    };

    #[test]
    #[cfg(windows)]
    fn normalized_artifact_paths_revalidate_exact_peer_invocations() {
        let target = NativeTarget::X86_64WindowsMsvc;

        let inner_iterations = std::num::NonZeroU64::new(50_000_000)
            .unwrap_or_else(|| panic!("fixture inner iteration count must be nonzero"));

        for (language, directory) in [(PeerLanguage::Rust, "rust"), (PeerLanguage::Cpp, "cpp")] {
            let native_path = format!(r"C:\work\peers\{directory}\peer.exe");
            let report_path = format!("C:/work/peers/{directory}/peer.exe");

            let configuration = fixture_build_configuration(
                language,
                "small_output",
                target,
                std::path::Path::new(&native_path),
                inner_iterations,
            );

            assert!(build_configuration_matches(
                language,
                "small_output",
                target,
                std::path::Path::new(&report_path),
                &configuration,
                inner_iterations,
            ));
        }
    }

    #[test]
    fn windows_peers_both_embed_the_static_msvc_runtime() {
        let target = NativeTarget::X86_64WindowsMsvc;
        let linker = rust_linker(std::path::Path::new("workspace"), target);

        let rust = rust_configuration(
            std::path::Path::new("peer.rs"),
            std::path::Path::new("peer.exe"),
            std::path::Path::new("peer.map"),
            target,
            "small_output",
            &linker,
            None,
        )
        .unwrap_or_else(|error| panic!("Rust fixture configuration must build: {error}"));

        let cpp = cpp_configuration(
            std::path::Path::new("peer.cpp"),
            std::path::Path::new("peer.exe"),
            std::path::Path::new("peer.map"),
            target,
            "1",
            None,
        )
        .unwrap_or_else(|error| panic!("C++ fixture configuration must build: {error}"));

        assert_eq!(
            linker.file_name().and_then(std::ffi::OsStr::to_str),
            Some(if cfg!(windows) {
                "lld-link.exe"
            } else {
                "lld-link"
            })
        );

        assert!(
            rust.arguments
                .iter()
                .any(|argument| argument == "target-feature=+crt-static")
        );

        assert!(
            cpp.arguments
                .iter()
                .any(|argument| argument == "-fms-runtime-lib=static")
        );

        assert!(
            !rust
                .arguments
                .iter()
                .any(|argument| argument.contains("-crt-static"))
        );

        assert!(
            !cpp.arguments
                .iter()
                .any(|argument| argument.contains("runtime-lib=dll"))
        );
    }

    #[test]
    fn linux_contended_output_enables_native_thread_support() {
        let target = NativeTarget::X86_64LinuxGnu;

        let contended = cpp_configuration(
            std::path::Path::new("peer.cpp"),
            std::path::Path::new("peer"),
            std::path::Path::new("peer.map"),
            target,
            "14",
            None,
        )
        .unwrap_or_else(|error| panic!("C++ fixture configuration must build: {error}"));

        let sequential = cpp_configuration(
            std::path::Path::new("peer.cpp"),
            std::path::Path::new("peer"),
            std::path::Path::new("peer.map"),
            target,
            "9",
            None,
        )
        .unwrap_or_else(|error| panic!("C++ fixture configuration must build: {error}"));

        assert!(
            contended
                .arguments
                .iter()
                .any(|argument| argument == "-pthread")
        );

        assert!(
            !sequential
                .arguments
                .iter()
                .any(|argument| argument == "-pthread")
        );
    }

    #[test]
    fn production_peer_arguments_exclude_timing_batches() {
        let target = NativeTarget::X86_64WindowsMsvc;
        let linker = rust_linker(std::path::Path::new("workspace"), target);

        let rust = rust_configuration(
            std::path::Path::new("peer.rs"),
            std::path::Path::new("peer.exe"),
            std::path::Path::new("peer.map"),
            target,
            "small_output",
            &linker,
            None,
        )
        .unwrap_or_else(|error| panic!("Rust fixture configuration must build: {error}"));

        let cpp = cpp_configuration(
            std::path::Path::new("peer.cpp"),
            std::path::Path::new("peer.exe"),
            std::path::Path::new("peer.map"),
            target,
            "1",
            None,
        )
        .unwrap_or_else(|error| panic!("C++ fixture configuration must build: {error}"));

        assert!(
            !rust
                .arguments
                .iter()
                .any(|argument| argument == "peer_timing")
        );

        assert!(
            !cpp.arguments
                .iter()
                .any(|argument| argument.contains("BRAY_INNER_ITERATIONS"))
        );

        assert!(rust.environment.is_empty());
        assert!(cpp.environment.is_empty());
        assert_eq!(rust.batching, PeerBatching::SingleExecution);
        assert_eq!(cpp.batching, PeerBatching::SingleExecution);
    }

    #[test]
    fn timed_peer_arguments_record_the_inner_iteration_count() {
        let target = NativeTarget::X86_64WindowsMsvc;
        let linker = rust_linker(std::path::Path::new("workspace"), target);

        let inner_iterations = std::num::NonZeroU64::new(50_000_000)
            .unwrap_or_else(|| panic!("fixture inner iteration count must be nonzero"));

        let rust = rust_configuration(
            std::path::Path::new("peer.rs"),
            std::path::Path::new("peer-timed.exe"),
            std::path::Path::new("peer-timed.map"),
            target,
            "small_output",
            &linker,
            Some(inner_iterations),
        )
        .unwrap_or_else(|error| panic!("Rust fixture configuration must build: {error}"));

        let cpp = cpp_configuration(
            std::path::Path::new("peer.cpp"),
            std::path::Path::new("peer-timed.exe"),
            std::path::Path::new("peer-timed.map"),
            target,
            "1",
            Some(inner_iterations),
        )
        .unwrap_or_else(|error| panic!("C++ fixture configuration must build: {error}"));

        assert!(
            rust.arguments
                .iter()
                .any(|argument| argument == "peer_timing")
        );

        assert_eq!(
            rust.environment
                .get(super::RUST_INNER_ITERATIONS_ENVIRONMENT),
            Some(&"50000000".to_owned())
        );

        assert!(
            cpp.arguments
                .iter()
                .any(|argument| argument == "-DBRAY_INNER_ITERATIONS=50000000ULL")
        );

        assert_eq!(
            rust.batching,
            PeerBatching::Repeated {
                inner_iterations: 50_000_000
            }
        );

        assert_eq!(rust.batching, cpp.batching);
    }
}
