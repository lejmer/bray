use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use bray_diagnostics::{DiagnosticBag, DiagnosticId};
use bray_project::{
    PackageRole, ProjectGraph, ProjectPackage, ProjectProduct, ProjectTarget, load_project_graph,
};
use bray_symbols::{PackageIdentity, ProductKind};
use bray_target::TargetIdentity;

use crate::tack::error::selection_diagnostics;
use crate::tack::model::TackSelection;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProductSelectionKind {
    Any,
    Executable,
    Test,
}

#[derive(Clone, Debug)]
pub(crate) struct PlannedProduct {
    package: PackageIdentity,
    product_name: String,
    target_name: String,
    target: TargetIdentity,
}

impl PlannedProduct {
    pub(crate) const fn package(&self) -> &PackageIdentity {
        &self.package
    }

    pub(crate) fn product_name(&self) -> &str {
        &self.product_name
    }

    pub(crate) fn target_name(&self) -> &str {
        &self.target_name
    }

    pub(crate) const fn target(&self) -> &TargetIdentity {
        &self.target
    }
}

pub(crate) fn load_graph(workspace_root: &Path) -> Result<ProjectGraph, DiagnosticBag> {
    load_project_graph(workspace_root)
        .map_err(|error| DiagnosticBag::single(error.into_diagnostic(DiagnosticId::new(0))))
}

pub(crate) fn select_products(
    graph: &ProjectGraph,
    selection: &TackSelection,
    kind: ProductSelectionKind,
    require_one: bool,
) -> Result<Vec<PlannedProduct>, DiagnosticBag> {
    let packages = select_packages(graph, selection.package.as_deref())?;
    let selected_target = select_target(graph, selection.target.as_deref())?;
    let mut products = Vec::new();

    for package in packages {
        for product in package.products() {
            if !product_matches(product, selection.product.as_deref(), kind) {
                continue;
            }

            products.extend(product_targets(graph, package, product, selected_target)?);
        }
    }

    if products.is_empty() {
        return Err(selection_diagnostics(selection_text(selection)));
    }

    if require_one && products.len() != 1 {
        return Err(selection_diagnostics("single_product_target_required"));
    }

    Ok(products)
}

pub(crate) fn root_source_files(graph: &ProjectGraph, workspace_root: &Path) -> Vec<PathBuf> {
    let mut files = BTreeSet::new();

    for package in graph
        .packages()
        .iter()
        .filter(|package| package.role() == PackageRole::Root)
    {
        for source_root in package.source_roots() {
            files.extend(
                source_root
                    .sources()
                    .iter()
                    .map(|source| source.beneath(workspace_root)),
            );
        }
    }

    files.into_iter().collect()
}

fn select_packages<'graph>(
    graph: &'graph ProjectGraph,
    selected: Option<&str>,
) -> Result<Vec<&'graph ProjectPackage>, DiagnosticBag> {
    let Some(selected) = selected else {
        return Ok(graph
            .packages()
            .iter()
            .filter(|package| package.role() == PackageRole::Root)
            .collect());
    };

    let Some(identity) = PackageIdentity::try_new(selected) else {
        return Err(selection_diagnostics(selected));
    };

    let Some(package) = graph.package(&identity) else {
        return Err(selection_diagnostics(selected));
    };

    Ok(vec![package])
}

pub(crate) fn select_target<'graph>(
    graph: &'graph ProjectGraph,
    selected: Option<&str>,
) -> Result<Option<&'graph ProjectTarget>, DiagnosticBag> {
    let Some(selected) = selected else {
        return Ok(None);
    };

    graph
        .targets()
        .iter()
        .find(|target| target.name() == selected)
        .map(Some)
        .ok_or_else(|| selection_diagnostics(selected))
}

fn product_matches(
    product: &ProjectProduct,
    selected: Option<&str>,
    kind: ProductSelectionKind,
) -> bool {
    selected.is_none_or(|name| product.identity().name() == name)
        && match kind {
            ProductSelectionKind::Any => true,
            ProductSelectionKind::Executable => product.kind() == ProductKind::Executable,
            ProductSelectionKind::Test => product.kind() == ProductKind::Test,
        }
}

fn product_targets(
    graph: &ProjectGraph,
    package: &ProjectPackage,
    product: &ProjectProduct,
    selected: Option<&ProjectTarget>,
) -> Result<Vec<PlannedProduct>, DiagnosticBag> {
    let targets = match selected {
        Some(target) if product.targets().contains(target.identity()) => vec![target],
        Some(target) => return Err(selection_diagnostics(target.name())),
        None => product
            .targets()
            .iter()
            .map(|identity| {
                graph
                    .targets()
                    .iter()
                    .find(|target| target.identity() == identity)
                    .ok_or_else(|| selection_diagnostics(identity.as_str()))
            })
            .collect::<Result<Vec<_>, _>>()?,
    };

    Ok(targets
        .into_iter()
        .map(|target| PlannedProduct {
            package: package.identity().clone(),
            product_name: product.identity().name().to_owned(),
            target_name: target.name().to_owned(),
            target: target.identity().clone(),
        })
        .collect())
}

fn selection_text(selection: &TackSelection) -> String {
    [
        selection.package.as_deref().unwrap_or("*"),
        selection.product.as_deref().unwrap_or("*"),
        selection.target.as_deref().unwrap_or("*"),
    ]
    .join("/")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use bray_project::load_project_graph;

    use super::{ProductSelectionKind, select_products};
    use crate::tack::model::TackSelection;
    use crate::test_support::ProjectWorkspace;

    #[test]
    fn selections_default_to_root_products_and_manifest_targets() {
        let workspace = ProjectWorkspace::basic();

        let graph = load_project_graph(workspace.path())
            .unwrap_or_else(|error| panic!("workspace should load: {error:?}"));

        let products = select_products(
            &graph,
            &TackSelection::default(),
            ProductSelectionKind::Any,
            false,
        )
        .unwrap_or_else(|error| panic!("default selection should resolve: {error:?}"));

        assert_eq!(products.len(), 1);
        assert_eq!(products[0].package().as_str(), "example.application");
        assert_eq!(products[0].product_name(), "application");
        assert_eq!(products[0].target_name(), "native");
    }

    #[test]
    fn selections_do_not_discover_parent_workspaces() {
        let workspace = ProjectWorkspace::basic();
        let nested = workspace.path().join("app");

        assert!(bray_project::load_project_graph(Path::new(&nested)).is_err());
    }
}
