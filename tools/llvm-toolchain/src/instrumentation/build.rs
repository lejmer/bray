use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::process::output_detail;

use super::source::{
    instrument_instruction_sinking_source, instrument_lto_source, prepare_sources,
    write_native_sources,
};

const BUILD_DIRECTORY: &str = "bray-lld-build";
pub(crate) fn install(
    toolchain: &Path,
    source_archive: &Path,
    version: &str,
    identity: &str,
) -> Result<(), InstrumentationError> {
    let build = toolchain.join(BUILD_DIRECTORY);

    if build.exists() {
        fs::remove_dir_all(&build)
            .map_err(|error| InstrumentationError::io("remove", &build, error))?;
    }

    fs::create_dir_all(&build)
        .map_err(|error| InstrumentationError::io("create", &build, error))?;

    let result = prepare_sources(&build, source_archive, version)
        .and_then(|source| build_linker(toolchain, &source, &build, identity));

    let cleanup = fs::remove_dir_all(&build)
        .map_err(|error| InstrumentationError::io("remove", &build, error));

    match (result, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Err(installation), Err(cleanup)) => Err(InstrumentationError::Cleanup {
            installation: Box::new(installation),
            cleanup: Box::new(cleanup),
        }),
    }
}

fn build_linker(
    toolchain: &Path,
    source: &Path,
    build: &Path,
    identity: &str,
) -> Result<(), InstrumentationError> {
    write_native_sources(build)?;

    let assembly_writer = source.join("llvm/lib/IR/AsmWriter.cpp");

    let mut writer_source = fs::read_to_string(&assembly_writer)
        .map_err(|error| InstrumentationError::io("read", &assembly_writer, error))?;

    writer_source.push_str("\n#include \"definition_sizes.cpp\"\n");

    fs::write(&assembly_writer, writer_source)
        .map_err(|error| InstrumentationError::io("write", &assembly_writer, error))?;

    let flavor = host_flavor();
    let flavor_source = source.join("lld").join(flavor.source_directory());
    let options = flavor_source.join("Options.inc");
    let include = toolchain.join("include");

    for candidate in HostFlavor::ALL {
        instrument_lto_source(
            &source
                .join("lld")
                .join(candidate.source_directory())
                .join("LTO.cpp"),
        )?;
    }

    run(
        Command::new(tool(toolchain, "llvm-tblgen"))
            .arg("-I")
            .arg(&flavor_source)
            .arg("-I")
            .arg(&include)
            .arg("-gen-opt-parser-defs")
            .arg(flavor_source.join("Options.td"))
            .arg("-o")
            .arg(options),
        "llvm-tblgen",
    )?;

    let lto_source = flavor_source.join("LTO.cpp");
    let combining_source = source.join("llvm/lib/Transforms/InstCombine/InstructionCombining.cpp");

    instrument_instruction_sinking_source(&combining_source)?;

    let writer_object = compile(
        toolchain,
        source,
        build,
        "assembly_writer",
        &assembly_writer,
        identity,
    )?;

    let mut objects = Vec::new();

    for (name, source_path) in [
        ("main", build.join("main.cpp")),
        ("telemetry", build.join("telemetry.cpp")),
        ("lto", lto_source),
        ("instruction_combining", combining_source),
    ] {
        objects.push(compile(
            toolchain,
            source,
            build,
            name,
            &source_path,
            identity,
        )?);
    }

    run_native_tests(toolchain, source, build, identity, &writer_object)?;
    objects.push(writer_object);

    if cfg!(windows) {
        objects.push(compile(
            toolchain,
            source,
            build,
            "manifest",
            &build.join("manifest.cpp"),
            identity,
        )?);
    }

    link(toolchain, build, flavor, &objects)
}

fn compile(
    toolchain: &Path,
    source_root: &Path,
    build: &Path,
    name: &str,
    source: &Path,
    identity: &str,
) -> Result<PathBuf, InstrumentationError> {
    let extension = if cfg!(windows) { "obj" } else { "o" };
    let object = build.join(format!("{name}.{extension}"));
    let compiler = if cfg!(windows) { "clang-cl" } else { "clang++" };
    let mut command = Command::new(tool(toolchain, compiler));

    if cfg!(windows) {
        command.args([
            "/nologo",
            "/std:c++17",
            "/O2",
            "/DNDEBUG",
            "/DLLVM_BUILD_STATIC",
            "/EHsc",
            "/c",
        ]);

        command.arg(format!("/DBRAY_LLD_TOOLCHAIN_IDENTITY=\"{identity}\""));
        command.arg(source);

        for include in include_directories(toolchain, source_root, build) {
            command.arg(format!("/I{}", include.display()));
        }

        command.arg(format!("/Fo{}", object.display()));
    } else {
        command.args([
            "-std=c++17",
            "-O2",
            "-DNDEBUG",
            "-DLLVM_BUILD_STATIC",
            "-fno-exceptions",
            "-fno-rtti",
            "-c",
        ]);

        command.arg(format!("-DBRAY_LLD_TOOLCHAIN_IDENTITY=\"{identity}\""));

        command.arg(source);

        for include in include_directories(toolchain, source_root, build) {
            command.arg("-I").arg(include);
        }

        command.arg("-o").arg(&object);
    }

    run(&mut command, compiler)?;

    Ok(object)
}

fn run_native_tests(
    toolchain: &Path,
    source: &Path,
    build: &Path,
    identity: &str,
    writer_object: &Path,
) -> Result<(), InstrumentationError> {
    for (name, stem) in [
        ("definition-sizes-test", "definition_sizes_test"),
        ("instruction-sinking-test", "instruction_sinking_test"),
    ] {
        let object = compile(
            toolchain,
            source,
            build,
            stem,
            &build.join(format!("{stem}.cpp")),
            identity,
        )?;

        let binary = build.join(executable(name));

        link_objects(
            toolchain,
            &binary,
            None,
            &[writer_object.to_path_buf(), object],
        )?;

        run(&mut Command::new(&binary), name)?;
    }

    Ok(())
}

fn include_directories(toolchain: &Path, source: &Path, build: &Path) -> [PathBuf; 5] {
    [
        build.to_path_buf(),
        source.join("lld"),
        source.join("lld/include"),
        source.join("llvm/lib/Transforms/InstCombine"),
        toolchain.join("include"),
    ]
}

fn link(
    toolchain: &Path,
    build: &Path,
    flavor: HostFlavor,
    objects: &[PathBuf],
) -> Result<(), InstrumentationError> {
    let output = build.join(executable(flavor.executable()));

    link_objects(toolchain, &output, Some(flavor), objects)?;

    publish_linker(
        &output,
        &toolchain.join("bin").join(executable(flavor.executable())),
    )
}

fn link_objects(
    toolchain: &Path,
    output: &Path,
    flavor: Option<HostFlavor>,
    objects: &[PathBuf],
) -> Result<(), InstrumentationError> {
    let compiler = if cfg!(windows) { "clang-cl" } else { "clang++" };
    let library_directory = toolchain.join("lib");
    let mut command = Command::new(tool(toolchain, compiler));

    if cfg!(windows) {
        command.arg("/nologo");
    }

    command.args(objects);

    if let Some(flavor) = flavor {
        command.arg(library_directory.join(flavor.library()));

        command.arg(library_directory.join(if cfg!(windows) {
            "lldCommon.lib"
        } else {
            "liblldCommon.a"
        }));
    }

    for library in llvm_libraries(toolchain)? {
        if !(cfg!(windows) && library == "LLVMWindowsManifest.lib") {
            command.arg(library_directory.join(library));
        }
    }

    for library in system_libraries(toolchain)? {
        if !(cfg!(windows) && library == "xml2s.lib") {
            command.arg(library);
        }
    }

    if cfg!(windows) {
        command.arg(format!("/Fe{}", output.display()));
        command.args(["/link", "/subsystem:console"]);
    } else {
        command.arg("-o").arg(&output);
    }

    run(&mut command, compiler)?;

    if !output.is_file() {
        return Err(InstrumentationError::MissingOutput(output.to_path_buf()));
    }

    Ok(())
}

fn publish_linker(source: &Path, destination: &Path) -> Result<(), InstrumentationError> {
    if destination.exists() {
        fs::remove_file(destination)
            .map_err(|error| InstrumentationError::io("remove", destination, error))?;
    }

    fs::rename(source, destination)
        .map_err(|error| InstrumentationError::rename(source, destination, error))
}

fn llvm_libraries(toolchain: &Path) -> Result<Vec<String>, InstrumentationError> {
    command_words(
        Command::new(tool(toolchain, "llvm-config")).args(["--link-static", "--libnames", "all"]),
        "llvm-config",
    )
}

fn system_libraries(toolchain: &Path) -> Result<Vec<String>, InstrumentationError> {
    command_words(
        Command::new(tool(toolchain, "llvm-config")).arg("--system-libs"),
        "llvm-config",
    )
}

fn command_words(
    command: &mut Command,
    program: &'static str,
) -> Result<Vec<String>, InstrumentationError> {
    let output = command
        .output()
        .map_err(|error| InstrumentationError::ProcessStart { program, error })?;

    if !output.status.success() {
        return Err(InstrumentationError::ProcessFailed {
            program,
            detail: output_detail(&output),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .map(str::to_owned)
        .collect())
}

pub(super) fn run(
    command: &mut Command,
    program: &'static str,
) -> Result<(), InstrumentationError> {
    let output = command
        .output()
        .map_err(|error| InstrumentationError::ProcessStart { program, error })?;

    if !output.status.success() {
        return Err(InstrumentationError::ProcessFailed {
            program,
            detail: output_detail(&output),
        });
    }

    Ok(())
}

fn tool(toolchain: &Path, name: &str) -> PathBuf {
    toolchain.join("bin").join(executable(name))
}

fn executable(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

const fn host_flavor() -> HostFlavor {
    if cfg!(windows) {
        HostFlavor::Coff
    } else if cfg!(target_os = "macos") {
        HostFlavor::MachO
    } else {
        HostFlavor::Elf
    }
}

#[derive(Clone, Copy)]
enum HostFlavor {
    Coff,
    Elf,
    MachO,
}

impl HostFlavor {
    const ALL: [Self; 3] = [Self::Coff, Self::Elf, Self::MachO];

    const fn source_directory(self) -> &'static str {
        match self {
            Self::Coff => "COFF",
            Self::Elf => "ELF",
            Self::MachO => "MachO",
        }
    }

    const fn executable(self) -> &'static str {
        match self {
            Self::Coff => "lld-link",
            Self::Elf => "ld.lld",
            Self::MachO => "ld64.lld",
        }
    }

    const fn library(self) -> &'static str {
        if cfg!(windows) {
            match self {
                Self::Coff => "lldCOFF.lib",
                Self::Elf => "lldELF.lib",
                Self::MachO => "lldMachO.lib",
            }
        } else {
            match self {
                Self::Coff => "liblldCOFF.a",
                Self::Elf => "liblldELF.a",
                Self::MachO => "liblldMachO.a",
            }
        }
    }
}

#[derive(Debug)]
pub(crate) enum InstrumentationError {
    Io {
        action: &'static str,
        path: PathBuf,
        error: io::Error,
    },
    ProcessStart {
        program: &'static str,
        error: io::Error,
    },
    ProcessFailed {
        program: &'static str,
        detail: String,
    },
    SourceContract(PathBuf),
    MissingOutput(PathBuf),
    Rename {
        source: PathBuf,
        destination: PathBuf,
        error: io::Error,
    },
    Cleanup {
        installation: Box<Self>,
        cleanup: Box<Self>,
    },
}

impl InstrumentationError {
    pub(super) fn io(action: &'static str, path: &Path, error: io::Error) -> Self {
        Self::Io {
            action,
            path: path.to_path_buf(),
            error,
        }
    }

    fn rename(source: &Path, destination: &Path, error: io::Error) -> Self {
        Self::Rename {
            source: source.to_path_buf(),
            destination: destination.to_path_buf(),
            error,
        }
    }
}

impl fmt::Display for InstrumentationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io {
                action,
                path,
                error,
            } => write!(formatter, "failed to {action} {}: {error}", path.display()),
            Self::ProcessStart { program, error } => {
                write!(formatter, "failed to start {program}: {error}")
            }
            Self::ProcessFailed { program, detail } => {
                write!(formatter, "{program} failed: {detail}")
            }
            Self::SourceContract(path) => write!(
                formatter,
                "pinned LLVM/LLD source does not match the instrumentation contract at {}",
                path.display()
            ),
            Self::MissingOutput(path) => write!(
                formatter,
                "instrumented LLD output is missing at {}",
                path.display()
            ),
            Self::Rename {
                source,
                destination,
                error,
            } => write!(
                formatter,
                "failed to rename {} to {}: {error}",
                source.display(),
                destination.display()
            ),
            Self::Cleanup {
                installation,
                cleanup,
            } => write!(
                formatter,
                "instrumented LLD build failed ({installation}) and cleanup failed ({cleanup})"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::source::{digest, identity, instrument_lto_source};
    use super::{HostFlavor, InstrumentationError};

    #[test]
    fn staged_native_sources_match_the_authenticated_inputs() {
        use std::hash::Hasher;

        use bray_base::{StableDigestHasher, lowercase_hex};

        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary directory must be available: {error}"));

        super::write_native_sources(directory.path())
            .unwrap_or_else(|error| panic!("native sources must be staged: {error}"));

        let mut staged_digest = StableDigestHasher::new();

        for implementation in super::super::source::RUST_SOURCES {
            staged_digest.write_usize(implementation.len());
            staged_digest.write(implementation);
        }

        for (name, _) in super::super::source::NATIVE_SOURCES {
            let contents = std::fs::read(directory.path().join(name))
                .unwrap_or_else(|error| panic!("staged {name} must be readable: {error}"));

            staged_digest.write_usize(contents.len());
            staged_digest.write(&contents);
        }

        assert_eq!(lowercase_hex(&staged_digest.finalize()), digest());

        assert_eq!(
            super::include_directories(
                &directory.path().join("toolchain"),
                &directory.path().join("llvm-source"),
                directory.path()
            )[0],
            directory.path()
        );
    }

    #[test]
    fn instrumentation_digest_authenticates_every_build_input() {
        assert_eq!(digest().len(), 64);
        assert!(digest().bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[test]
    fn toolchain_identity_covers_llvm_source_and_instrumentation() {
        let source = "a".repeat(64);
        let identity = identity("22.1.8", &source);

        assert_eq!(identity.len(), 64);
        assert_ne!(identity, super::super::source::identity("22.1.9", &source));

        assert_ne!(
            identity,
            super::super::source::identity("22.1.8", &"b".repeat(64))
        );
    }

    #[test]
    fn every_pinned_lld_flavor_has_one_instrumentation_contract() {
        assert_eq!(
            HostFlavor::ALL.map(HostFlavor::source_directory),
            ["COFF", "ELF", "MachO"]
        );

        assert_eq!(
            HostFlavor::ALL.map(HostFlavor::executable),
            ["lld-link", "ld.lld", "ld64.lld"]
        );
    }

    #[test]
    fn instrumentation_requires_each_pinned_source_anchor_once() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary directory must be available: {error}"));

        let path = directory.path().join("LTO.cpp");

        std::fs::write(
            &path,
            "#include \"LTO.h\"\nlocalCache(\"ThinLTO\", \"Thin\", cache);\n  return c;\n}\n",
        )
        .unwrap_or_else(|error| panic!("test source must be written: {error}"));

        instrument_lto_source(&path)
            .unwrap_or_else(|error| panic!("pinned source must be instrumented: {error}"));

        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("instrumented source must be read: {error}"));

        assert!(source.contains("#include \"telemetry.h\""));
        assert!(source.contains("bray::lld::configure_telemetry(c)"));
        assert!(source.contains("bray::lld::telemetry_cache(cache)"));

        assert!(matches!(
            instrument_lto_source(&path),
            Err(InstrumentationError::SourceContract(_))
        ));
    }
}
