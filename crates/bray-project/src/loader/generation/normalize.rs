use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use bray_diagnostics::{
    DiagnosticProjectGeneratorFailure as Failure, DiagnosticProjectManifestField as Field,
};
use bray_standard_library::PackageSourceAuthority;
use bray_symbols::{PackageIdentity, ProductIdentity};

use super::super::validation::{local_name, package_identity, paths_overlap, project_path};
use crate::manifest::GeneratorManifest;
use crate::{
    GeneratorIdentity, GeneratorPermissions, GeneratorTool, ProjectGenerator, ProjectLoadError,
    ProjectPath, ProjectTarget,
};

pub(in super::super) fn normalize_generators(
    manifests: Vec<GeneratorManifest>,
    package: &PackageIdentity,
    targets: &[ProjectTarget],
    manifest_path: &Path,
    authority: PackageSourceAuthority,
) -> Result<Arc<[ProjectGenerator]>, ProjectLoadError> {
    let mut generators = Vec::with_capacity(manifests.len());

    for manifest in manifests {
        let name = local_name(manifest.name, manifest_path, Field::GeneratorName)?;

        // Generator identities share the package's immutable Arc-backed identity.
        let identity = GeneratorIdentity {
            package: package.clone(),
            name,
        };

        let failure = |failure| ProjectLoadError::Generator {
            path: manifest_path.to_path_buf(),
            generator: identity.qualified_name(),
            failure,
        };

        let tool_package = package_identity(
            manifest.tool.package,
            manifest_path,
            authority,
            Field::GeneratorTool,
        )?;

        let tool_name = local_name(manifest.tool.product, manifest_path, Field::GeneratorTool)?;

        let product = ProductIdentity::try_new(tool_package, tool_name)
            .expect("validated tool name must form a product identity");

        let target = targets
            .iter()
            .find(|target| target.name() == manifest.tool.target)
            .ok_or_else(|| {
                ProjectLoadError::unknown_target(
                    manifest_path.to_path_buf(),
                    Field::GeneratorTool,
                    &manifest.tool.target,
                )
            })?;

        let inputs = manifest
            .inputs
            .into_iter()
            .map(|path| project_path(path, false, manifest_path, Field::GeneratorInputs))
            .collect::<Result<Vec<_>, _>>()?;

        let mut seen_inputs = BTreeSet::new();

        for input in &inputs {
            if !seen_inputs.insert(input) {
                return Err(ProjectLoadError::duplicate_selection(
                    manifest_path.to_path_buf(),
                    Field::GeneratorInputs,
                    input.as_str(),
                ));
            }
        }

        let generated_inputs = manifest
            .generated_inputs
            .into_iter()
            .map(|input| {
                let name = local_name(input.generator, manifest_path, Field::GeneratorInputs)?;

                let output =
                    project_path(input.output, false, manifest_path, Field::GeneratorInputs)?;

                Ok((
                    GeneratorIdentity {
                        package: package.clone(),
                        name,
                    },
                    output,
                ))
            })
            .collect::<Result<Arc<[_]>, ProjectLoadError>>()?;

        let mut seen_generated = BTreeSet::new();

        for input in generated_inputs.iter() {
            if !seen_generated.insert(input) {
                return Err(ProjectLoadError::duplicate_selection(
                    manifest_path.to_path_buf(),
                    Field::GeneratorInputs,
                    format!("{}/{}", input.0.qualified_name(), input.1.as_str()),
                ));
            }
        }

        let mut outputs = manifest
            .outputs
            .into_iter()
            .map(|path| {
                let output = ProjectPath::try_relative(path.as_str()).filter(|_| {
                    Path::new(&path)
                        .extension()
                        .is_some_and(|extension| extension == "bray")
                });

                output.ok_or_else(|| failure(Failure::InvalidOutput { path }))
            })
            .collect::<Result<Vec<_>, _>>()?;

        if outputs.is_empty() {
            return Err(ProjectLoadError::missing_selection(
                manifest_path.to_path_buf(),
                Field::GeneratorOutputs,
            ));
        }

        outputs.sort_unstable();

        for (index, output) in outputs.iter().enumerate() {
            if outputs[index + 1..]
                .iter()
                .any(|other| paths_overlap(output, other))
            {
                return Err(failure(Failure::OutputCollision {
                    path: output.as_str().into(),
                }));
            }
        }

        let mut environment = BTreeMap::new();

        for (name, value) in manifest.environment {
            if name.is_empty() || name.contains(['=', '\0']) || value.contains('\0') {
                return Err(failure(Failure::InvalidEnvironment { name }));
            }

            environment.insert(Arc::from(name), Arc::from(value));
        }

        generators.push(ProjectGenerator {
            identity,
            tool: GeneratorTool::new(product, target.identity().clone()),
            inputs: inputs.into(),
            generated_inputs,
            outputs: outputs.into(),
            environment,
            permissions: GeneratorPermissions {
                network: manifest.permissions.network,
            },
        });
    }

    generators.sort_unstable_by(|left, right| left.identity.cmp(&right.identity));

    if let Some(pair) = generators
        .windows(2)
        .find(|pair| pair[0].identity == pair[1].identity)
    {
        return Err(ProjectLoadError::duplicate_selection(
            manifest_path.to_path_buf(),
            Field::GeneratorName,
            pair[0].identity.name(),
        ));
    }

    for generator in &generators {
        for (input, output) in generator.generated_inputs.iter() {
            if !generators
                .iter()
                .any(|candidate| candidate.identity == *input && candidate.outputs.contains(output))
            {
                return Err(ProjectLoadError::Generator {
                    path: manifest_path.to_path_buf(),
                    generator: generator.identity.qualified_name(),
                    failure: Failure::UnknownReference {
                        name: format!("{}/{}", input.qualified_name(), output.as_str()),
                    },
                });
            }
        }
    }

    Ok(generators.into())
}
