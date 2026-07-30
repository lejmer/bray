use bray_compilation::Compilation;
use bray_diagnostics::DiagnosticBag;
use bray_project::{PackageRole, ProjectGraph};
use serde::Serialize;

use crate::command::UnitInspectionTarget;
use crate::inspection::{
    render_bound_inspection, render_declaration_inspection,
    render_lowered_inspection, render_mir_inspection,
    render_source_inspection, render_symbol_inspection,
    render_syntax_inspection, render_token_inspection,
};
use crate::tack::error::operation_diagnostics;
use crate::tack::model::TackInspection;
use crate::DriverOutputFormat;

pub(crate) fn render_project_inspection(
    graph: &ProjectGraph,
    output_format: DriverOutputFormat,
) -> Result<String, DiagnosticBag> {
    let report = ProjectInspection::from_graph(graph);

    match output_format {
        DriverOutputFormat::Text => Ok(report.text()),
        DriverOutputFormat::Json => serde_json::to_string_pretty(&report)
            .map(|json| format!("{json}\n"))
            .map_err(|_| operation_diagnostics("project_inspection_json")),
    }
}

pub(crate) fn render_compiler_inspection(
    compilation: &Compilation,
    inspection: TackInspection,
    source_id: u32,
    position: Option<bray_source::TextSize>,
    output_format: DriverOutputFormat,
) -> Result<(String, DiagnosticBag), DiagnosticBag> {
    if inspection == TackInspection::Source {
        let stdout = render_source_inspection(compilation, output_format)
            .map_err(|_| operation_diagnostics("source_inspection"))?;

        return Ok((stdout, DiagnosticBag::new()));
    }

    let target = position.map_or_else(
        || UnitInspectionTarget::source(source_id),
        |position| UnitInspectionTarget::at(source_id, position),
    );

    let output = match inspection {
        TackInspection::Tokens => render_token_inspection(compilation, output_format)
            .map_err(|_| operation_diagnostics("token_inspection"))?,
        TackInspection::Syntax => render_syntax_inspection(compilation, output_format)
            .map_err(|_| operation_diagnostics("syntax_inspection"))?,
        TackInspection::Declarations => {
            render_declaration_inspection(compilation, output_format)
                .map_err(|_| operation_diagnostics("declaration_inspection"))?
        }
        TackInspection::Symbols => render_symbol_inspection(compilation, output_format)
            .map_err(|_| operation_diagnostics("symbol_inspection"))?,
        TackInspection::Bound => render_bound_inspection(compilation, target, output_format)
            .map_err(|_| operation_diagnostics("bound_inspection"))?,
        TackInspection::Lowered => {
            render_lowered_inspection(compilation, target, output_format)
                .map_err(|_| operation_diagnostics("lowered_inspection"))?
        }
        TackInspection::Mir => render_mir_inspection(compilation, target, output_format)
            .map_err(|_| operation_diagnostics("mir_inspection"))?,
        TackInspection::Project | TackInspection::Source => {
            return Err(operation_diagnostics("inspection_routing"));
        }
    };

    Ok(output.into_parts())
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
                    role: match package.role() {
                        PackageRole::Root => "root",
                        PackageRole::Vendored => "vendored",
                    },
                    path: package.path().as_str().to_owned(),
                    dependencies: package
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
                    products: package
                        .products()
                        .iter()
                        .map(|product| product.identity().name().to_owned())
                        .collect(),
                })
                .collect(),
        }
    }

    fn text(&self) -> String {
        let mut text = format!(
            "kind: {}\noutput_root: {}\n",
            self.kind, self.output_root
        );

        for target in &self.targets {
            text.push_str(&format!(
                "target {}: {}\n",
                target.name, target.identity
            ));
        }

        for package in &self.packages {
            text.push_str(&format!(
                "package {} [{}] at {}\n",
                package.identity, package.role, package.path
            ));

            for dependency in &package.dependencies {
                text.push_str(&format!("  dependency {dependency}\n"));
            }

            for product in &package.products {
                text.push_str(&format!("  product {product}\n"));
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
    role: &'static str,
    path: String,
    dependencies: Vec<String>,
    products: Vec<String>,
}
