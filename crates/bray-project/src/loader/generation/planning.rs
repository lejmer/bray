use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_diagnostics::DiagnosticProjectGeneratorFailure as Failure;
use bray_symbols::ProductKind;
use bray_target::{NativeTarget, TargetIdentity, TargetOutputKind};

use crate::{
    GeneratorIdentity, ProjectBuildNode as Node, ProjectGenerator, ProjectGraph, ProjectLoadError,
};

type ProjectPlan = (Arc<[Node]>, Arc<[crate::ProjectTargetBuildPlan]>);

pub(in super::super) fn plan_project(
    graph: &ProjectGraph,
) -> Result<ProjectPlan, ProjectLoadError> {
    validate_tools(graph)?;

    let mut pending = graph
        .packages()
        .iter()
        .flat_map(|package| package.products())
        .flat_map(|product| {
            product.targets().iter().map(|target| Node::Product {
                product: product.identity().clone(),
                target: target.clone(),
            })
        })
        .collect::<Vec<_>>();

    let mut dependencies = BTreeMap::new();

    while let Some(node) = pending.pop() {
        if dependencies.contains_key(&node) {
            continue;
        }

        let edges = node_dependencies(graph, &node)?;

        // Nodes are small Arc-backed identities, retained while discovering the closed graph.
        pending.extend(edges.iter().cloned());
        dependencies.insert(node, edges.into_iter().collect::<BTreeSet<_>>());
    }

    let order = dependency_first_order(dependencies)
        .map_err(|remaining| cycle_failure(graph, &remaining))?;

    let plans = graph
        .targets()
        .iter()
        .map(|target| target_plan(graph, target.identity(), &order))
        .collect::<Result<Arc<[_]>, _>>()?;

    Ok((order.into(), plans))
}

// Ordering retains only the small immutable product, generator, target, and package identities.
fn dependency_first_order<Key: Clone + Ord>(
    mut dependencies: BTreeMap<Key, BTreeSet<Key>>,
) -> Result<Vec<Key>, Vec<Key>> {
    let mut order = Vec::with_capacity(dependencies.len());

    while !dependencies.is_empty() {
        let Some(node) = dependencies
            .iter()
            .find(|(_, edges)| edges.iter().all(|edge| !dependencies.contains_key(edge)))
            .map(|(node, _)| node.clone())
        else {
            return Err(dependencies.into_keys().collect());
        };

        dependencies.remove(&node);
        order.push(node);
    }

    Ok(order)
}

fn cycle_failure(graph: &ProjectGraph, remaining: &[Node]) -> ProjectLoadError {
    if let Some(Node::Generator { generator, .. }) = remaining
        .iter()
        .find(|node| matches!(node, Node::Generator { .. }))
    {
        return failure(
            graph,
            generator.package(),
            generator.name(),
            Failure::DependencyCycle,
        );
    }

    let Some(Node::Product { product, .. }) = remaining.first() else {
        panic!("a cyclic project must have an unfulfilled product or generator");
    };

    let path = graph
        .package(product.package())
        .expect("cycle product owner must exist")
        .path()
        .beneath(std::path::Path::new(""))
        .join(crate::PACKAGE_MANIFEST_FILE_NAME);

    ProjectLoadError::dependency_cycle_product(
        path,
        bray_diagnostics::DiagnosticProjectManifestField::ProductDependencies,
        product.clone(),
    )
}

fn target_plan(
    graph: &ProjectGraph,
    target: &TargetIdentity,
    order: &[Node],
) -> Result<crate::ProjectTargetBuildPlan, ProjectLoadError> {
    let products = order
        .iter()
        .filter_map(|node| match node {
            Node::Product {
                product,
                target: selected,
            } if selected == target => Some(product.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();

    let mut dependencies = BTreeMap::new();

    for identity in &products {
        let package = graph
            .package(identity.package())
            .expect("planned product owner must exist");

        let product = package
            .products()
            .iter()
            .find(|product| product.identity() == identity)
            .expect("planned product must exist");

        let edges = dependencies
            .entry(identity.package().clone())
            .or_insert_with(BTreeSet::new);

        edges.extend(
            product
                .dependencies()
                .iter()
                .filter(|dependency| dependency.is_active_for(target))
                .map(|dependency| dependency.product().package().clone()),
        );
    }

    for node in order {
        let Node::Generator {
            generator: identity,
            target: selected,
        } = node
        else {
            continue;
        };

        let generator = selected_generator(graph, identity);

        if selected == target
            && generator.tool().target() == target
            && identity.package() != generator.tool().product().package()
        {
            dependencies
                .get_mut(identity.package())
                .expect("a selected generator must have a product owner for its target")
                .insert(generator.tool().product().package().clone());
        }
    }

    let packages = dependency_first_order(dependencies).map_err(|remaining| {
        let identity = remaining
            .first()
            .expect("a package cycle must have an unfulfilled package");

        let path = graph
            .package(identity)
            .expect("cycle package owner must exist")
            .path()
            .beneath(std::path::Path::new(""))
            .join(crate::PACKAGE_MANIFEST_FILE_NAME);

        ProjectLoadError::dependency_cycle_package(
            path,
            bray_diagnostics::DiagnosticProjectManifestField::ProductDependencies,
            identity.clone(),
        )
    })?;

    Ok(crate::ProjectTargetBuildPlan::new(
        target.clone(),
        packages.into(),
        products.into(),
    ))
}

fn validate_tools(graph: &ProjectGraph) -> Result<(), ProjectLoadError> {
    for package in graph.packages() {
        for generator in package.generators() {
            let tool = generator.tool();

            let tool_package = graph.package(tool.product().package()).ok_or_else(|| {
                let path = package
                    .path()
                    .beneath(std::path::Path::new(""))
                    .join(crate::PACKAGE_MANIFEST_FILE_NAME);

                ProjectLoadError::unknown_dependency_package(
                    path,
                    bray_diagnostics::DiagnosticProjectManifestField::GeneratorTool,
                    tool.product().package().clone(),
                )
            })?;

            let product = tool_package
                .products()
                .iter()
                .find(|product| product.identity() == tool.product())
                .ok_or_else(|| {
                    let path = package
                        .path()
                        .beneath(std::path::Path::new(""))
                        .join(crate::PACKAGE_MANIFEST_FILE_NAME);

                    ProjectLoadError::unknown_dependency_product(
                        path,
                        bray_diagnostics::DiagnosticProjectManifestField::GeneratorTool,
                        tool.product().clone(),
                    )
                })?;

            if product.kind() != ProductKind::Executable
                || !product.outputs().contains(&TargetOutputKind::Executable)
            {
                return Err(failure(
                    graph,
                    package.identity(),
                    generator.identity().name(),
                    Failure::ToolNotExecutable {
                        tool: tool.qualified_name(),
                    },
                ));
            }

            if !product.targets().contains(tool.target()) {
                let path = package
                    .path()
                    .beneath(std::path::Path::new(""))
                    .join(crate::PACKAGE_MANIFEST_FILE_NAME);

                return Err(ProjectLoadError::dependency_target_unavailable(
                    path,
                    bray_diagnostics::DiagnosticProjectManifestField::GeneratorTool,
                    tool.product().clone(),
                    tool.target().clone(),
                ));
            }

            let host = NativeTarget::current();

            if host.is_none_or(|host| host.identity() != *tool.target()) {
                return Err(failure(
                    graph,
                    package.identity(),
                    generator.identity().name(),
                    Failure::ToolTargetMismatch {
                        target: tool.target().as_str().into(),
                        host_target: host.map(|host| host.as_str().into()),
                    },
                ));
            }
        }
    }

    Ok(())
}

fn node_dependencies(graph: &ProjectGraph, node: &Node) -> Result<Vec<Node>, ProjectLoadError> {
    match node {
        Node::Product {
            product: identity,
            target,
        } => {
            let package = graph
                .package(identity.package())
                .expect("product work node must have its validated owner");

            let product = package
                .products()
                .iter()
                .find(|product| product.identity() == identity)
                .expect("product work node must name a validated product");

            let mut edges = product
                .dependencies()
                .iter()
                .filter(|dependency| dependency.is_active_for(target))
                .map(|dependency| Node::Product {
                    product: dependency.product().clone(),
                    target: target.clone(),
                })
                .collect::<Vec<_>>();

            if let Some(library) = product.tested_library() {
                edges.push(Node::Product {
                    product: library.clone(),
                    target: target.clone(),
                });
            }

            for identity in product.generated_sources() {
                if !package
                    .generators()
                    .iter()
                    .any(|generator| generator.identity() == identity)
                {
                    return Err(failure(
                        graph,
                        identity.package(),
                        identity.name(),
                        Failure::UnknownReference {
                            name: identity.qualified_name(),
                        },
                    ));
                }

                edges.push(Node::Generator {
                    generator: identity.clone(),
                    target: target.clone(),
                });
            }

            Ok(edges)
        }
        Node::Generator {
            generator: identity,
            target,
        } => {
            let generator = selected_generator(graph, identity);

            let mut edges = vec![Node::Product {
                product: generator.tool().product().clone(),
                target: generator.tool().target().clone(),
            }];

            edges.extend(generator.generated_inputs().iter().map(|(generator, _)| {
                Node::Generator {
                    generator: generator.clone(),
                    target: target.clone(),
                }
            }));

            Ok(edges)
        }
    }
}

pub(super) fn selected_generator<'a>(
    graph: &'a ProjectGraph,
    identity: &GeneratorIdentity,
) -> &'a ProjectGenerator {
    graph
        .package(identity.package())
        .and_then(|package| {
            package
                .generators()
                .iter()
                .find(|generator| generator.identity() == identity)
        })
        .expect("planned generator references must have been validated before execution")
}

pub(super) fn failure(
    graph: &ProjectGraph,
    package: &bray_symbols::PackageIdentity,
    name: &str,
    failure: Failure,
) -> ProjectLoadError {
    let package = graph
        .package(package)
        .expect("generator failure must have a validated owning package");

    let path = package
        .path()
        .beneath(std::path::Path::new(""))
        .join(crate::PACKAGE_MANIFEST_FILE_NAME);

    ProjectLoadError::Generator {
        path,
        generator: format!("{}/{}", package.identity().as_str(), name),
        failure,
    }
}

pub(super) fn target_profile<'a>(
    graph: &'a ProjectGraph,
    identity: &TargetIdentity,
) -> &'a bray_target::TargetProfile {
    graph
        .targets()
        .iter()
        .find(|target| target.identity() == identity)
        .expect("generator work target must be declared")
        .profile()
}
