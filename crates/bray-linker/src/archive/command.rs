use std::ffi::OsString;
use std::path::Path;

use super::format::ArchiveFormat;
use crate::external_tool::{
    ResponseFileEncoding, ResponseFileEncodingError,
    encode_response_arguments, response_file_materialization_path,
    response_file_path, response_file_reference,
};
use crate::{
    DeadStripPolicy, ExternalToolInvocation,
    ExternalToolInvocationBuildError, ExternalToolResponseFile,
    ExternalToolResponseFileBuildError, LinkInputKind, LinkInputSource,
    LinkPlan, LinkedArtifactKind, SectionGarbageCollectionPolicy,
};

pub(super) fn invocation(
    template: &ExternalToolInvocation,
    plan: &LinkPlan,
    format: ArchiveFormat,
) -> Result<ExternalToolInvocation, ArchiveInvocationBuildError> {
    validate_plan(plan)?;

    let output = plan
        .primary_output()
        .ok_or(ArchiveInvocationBuildError::InvalidPlan)?
        .destination()
        .path();

    let input_paths = plan
        .inputs()
        .iter()
        .map(|input| {
            let LinkInputSource::File(path) = input.source() else {
                return Err(ArchiveInvocationBuildError::InvalidPlan);
            };

            Ok(path.as_os_str().to_os_string())
        })
        .collect::<Result<Vec<_>, _>>()?;

    let reference_path =
        response_file_path(output, ".bray-archive.rsp");

    let materialization_path = response_file_materialization_path(
        &reference_path,
        template.current_directory(),
    );

    let contents = encode_response_arguments(
        &input_paths,
        ResponseFileEncoding::Utf8,
    )
    .map_err(ArchiveInvocationBuildError::ResponseEncoding)?;

    let response_file =
        ExternalToolResponseFile::try_new(materialization_path, contents)
            .map_err(ArchiveInvocationBuildError::ResponseFile)?;

    let arguments = arguments(
        format,
        output,
        response_file_reference(&reference_path),
    );

    // Each process request owns its configuration so the immutable driver can serve concurrent links.
    let environment = template.environment().iter().cloned();

    let current_directory = template
        .current_directory()
        .map(std::path::Path::to_path_buf);

    ExternalToolInvocation::try_new(
        template.program(),
        arguments,
        environment,
        current_directory,
        [response_file],
    )
    .map_err(ArchiveInvocationBuildError::Invocation)
}

fn validate_plan(
    plan: &LinkPlan,
) -> Result<(), ArchiveInvocationBuildError> {
    let policy = plan.policy();

    if plan.outputs().len() != 1
        || plan.outputs()[0].kind() != LinkedArtifactKind::StaticLibrary
        || plan
            .inputs()
            .iter()
            .any(|input| input.kind() != LinkInputKind::RelocatableObject)
        || !plan.exported_symbols().is_empty()
        || !plan.retained_symbols().is_empty()
        || !plan.search_paths().is_empty()
        || policy.dead_strip() != DeadStripPolicy::Preserve
        || policy.section_garbage_collection()
            != SectionGarbageCollectionPolicy::Preserve
        || policy.subsystem().is_some()
    {
        return Err(ArchiveInvocationBuildError::InvalidPlan);
    }

    let output = plan.outputs()[0].destination().path();

    if plan.inputs().iter().any(
        |input| matches!(input.source(), LinkInputSource::File(path) if path == output),
    ) {
        return Err(ArchiveInvocationBuildError::InvalidPlan);
    }

    Ok(())
}

fn arguments(
    format: ArchiveFormat,
    output: &Path,
    response_file: OsString,
) -> Vec<OsString> {
    // Quick append preserves repeated member names while `s` indexes and `D` normalizes metadata.
    vec![
        OsString::from("--rsp-quoting=posix"),
        OsString::from(format!("--format={}", format.llvm_name())),
        OsString::from("qcsD"),
        output.as_os_str().to_os_string(),
        response_file,
    ]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ArchiveInvocationBuildError {
    InvalidPlan,
    Invocation(ExternalToolInvocationBuildError),
    ResponseEncoding(ResponseFileEncodingError),
    ResponseFile(ExternalToolResponseFileBuildError),
}
