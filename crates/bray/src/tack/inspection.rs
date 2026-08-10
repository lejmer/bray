use bray_diagnostics::DiagnosticBag;
use bray_project::{PackageRole, ProjectGraph};
use bray_tooling::OutputFormat;
use serde::Serialize;

use crate::tack::error::operation_diagnostics;

pub(crate) fn render_project_inspection(
    graph: &ProjectGraph,
    output_format: OutputFormat,
) -> Result<String, DiagnosticBag> {
    let report = ProjectInspection::from_graph(graph);

    match output_format {
        OutputFormat::Text => Ok(report.text()),
        OutputFormat::Json => serde_json::to_string_pretty(&report)
            .map(|json| format!("{json}\n"))
            .map_err(|_| operation_diagnostics("project_inspection_json")),
    }
}

#[derive(Serialize)]
struct ProjectInspection {
    kind: &'static str,
    output_root: String,
    targets: Vec<ProjectTargetInspection>,
    packages: Vec<ProjectPackageInspection>,
}

impl ProjectInspection {
    fn from_graph(graph: &ProjectGraph) -> Self {
        Self {
            kind: "project_inspection",
            output_root: graph.output_root().as_str().to_owned(),
            targets: graph
                .targets()
                .iter()
                .map(|target| ProjectTargetInspection {
                    name: target.name().to_owned(),
                    identity: target.identity().as_str().to_owned(),
                })
                .collect(),
            packages: graph
                .packages()
                .iter()
                .map(|package| ProjectPackageInspection {
                    identity: package.identity().as_str().to_owned(),
                    version: package.version().to_string(),
                    role: match package.role() {
                        PackageRole::Root => "root",
                        PackageRole::Vendored => "vendored",
                    },
                    path: package.path().as_str().to_owned(),
                    products: package
                        .products()
                        .iter()
                        .map(|product| ProjectProductInspection {
                            name: product.identity().name().to_owned(),
                            dependencies: product
                                .dependencies()
                                .iter()
                                .map(|dependency| {
                                    format!(
                                        "{}/{}",
                                        dependency.product().package().as_str(),
                                        dependency.product().name()
                                    )
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    fn text(&self) -> String {
        let mut text = format!("kind: {}\noutput_root: {}\n", self.kind, self.output_root);

        for target in &self.targets {
            text.push_str(&format!("target {}: {}\n", target.name, target.identity));
        }

        for package in &self.packages {
            text.push_str(&format!(
                "package {} {} [{}] at {}\n",
                package.identity, package.version, package.role, package.path
            ));

            for product in &package.products {
                text.push_str(&format!("  product {}\n", product.name));

                for dependency in &product.dependencies {
                    text.push_str(&format!("    dependency {dependency}\n"));
                }
            }
        }

        text
    }
}

#[derive(Serialize)]
struct ProjectTargetInspection {
    name: String,
    identity: String,
}

#[derive(Serialize)]
struct ProjectPackageInspection {
    identity: String,
    version: String,
    role: &'static str,
    path: String,
    products: Vec<ProjectProductInspection>,
}

#[derive(Serialize)]
struct ProjectProductInspection {
    name: String,
    dependencies: Vec<String>,
}
