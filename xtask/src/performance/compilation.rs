use std::collections::BTreeMap;
use std::path::Path;

use bray_target::NativeTarget;
use sha2::{Digest as _, Sha256};

use super::model::{
    ArtifactReport, BoundedList, CompilationAuthority, CompilationBuildReport,
    CompilationComparability, CompilationComparisonReport, CompilationIncomparability,
    CompilationKind, CompilationLanguage, CompilationReuseEvidence, LibraryReuse,
    LinkerInvocationReport, RetainedInput,
    MAX_COMPILATION_INPUT_COUNT, MAX_RESPONSE_FILE_COUNT, MAX_RETAINED_INPUT_COUNT,
    MAX_TOOL_ARGUMENT_COUNT, MAX_TOOL_ENVIRONMENT_COUNT, ToolInvocationReport,
};

const LANGUAGES: [CompilationLanguage; 3] = [
    CompilationLanguage::Bray,
    CompilationLanguage::Rust,
    CompilationLanguage::Cpp,
];
const MAXIMUM_SOURCE_BYTE_RATIO: u64 = 8;

pub(super) const MATCHED_LIBRARY_CONTRACT: &str =
    "compile one source-authored library that exports an unsigned 64-bit accumulation operation";
pub(super) const MATCHED_APPLICATION_CONTRACT: &str =
    "compile one source-authored application that returns success while consuming the language's packaged library and runtime";

pub(super) fn comparison(
    kind: CompilationKind,
    contract: impl Into<String>,
    builds: BTreeMap<CompilationLanguage, CompilationBuildReport>,
) -> CompilationComparisonReport {
    let comparability = comparability(kind, &builds);

    CompilationComparisonReport {
        kind,
        contract: contract.into(),
        comparability,
        builds,
    }
}

pub(super) fn authority(
    source_units: u64,
    source_bytes: u64,
    packages: impl IntoIterator<Item = String>,
    modules: impl IntoIterator<Item = String>,
    library_reuse: LibraryReuse,
) -> CompilationAuthority {
    let mut packages = packages.into_iter().collect::<Vec<_>>();
    let mut modules = modules.into_iter().collect::<Vec<_>>();

    packages.sort_unstable();
    packages.dedup();
    modules.sort_unstable();
    modules.dedup();

    CompilationAuthority {
        source_units,
        source_bytes,
        packages,
        modules,
        library_reuse,
    }
}

pub(super) fn source_digest(source: &str) -> String {
    bray_base::lowercase_hex(&Sha256::digest(source.as_bytes()))
}

pub(super) fn source_bytes(source: &str) -> Result<u64, String> {
    u64::try_from(source.len()).map_err(|_| "compilation source byte count exceeds u64".to_owned())
}

pub(super) fn external_invocation(
    program: impl Into<String>,
    arguments: Vec<String>,
    environment: BTreeMap<String, String>,
) -> ToolInvocationReport {
    ToolInvocationReport {
        program: program.into(),
        arguments,
        environment,
        response_files: Vec::new(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CompilationReuseRole {
    PackagedLibrary,
    Runtime,
}

pub(super) fn reuse_evidence(
    artifacts: &[ArtifactReport],
    role: impl Fn(&RetainedInput) -> Option<CompilationReuseRole>,
) -> CompilationReuseEvidence {
    let entries = artifacts
        .iter()
        .flat_map(|artifact| {
            artifact
                .dependencies
                .static_inputs
                .entries
                .iter()
                .filter(|input| input.member.is_some())
                .cloned()
        })
        .collect::<Vec<_>>();

    let mut packaged_library = Vec::new();
    let mut runtime = Vec::new();

    for input in entries {
        match role(&input) {
            Some(CompilationReuseRole::PackagedLibrary) => packaged_library.push(input),
            Some(CompilationReuseRole::Runtime) => runtime.push(input),
            None => {}
        }
    }

    CompilationReuseEvidence {
        packaged_library: bounded_reuse(packaged_library),
        runtime: bounded_reuse(runtime),
    }
}

fn bounded_reuse(entries: impl IntoIterator<Item = RetainedInput>) -> BoundedList<RetainedInput> {
    let mut entries = entries.into_iter().collect::<Vec<_>>();

    entries.sort_unstable();
    entries.dedup();

    let omitted_count = entries.len().saturating_sub(MAX_RETAINED_INPUT_COUNT);
    entries.truncate(MAX_RETAINED_INPUT_COUNT);

    BoundedList {
        entries,
        omitted_count: u64::try_from(omitted_count).unwrap_or(u64::MAX),
    }
}

pub(super) const fn empty_reuse_evidence() -> CompilationReuseEvidence {
    CompilationReuseEvidence {
        packaged_library: BoundedList {
            entries: Vec::new(),
            omitted_count: 0,
        },
        runtime: BoundedList {
            entries: Vec::new(),
            omitted_count: 0,
        },
    }
}

pub(super) fn validate(
    report: &CompilationComparisonReport,
    target: NativeTarget,
) -> Result<(), String> {
    if report.contract.is_empty() {
        return Err("compilation comparison has no semantic contract".to_owned());
    }

    for (language, build) in &report.builds {
        validate_build(report.kind, *language, target, build)?;
    }

    let expected = comparability(report.kind, &report.builds);

    if report.comparability != expected {
        return Err("compilation comparison publishes a misleading comparability result".to_owned());
    }

    Ok(())
}

fn comparability(
    kind: CompilationKind,
    builds: &BTreeMap<CompilationLanguage, CompilationBuildReport>,
) -> CompilationComparability {
    let mut reasons = BTreeMap::new();

    let reference_authority = LANGUAGES
        .into_iter()
        .find_map(|language| builds.get(&language))
        .map(|build| &build.authority);

    let source_unit_counts_match = reference_authority.is_none_or(|reference| {
        builds
            .values()
            .all(|build| build.authority.source_units == reference.source_units)
    });

    let package_input_counts_match = reference_authority.is_none_or(|reference| {
        builds
            .values()
            .all(|build| build.authority.packages.len() == reference.packages.len())
    });

    let module_input_counts_match = reference_authority.is_none_or(|reference| {
        builds
            .values()
            .all(|build| build.authority.modules.len() == reference.modules.len())
    });

    let source_byte_scale_matches = source_byte_scale_matches(builds);

    for language in LANGUAGES {
        let Some(build) = builds.get(&language) else {
            reasons.insert(
                language,
                vec![CompilationIncomparability::MissingImplementation],
            );

            continue;
        };

        let mut language_reasons = Vec::new();
        let authority = &build.authority;

        if authority.source_units == 0 || authority.source_bytes == 0 {
            language_reasons.push(CompilationIncomparability::MissingSourceAuthority);
        }

        if authority.packages.is_empty() {
            language_reasons.push(CompilationIncomparability::MissingPackageInputs);
        }

        if authority.modules.is_empty() {
            language_reasons.push(CompilationIncomparability::MissingModuleInputs);
        }

        if !source_unit_counts_match {
            language_reasons.push(CompilationIncomparability::DifferentSourceUnitCount);
        }

        if !package_input_counts_match {
            language_reasons.push(CompilationIncomparability::DifferentPackageInputCount);
        }

        if !module_input_counts_match {
            language_reasons.push(CompilationIncomparability::DifferentModuleInputCount);
        }

        if !source_byte_scale_matches {
            language_reasons.push(CompilationIncomparability::DifferentSourceByteScale);
        }

        match (kind, authority.library_reuse) {
            (CompilationKind::Application, LibraryReuse::Packaged) => {
                if build.reuse.packaged_library.entries.is_empty() {
                    language_reasons
                        .push(CompilationIncomparability::MissingPackagedLibraryArtifact);
                }

                if build.reuse.runtime.entries.is_empty() {
                    language_reasons.push(CompilationIncomparability::MissingRuntimeArtifact);
                }
            }
            (CompilationKind::Application, LibraryReuse::Source) => language_reasons
                .push(CompilationIncomparability::CompilesLibrarySourceForApplication),
            (CompilationKind::Library, LibraryReuse::Packaged) => language_reasons
                .push(CompilationIncomparability::ReusesPackagedLibraryForLibraryBuild),
            (CompilationKind::Library, LibraryReuse::Source) => {
                if !build.reuse.packaged_library.entries.is_empty()
                    || build.reuse.packaged_library.omitted_count != 0
                    || !build.reuse.runtime.entries.is_empty()
                    || build.reuse.runtime.omitted_count != 0
                {
                    language_reasons.push(CompilationIncomparability::UnexpectedReusedArtifact);
                }
            }
        }

        if !language_reasons.is_empty() {
            reasons.insert(language, language_reasons);
        }
    }

    if reasons.is_empty() {
        CompilationComparability::Comparable
    } else {
        CompilationComparability::Incomparable { reasons }
    }
}

fn source_byte_scale_matches(
    builds: &BTreeMap<CompilationLanguage, CompilationBuildReport>,
) -> bool {
    let Some(minimum) = builds.values().map(|build| build.authority.source_bytes).min() else {
        return true;
    };

    let Some(maximum) = builds.values().map(|build| build.authority.source_bytes).max() else {
        return true;
    };

    minimum > 0 && maximum <= minimum.saturating_mul(MAXIMUM_SOURCE_BYTE_RATIO)
}

fn validate_build(
    kind: CompilationKind,
    language: CompilationLanguage,
    target: NativeTarget,
    build: &CompilationBuildReport,
) -> Result<(), String> {
    if build.toolchain.is_empty()
        || build.source_sha256.len() != 64
        || !bray_base::is_lowercase_hex(&build.source_sha256)
        || build.elapsed_nanoseconds == 0
        || build.authority.packages.len() > MAX_COMPILATION_INPUT_COUNT
        || build.authority.modules.len() > MAX_COMPILATION_INPUT_COUNT
        || !strictly_sorted(&build.authority.packages)
        || !strictly_sorted(&build.authority.modules)
        || !reuse_is_valid(&build.reuse.packaged_library)
        || !reuse_is_valid(&build.reuse.runtime)
    {
        return Err("compilation build is incomplete or outside its bounds".to_owned());
    }

    if let Some(profile) = &build.profile {
        profile
            .validate()
            .map_err(|error| format!("compilation build profile is invalid: {error:?}"))?;
    }

    if let Some(evidence) = &build.evidence {
        validate_invocation(&evidence.compiler)?;

        if let Some(linker_map) = &evidence.linker_map
            && (linker_map.bytes == 0
                || linker_map.sha256.len() != 64
                || !bray_base::is_lowercase_hex(&linker_map.sha256))
        {
            return Err("compilation linker evidence is invalid".to_owned());
        }
    }

    validate_invocation(&build.compiler)?;
    validate_compiler_policy(kind, language, target, build)?;

    match (&build.linker, kind) {
        (
            LinkerInvocationReport::IntegratedCompilerDriver { driver, arguments },
            _,
        )
            if driver.is_empty()
                || arguments.is_empty()
                || arguments.len() > MAX_TOOL_ARGUMENT_COUNT =>
        {
            return Err("integrated linker arguments are empty or outside their bound".to_owned());
        }
        (LinkerInvocationReport::NotApplicable, CompilationKind::Application) => {
            return Err("application compilation omits its linker invocation".to_owned());
        }
        (LinkerInvocationReport::NotApplicable, CompilationKind::Library)
        | (LinkerInvocationReport::IntegratedCompilerDriver { .. }, _) => {}
    }

    Ok(())
}

fn validate_compiler_policy(
    kind: CompilationKind,
    language: CompilationLanguage,
    target: NativeTarget,
    build: &CompilationBuildReport,
) -> Result<(), String> {
    let invocation = &build.compiler;
    let arguments = &invocation.arguments;

    let program = Path::new(&invocation.program)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if !invocation.environment.is_empty()
        || !invocation.response_files.is_empty()
        || contains_timing_instrumentation(arguments)
    {
        return Err("matched compiler invocation includes noncanonical timing inputs".to_owned());
    }

    let valid = match language {
        CompilationLanguage::Bray => {
            program == "brayc"
                && arguments.iter().any(|argument| argument == "build")
                && arguments.iter().any(|argument| argument == "--release")
                && has_pair(arguments, "--target", target.as_str())
                && has_pair(
                    arguments,
                    "--product-kind",
                    match kind {
                        CompilationKind::Application => "executable",
                        CompilationKind::Library => "library",
                    },
                )
                && has_pair(
                    arguments,
                    "--artifact",
                    match kind {
                        CompilationKind::Application => "executable",
                        CompilationKind::Library => "relocatable-object",
                    },
                )
                && has_argument_value(arguments, "--output")
                && match kind {
                    CompilationKind::Application => {
                        has_argument_value(arguments, "--runtime-artifact")
                            && has_argument_value(arguments, "--standard-library-root")
                    }
                    CompilationKind::Library => {
                        !arguments.iter().any(|argument| argument == "--runtime-artifact")
                    }
                }
        }
        CompilationLanguage::Rust => {
            program == "rustc"
                && has_pair(arguments, "--target", target.as_str())
                && has_pair(arguments, "-C", "opt-level=3")
                && has_pair(arguments, "-C", "debuginfo=0")
                && has_argument_value(arguments, "-o")
                && match kind {
                    CompilationKind::Application => {
                        !has_pair(arguments, "--emit", "obj")
                    }
                    CompilationKind::Library => {
                        has_pair(arguments, "--crate-type", "lib")
                            && has_pair(arguments, "--emit", "obj")
                    }
                }
        }
        CompilationLanguage::Cpp => {
            (program == "clang" || program == "clang++")
                && arguments.iter().any(|argument| argument == "-O3")
                && arguments.iter().any(|argument| argument == "-DNDEBUG")
                && arguments
                    .iter()
                    .any(|argument| argument == &format!("--target={}", target.as_str()))
                && has_argument_value(arguments, "-o")
                && (arguments.iter().any(|argument| argument == "-c")
                    == matches!(kind, CompilationKind::Library))
        }
    };

    if !valid {
        return Err("matched compiler invocation violates its language policy".to_owned());
    }

    match (&build.linker, kind) {
        (
            LinkerInvocationReport::IntegratedCompilerDriver { driver, arguments },
            CompilationKind::Application,
        ) if driver == &invocation.program && arguments == &invocation.arguments => {}
        (LinkerInvocationReport::NotApplicable, CompilationKind::Library) => {}
        _ => return Err("matched linker invocation differs from compiler policy".to_owned()),
    }

    match (kind, language, build.profile.as_ref(), build.evidence.as_ref()) {
        (
            CompilationKind::Application,
            CompilationLanguage::Bray,
            Some(_),
            Some(evidence),
        ) if evidence.linker_map.is_some()
            && evidence_matches_timed_invocation(invocation, &evidence.compiler)
            && contains_profile_instrumentation(&evidence.compiler.arguments)
            && contains_linker_map_instrumentation(&evidence.compiler.arguments) => {}
        (
            CompilationKind::Application,
            CompilationLanguage::Rust | CompilationLanguage::Cpp,
            None,
            Some(evidence),
        ) if evidence.linker_map.is_some()
            && evidence_matches_timed_invocation(invocation, &evidence.compiler)
            && !contains_profile_instrumentation(&evidence.compiler.arguments)
            && contains_linker_map_instrumentation(&evidence.compiler.arguments) => {}
        (
            CompilationKind::Library,
            CompilationLanguage::Bray,
            Some(_),
            Some(evidence),
        ) if evidence.linker_map.is_none()
            && evidence_matches_timed_invocation(invocation, &evidence.compiler)
            && contains_profile_instrumentation(&evidence.compiler.arguments)
            && !contains_linker_map_instrumentation(&evidence.compiler.arguments) => {}
        (
            CompilationKind::Library,
            CompilationLanguage::Rust | CompilationLanguage::Cpp,
            None,
            None,
        ) => {}
        _ => return Err("compilation evidence does not match its lane policy".to_owned()),
    }

    Ok(())
}

fn evidence_matches_timed_invocation(
    timed: &ToolInvocationReport,
    evidence: &ToolInvocationReport,
) -> bool {
    timed.program == evidence.program
        && evidence.environment.is_empty()
        && evidence.response_files.is_empty()
        && normalized_evidence_arguments(&timed.arguments)
            == normalized_evidence_arguments(&evidence.arguments)
}

fn normalized_evidence_arguments(arguments: &[String]) -> Vec<String> {
    let mut normalized = Vec::with_capacity(arguments.len());
    let mut index = 0;

    while let Some(argument) = arguments.get(index) {
        if matches!(argument.as_str(), "--profile" | "--profile-output" | "--linker-map-output") {
            index = index.saturating_add(2);

            continue;
        }

        if matches!(argument.as_str(), "--output" | "-o") {
            normalized.push(argument.clone());
            normalized.push("<output>".to_owned());
            index = index.saturating_add(2);

            continue;
        }

        if argument == "-C"
            && arguments
                .get(index.saturating_add(1))
                .is_some_and(|value| linker_map_argument(value))
        {
            index = index.saturating_add(2);

            continue;
        }

        if linker_map_argument(argument) {
            index = index.saturating_add(1);

            continue;
        }

        normalized.push(argument.clone());
        index = index.saturating_add(1);
    }

    normalized
}

fn linker_map_argument(argument: &str) -> bool {
    let argument = argument.to_ascii_lowercase();

    argument.contains("-map,")
        || argument.contains("/map:")
        || argument.starts_with("/map")
}

fn contains_timing_instrumentation(arguments: &[String]) -> bool {
    contains_profile_instrumentation(arguments)
        || contains_linker_map_instrumentation(arguments)
}

fn contains_profile_instrumentation(arguments: &[String]) -> bool {
    arguments
        .iter()
        .any(|argument| argument == "--profile" || argument == "--profile-output")
}

fn contains_linker_map_instrumentation(arguments: &[String]) -> bool {
    arguments
        .iter()
        .any(|argument| argument == "--linker-map-output" || linker_map_argument(argument))
}

fn has_pair(arguments: &[String], name: &str, value: &str) -> bool {
    arguments
        .windows(2)
        .any(|pair| pair[0] == name && pair[1] == value)
}

fn has_argument_value(arguments: &[String], name: &str) -> bool {
    arguments
        .windows(2)
        .any(|pair| pair[0] == name && !pair[1].is_empty())
}

fn reuse_is_valid(inputs: &BoundedList<RetainedInput>) -> bool {
    inputs.entries.len() <= MAX_RETAINED_INPUT_COUNT
        && strictly_sorted(&inputs.entries)
        && inputs.entries.iter().all(|input| {
            !input.artifact.is_empty() && input.member.as_ref().is_some_and(|member| !member.is_empty())
        })
}

fn validate_invocation(invocation: &ToolInvocationReport) -> Result<(), String> {
    if invocation.program.is_empty()
        || invocation.arguments.is_empty()
        || invocation.arguments.len() > MAX_TOOL_ARGUMENT_COUNT
        || invocation.environment.len() > MAX_TOOL_ENVIRONMENT_COUNT
        || invocation.response_files.len() > MAX_RESPONSE_FILE_COUNT
        || invocation
            .environment
            .iter()
            .any(|(name, value)| name.is_empty() || value.is_empty())
        || invocation
            .response_files
            .iter()
            .any(|file| {
                file.path.is_empty()
                    || file.contents_hex.is_empty()
                    || !bray_base::is_lowercase_hex(&file.contents_hex)
            })
    {
        return Err("external tool invocation is incomplete or outside its bounds".to_owned());
    }

    Ok(())
}

fn strictly_sorted<T: Ord>(entries: &[T]) -> bool {
    entries.windows(2).all(|pair| pair[0] < pair[1])
}
