use std::collections::BTreeMap;

use sha2::{Digest as _, Sha256};

use super::model::{
    CompilationAuthority, CompilationBuildReport, CompilationComparability,
    CompilationComparisonReport, CompilationIncomparability, CompilationKind,
    CompilationLanguage, LibraryReuse, LinkerInvocationReport,
    MAX_COMPILATION_INPUT_COUNT, MAX_RESPONSE_FILE_COUNT, MAX_RETAINED_INPUT_COUNT,
    MAX_TOOL_ARGUMENT_COUNT, MAX_TOOL_ENVIRONMENT_COUNT, ToolInvocationReport,
    ArtifactReport, BoundedList, RetainedInput,
};

const LANGUAGES: [CompilationLanguage; 3] = [
    CompilationLanguage::Bray,
    CompilationLanguage::Rust,
    CompilationLanguage::Cpp,
];

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

pub(super) fn reused_artifacts(
    artifacts: &[ArtifactReport],
    additional: impl IntoIterator<Item = RetainedInput>,
) -> BoundedList<RetainedInput> {
    let mut entries = artifacts
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
        .chain(additional)
        .collect::<Vec<_>>();

    entries.sort_unstable();
    entries.dedup();

    let omitted_count = entries.len().saturating_sub(MAX_RETAINED_INPUT_COUNT);
    entries.truncate(MAX_RETAINED_INPUT_COUNT);

    BoundedList {
        entries,
        omitted_count: u64::try_from(omitted_count).unwrap_or(u64::MAX),
    }
}

pub(super) const fn empty_reused_artifacts() -> BoundedList<RetainedInput> {
    BoundedList {
        entries: Vec::new(),
        omitted_count: 0,
    }
}

pub(super) fn validate(report: &CompilationComparisonReport) -> Result<(), String> {
    if report.contract.is_empty() {
        return Err("compilation comparison has no semantic contract".to_owned());
    }

    for build in report.builds.values() {
        validate_build(report.kind, build)?;
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

        match (kind, authority.library_reuse) {
            (CompilationKind::Application, LibraryReuse::Packaged) => {
                if build.reused_artifacts.entries.is_empty() {
                    language_reasons
                        .push(CompilationIncomparability::MissingPackagedLibraryArtifact);
                }
            }
            (CompilationKind::Application, LibraryReuse::Source) => language_reasons
                .push(CompilationIncomparability::CompilesLibrarySourceForApplication),
            (CompilationKind::Library, LibraryReuse::Packaged) => language_reasons
                .push(CompilationIncomparability::ReusesPackagedLibraryForLibraryBuild),
            (CompilationKind::Library, LibraryReuse::Source) => {
                if !build.reused_artifacts.entries.is_empty()
                    || build.reused_artifacts.omitted_count != 0
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

fn validate_build(kind: CompilationKind, build: &CompilationBuildReport) -> Result<(), String> {
    if build.toolchain.is_empty()
        || build.source_sha256.len() != 64
        || !bray_base::is_lowercase_hex(&build.source_sha256)
        || build.elapsed_nanoseconds == 0
        || build.authority.packages.len() > MAX_COMPILATION_INPUT_COUNT
        || build.authority.modules.len() > MAX_COMPILATION_INPUT_COUNT
        || !strictly_sorted(&build.authority.packages)
        || !strictly_sorted(&build.authority.modules)
        || build.reused_artifacts.entries.len() > MAX_RETAINED_INPUT_COUNT
        || !strictly_sorted(&build.reused_artifacts.entries)
    {
        return Err("compilation build is incomplete or outside its bounds".to_owned());
    }

    if let Some(profile) = &build.profile {
        profile
            .validate()
            .map_err(|error| format!("compilation build profile is invalid: {error:?}"))?;
    }

    validate_invocation(&build.compiler)?;

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
