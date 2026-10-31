use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use bray_diagnostics::{DiagnosticIoErrorKind, DiagnosticProjectGeneratorFailure as Failure};

use super::planning::{failure, selected_generator, target_profile};
use crate::generator::execution::{cache_key, validate_outputs};
use crate::{
    GeneratedSource, GeneratorHost, GeneratorInput, GeneratorInputIdentity, GeneratorRequest,
    ProjectBuildNode, ProjectGenerator, ProjectGraph, ProjectLoadError,
};

pub(in super::super) fn generate_sources(
    workspace_root: &Path,
    graph: &ProjectGraph,
    host: Option<&mut GeneratorHost<'_>>,
) -> Result<Arc<[GeneratedSource]>, ProjectLoadError> {
    let Some(identity) = graph.build_order().iter().find_map(|node| match node {
        ProjectBuildNode::Generator { generator, .. } => Some(generator),
        ProjectBuildNode::Product { .. } => None,
    }) else {
        return Ok(Arc::from([]));
    };

    let error = |cause| failure(graph, identity.package(), identity.name(), cause);
    let host = host.ok_or_else(|| error(Failure::ExecutorUnavailable))?;
    let mut sources = Vec::new();
    let mut staged = BTreeMap::new();
    let mut owned_inputs = BTreeMap::new();

    // Capture all source inputs before invoking any tool. Each target observes the same file snapshot.
    for node in graph.build_order() {
        if let ProjectBuildNode::Generator { generator, .. } = node {
            if host.cancellation.is_cancelled() {
                return Err(error(Failure::Cancelled));
            }

            if !owned_inputs.contains_key(generator) {
                let declaration = selected_generator(graph, generator);
                let inputs = read_inputs(workspace_root, graph, declaration)?;

                owned_inputs.insert(generator, inputs);
            }
        }
    }

    for node in graph.build_order() {
        let ProjectBuildNode::Generator {
            generator: identity,
            target,
        } = node
        else {
            continue;
        };

        let generator = selected_generator(graph, identity);
        let error = |cause| failure(graph, identity.package(), identity.name(), cause);

        if host.cancellation.is_cancelled() {
            return Err(error(Failure::Cancelled));
        }

        let executable = host.executables.get(generator.tool()).ok_or_else(|| {
            error(Failure::ToolUnavailable {
                tool: generator.tool().qualified_name(),
            })
        })?;

        // Inputs share immutable snapshot bytes rather than rereading the workspace per target.
        let mut inputs = owned_inputs[identity].to_vec();

        for (input_generator, output) in generator.generated_inputs() {
            let source: &GeneratedSource = sources
                .iter()
                .find(|source: &&GeneratedSource| {
                    source.generator() == input_generator
                        && source.target() == target
                        && source.output() == output
                })
                .expect("generated dependencies must execute before their consumers");

            inputs.push(GeneratorInput {
                identity: GeneratorInputIdentity::Generated {
                    generator: input_generator.clone(),
                    output: output.clone(),
                },
                bytes: Arc::from(source.text().as_bytes()),
            });
        }

        let request = GeneratorRequest {
            generator,
            executable,
            target: target_profile(graph, target),
            inputs: &inputs,
            policy_identity: &host.policy_identity,
        };

        let key = cache_key(&request);

        let generated = match (host.cache.entries.get(&key), host.cache_mode) {
            (Some(cached), crate::GeneratorCacheMode::Reuse) => Arc::clone(cached),
            _ => {
                let outputs = host
                    .executor
                    .execute(&request, host.cancellation)
                    .map_err(error)?;

                if host.cancellation.is_cancelled() {
                    return Err(error(Failure::Cancelled));
                }

                let validated = validate_outputs(&request, key, outputs).map_err(error)?;

                if let Some(previous) = host.cache.entries.get(&key) {
                    assert_eq!(
                        previous, &validated,
                        "generator output changed for equal inputs and policy: {key:?}"
                    );
                }

                staged.insert(key, Arc::clone(&validated));

                validated
            }
        };

        // Graph snapshots share the cache's immutable identity and text values.
        sources.extend(generated.iter().cloned());
    }

    if host.cancellation.is_cancelled() {
        return Err(error(Failure::Cancelled));
    }

    host.cache.entries.extend(staged);

    Ok(sources.into())
}

fn read_inputs(
    workspace_root: &Path,
    graph: &ProjectGraph,
    generator: &ProjectGenerator,
) -> Result<Vec<GeneratorInput>, ProjectLoadError> {
    let package = graph
        .package(generator.identity().package())
        .expect("generator declaration must have an owning package");

    let package_root = package.path().beneath(workspace_root);
    let mut inputs = Vec::with_capacity(generator.inputs().len());

    for input in generator.inputs() {
        let error = |cause| {
            failure(
                graph,
                package.identity(),
                generator.identity().name(),
                cause,
            )
        };

        let io_error = |cause: std::io::Error| {
            error(Failure::InputIo {
                path: input.as_str().into(),
                kind: DiagnosticIoErrorKind::from(cause.kind()),
            })
        };

        let mut current = package_root.to_path_buf();
        let mut components = input.as_str().split('/').peekable();

        while let Some(component) = components.next() {
            current.push(component);

            let metadata = fs::symlink_metadata(&current).map_err(io_error)?;

            let regular = if components.peek().is_some() {
                metadata.is_dir()
            } else {
                metadata.is_file()
            };

            if metadata.file_type().is_symlink() || !regular {
                return Err(error(Failure::InputNotRegular {
                    path: input.as_str().into(),
                }));
            }
        }

        let bytes = fs::read(&current).map_err(io_error)?;

        inputs.push(GeneratorInput {
            identity: GeneratorInputIdentity::Source(input.clone()),
            bytes: bytes.into(),
        });
    }

    Ok(inputs)
}
