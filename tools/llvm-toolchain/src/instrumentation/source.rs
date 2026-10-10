use std::fs;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_base::{StableDigestHasher, lowercase_hex};

use super::build::{InstrumentationError, run};

const SOURCE_DIRECTORY: &str = "source";

pub(super) const NATIVE_SOURCES: [(&str, &[u8]); 8] = [
    ("main.cpp", include_bytes!("../../native/lld/main.cpp")),
    (
        "manifest.cpp",
        include_bytes!("../../native/lld/manifest.cpp"),
    ),
    (
        "telemetry.cpp",
        include_bytes!("../../native/lld/telemetry.cpp"),
    ),
    (
        "telemetry.h",
        include_bytes!("../../native/lld/telemetry.h"),
    ),
    (
        "definition_sizes.cpp",
        include_bytes!("../../native/lld/definition_sizes.cpp"),
    ),
    (
        "definition_sizes_test.cpp",
        include_bytes!("../../native/lld/definition_sizes_test.cpp"),
    ),
    (
        "instruction_sinking.cpp",
        include_bytes!("../../native/lld/instruction_sinking.cpp"),
    ),
    (
        "instruction_sinking_test.cpp",
        include_bytes!("../../native/lld/instruction_sinking_test.cpp"),
    ),
];

pub(super) const RUST_SOURCES: [&[u8]; 3] = [
    include_bytes!("../instrumentation.rs"),
    include_bytes!("build.rs"),
    include_bytes!("source.rs"),
];

pub(crate) fn digest() -> String {
    let mut digest = StableDigestHasher::new();

    for input in RUST_SOURCES
        .into_iter()
        .chain(NATIVE_SOURCES.map(|(_, contents)| contents))
    {
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

pub(super) fn prepare_sources(
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
            .arg(format!("llvm-project-{version}.src/lld"))
            .arg(format!(
                "llvm-project-{version}.src/llvm/lib/IR/AsmWriter.cpp"
            ))
            .arg(format!("llvm-project-{version}.src/llvm/lib/Transforms/InstCombine/InstructionCombining.cpp"))
            .arg(format!("llvm-project-{version}.src/llvm/lib/Transforms/InstCombine/InstCombineInternal.h")),
        "tar",
    )?;

    Ok(source)
}

pub(super) fn write_native_sources(build: &Path) -> Result<(), InstrumentationError> {
    for (name, contents) in NATIVE_SOURCES {
        let path = build.join(name);

        fs::write(&path, contents)
            .map_err(|error| InstrumentationError::io("write", &path, error))?;
    }

    Ok(())
}

pub(super) fn instrument_lto_source(path: &Path) -> Result<(), InstrumentationError> {
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

pub(super) fn instrument_instruction_sinking_source(
    path: &Path,
) -> Result<(), InstrumentationError> {
    let source =
        fs::read_to_string(path).map_err(|error| InstrumentationError::io("read", path, error))?;

    let source = replace_section(
        source,
        "bool InstCombinerImpl::tryToSinkInstruction(",
        "\nvoid InstCombinerImpl::tryToSinkInstructionDbgVariableRecords(",
        "#include \"instruction_sinking.cpp\"\n",
        path,
    )?;

    let source = replace_section(
        source,
        "    auto getOptionalSinkBlockForInst =",
        "\n    auto OptBB = getOptionalSinkBlockForInst(I);",
        "",
        path,
    )?;

    let source = replace_once(
        source,
        "getOptionalSinkBlockForInst(I);",
        "getOptionalSinkBlockForInst(I, DT);",
        path,
    )?;

    fs::write(path, source).map_err(|error| InstrumentationError::io("write", path, error))
}

fn replace_section(
    mut source: String,
    start: &str,
    end: &str,
    replacement: &str,
    path: &Path,
) -> Result<String, InstrumentationError> {
    let starts = source
        .match_indices(start)
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();

    let ends = source
        .match_indices(end)
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();

    let ([start], [end]) = (starts.as_slice(), ends.as_slice()) else {
        return Err(InstrumentationError::SourceContract(path.to_path_buf()));
    };

    if start >= end {
        return Err(InstrumentationError::SourceContract(path.to_path_buf()));
    }

    source.replace_range(*start..*end, replacement);

    Ok(source)
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::replace_section;
    use crate::instrumentation::InstrumentationError;

    #[test]
    fn source_sections_require_unique_ordered_boundaries() {
        let path = Path::new("InstructionCombining.cpp");

        for source in [
            "end",
            "start",
            "start start end",
            "start end end",
            "end start",
        ] {
            assert!(matches!(
                replace_section(source.into(), "start", "end", "replacement", path),
                Err(InstrumentationError::SourceContract(_))
            ));
        }
    }
}
