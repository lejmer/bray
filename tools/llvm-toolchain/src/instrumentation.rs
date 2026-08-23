use std::fmt;
use std::fs;
use std::hash::Hasher;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_base::{StableDigestHasher, lowercase_hex};

use crate::process::output_detail;

const BUILD_DIRECTORY: &str = "bray-lld-build";
const SOURCE_DIRECTORY: &str = "source";
const BUILD_INPUTS: [&[u8]; 5] = [
    include_bytes!("instrumentation.rs"),
    include_bytes!("../native/lld/main.cpp"),
    include_bytes!("../native/lld/manifest.cpp"),
    include_bytes!("../native/lld/telemetry.cpp"),
    include_bytes!("../native/lld/telemetry.h"),
];

pub(crate) fn digest() -> String {
    let mut digest = StableDigestHasher::new();

    for input in BUILD_INPUTS {
        digest.write_usize(input.len());
        digest.write(input);
    }

    lowercase_hex(&digest.finalize())
}

pub(crate) fn identity(version: &str, source_digest: &str) -> String {
    let instrumentation = digest();
    let mut identity = StableDigestHasher::new();

    for component in [version, source_digest, instrumentation.as_str()] {
        identity.write_usize(component.len());
        identity.write(component.as_bytes());
    }

    lowercase_hex(&identity.finalize())
}

pub(crate) fn install(
    toolchain: &Path,
    source_archive: &Path,
    version: &str,
    identity: &str,
    native_sources: &Path,
) -> Result<(), InstrumentationError> {
    let build = toolchain.join(BUILD_DIRECTORY);

    if build.exists() {
        fs::remove_dir_all(&build)
            .map_err(|error| InstrumentationError::io("remove", &build, error))?;
    }

    fs::create_dir_all(&build)
        .map_err(|error| InstrumentationError::io("create", &build, error))?;

    let result = prepare_sources(&build, source_archive, version)
        .and_then(|source| build_linker(toolchain, &source, native_sources, &build, identity));

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

fn prepare_sources(
    build: &Path,
    archive: &Path,
    version: &str,
) -> Result<PathBuf, InstrumentationError> {
    let source = build.join(SOURCE_DIRECTORY);

    fs::create_dir(&source).map_err(|error| InstrumentationError::io("create", &source, error))?;

    run(
        Command::new("tar")
            .arg("-xJf")
            .arg(archive)
            .arg("--directory")
            .arg(&source)
            .arg("--strip-components")
            .arg("1")
            .arg(format!("llvm-project-{version}.src/lld")),
        "tar",
    )?;

    Ok(source)
}

fn build_linker(
    toolchain: &Path,
    source: &Path,
    native_sources: &Path,
    build: &Path,
    identity: &str,
) -> Result<(), InstrumentationError> {
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

    let mut objects = Vec::new();

    for (name, source_path) in [
        ("main", native_sources.join("main.cpp")),
        ("telemetry", native_sources.join("telemetry.cpp")),
        ("lto", lto_source),
    ] {
        objects.push(compile(
            toolchain,
            source,
            native_sources,
            build,
            name,
            &source_path,
            identity,
        )?);
    }

    if cfg!(windows) {
        objects.push(compile(
            toolchain,
            source,
            native_sources,
            build,
            "manifest",
            &native_sources.join("manifest.cpp"),
            identity,
        )?);
    }

    link(toolchain, build, flavor, &objects)
}

fn instrument_lto_source(path: &Path) -> Result<(), InstrumentationError> {
    let source =
        fs::read_to_string(path).map_err(|error| InstrumentationError::io("read", path, error))?;

    let source = replace_once(
        source,
        "#include \"LTO.h\"\n",
        "#include \"LTO.h\"\n#include \"telemetry.h\"\n",
        path,
    )?;

    let source = replace_once(
        source,
        "  return c;\n}",
        "  bray::lld::configure_telemetry(c);\n\n  return c;\n}",
        path,
    )?;

    let source = replace_once(
        source,
        "localCache(\"ThinLTO\", \"Thin\", ",
        "bray::lld::telemetry_cache(",
        path,
    )?;

    fs::write(path, source).map_err(|error| InstrumentationError::io("write", path, error))
}

fn replace_once(
    source: String,
    expected: &str,
    replacement: &str,
    path: &Path,
) -> Result<String, InstrumentationError> {
    if source.match_indices(expected).count() != 1 {
        return Err(InstrumentationError::SourceContract(path.to_path_buf()));
    }

    Ok(source.replacen(expected, replacement, 1))
}

fn compile(
    toolchain: &Path,
    source_root: &Path,
    native_sources: &Path,
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
        command.args(["/nologo", "/std:c++17", "/O2", "/DNDEBUG", "/EHsc", "/c"]);
        command.arg(format!("/DBRAY_LLD_TOOLCHAIN_IDENTITY=\"{identity}\""));
        command.arg(source);

        for include in include_directories(toolchain, source_root, native_sources) {
            command.arg(format!("/I{}", include.display()));
        }

        command.arg(format!("/Fo{}", object.display()));
    } else {
        command.args([
            "-std=c++17",
            "-O2",
            "-DNDEBUG",
            "-fno-exceptions",
            "-fno-rtti",
            "-c",
        ]);

        command.arg(format!("-DBRAY_LLD_TOOLCHAIN_IDENTITY=\"{identity}\""));

        command.arg(source);

        for include in include_directories(toolchain, source_root, native_sources) {
            command.arg("-I").arg(include);
        }

        command.arg("-o").arg(&object);
    }

    run(&mut command, compiler)?;

    Ok(object)
}

fn include_directories(toolchain: &Path, source: &Path, native_sources: &Path) -> [PathBuf; 4] {
    [
        native_sources.to_path_buf(),
        source.join("lld"),
        source.join("lld/include"),
        toolchain.join("include"),
    ]
}

fn link(
    toolchain: &Path,
    build: &Path,
    flavor: HostFlavor,
    objects: &[PathBuf],
) -> Result<(), InstrumentationError> {
    let compiler = if cfg!(windows) { "clang-cl" } else { "clang++" };
    let output = build.join(executable(flavor.executable()));
    let library_directory = toolchain.join("lib");
    let mut command = Command::new(tool(toolchain, compiler));

    if cfg!(windows) {
        command.arg("/nologo");
    }

    command.args(objects);
    command.arg(library_directory.join(flavor.library()));

    command.arg(library_directory.join(if cfg!(windows) {
        "lldCommon.lib"
    } else {
        "liblldCommon.a"
    }));

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
        return Err(InstrumentationError::MissingOutput(output));
    }

    publish_linker(
        &output,
        &toolchain.join("bin").join(executable(flavor.executable())),
    )
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

fn run(command: &mut Command, program: &'static str) -> Result<(), InstrumentationError> {
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
    fn io(action: &'static str, path: &Path, error: io::Error) -> Self {
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
                "pinned LLD source does not match the instrumentation contract at {}",
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
    use super::{HostFlavor, InstrumentationError, digest, identity, instrument_lto_source};

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
        assert_ne!(identity, super::identity("22.1.9", &source));
        assert_ne!(identity, super::identity("22.1.8", &"b".repeat(64)));
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
