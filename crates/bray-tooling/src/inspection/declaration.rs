use bray_compilation::Compilation;
use bray_declarations::{
    ContainerId, ContainerKind, ContainerRecord, DeclarationName, DeclarationRecord,
    DeclarationSurface, DeclarationTable, ModulePartRecord, SyntaxAnchor,
};
use bray_diagnostics::DiagnosticBag;
use serde::Serialize;

use crate::OutputFormat;
use crate::inspection::{
    InspectionOutput, InspectionSourceError, InspectionSources, InspectionSyntaxAnchor, TreeWriter,
    push_report_value, push_text_diagnostic, render_pretty_json,
};
use crate::output::{DiagnosticJson, diagnostic_jsons};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeclarationInspectionRenderError {
    Container,
    Declaration,
    ModulePart,
    Source,
    SourceIndex,
    Json,
}

impl From<InspectionSourceError> for DeclarationInspectionRenderError {
    fn from(error: InspectionSourceError) -> Self {
        match error {
            InspectionSourceError::Source => Self::Source,
            InspectionSourceError::SourceIndex => Self::SourceIndex,
        }
    }
}

pub(crate) fn render_declaration_inspection(
    compilation: &Compilation,
    output_format: OutputFormat,
) -> Result<InspectionOutput, DeclarationInspectionRenderError> {
    let result = compilation.declaration_table_result();

    let diagnostics = compilation
        .syntax_tree_result()
        .diagnostics()
        .merged(result.diagnostics());

    let report =
        DeclarationInspectionReport::from_compilation(compilation, result.table(), &diagnostics)?;

    let stdout = match output_format {
        OutputFormat::Text => render_text_report(&report),
        OutputFormat::Json => {
            render_pretty_json(&report).map_err(|_| DeclarationInspectionRenderError::Json)?
        }
    };

    Ok(InspectionOutput::new(stdout, diagnostics))
}

#[derive(Serialize)]
struct DeclarationInspectionReport {
    kind: &'static str,
    declaration_count: usize,
    container_count: usize,
    module_part_count: usize,
    has_errors: bool,
    root: InspectionContainer,
    diagnostics: Vec<DiagnosticJson>,
}

impl DeclarationInspectionReport {
    fn from_compilation(
        compilation: &Compilation,
        table: &DeclarationTable,
        diagnostics: &DiagnosticBag,
    ) -> Result<Self, DeclarationInspectionRenderError> {
        let sources = InspectionSources::new(compilation.sources())?;

        let root = table
            .container(table.root_container())
            .ok_or(DeclarationInspectionRenderError::Container)?;

        let root = InspectionContainer::from_record(&sources, table, root)?;

        let diagnostic_jsons = diagnostic_jsons(diagnostics, Some(compilation.sources()));

        Ok(Self {
            kind: "declaration_inspection",
            declaration_count: table.declarations().len(),
            container_count: table.containers().len(),
            module_part_count: table.module_parts().len(),
            has_errors: diagnostics.has_errors(),
            root,
            diagnostics: diagnostic_jsons,
        })
    }
}

#[derive(Serialize)]
struct InspectionContainer {
    container_kind: &'static str,
    id: u32,
    module_path: Option<String>,
    contributions: Vec<InspectionModulePart>,
    declarations: Vec<InspectionDeclaration>,
    modules: Vec<InspectionContainer>,
}

impl InspectionContainer {
    fn from_record(
        sources: &InspectionSources<'_>,
        table: &DeclarationTable,
        container: &ContainerRecord,
    ) -> Result<Self, DeclarationInspectionRenderError> {
        let contributions = container
            .module_parts()
            .iter()
            .map(|id| {
                table
                    .module_part(*id)
                    .ok_or(DeclarationInspectionRenderError::ModulePart)
                    .and_then(|part| InspectionModulePart::from_record(sources, table, part))
            })
            .collect::<Result<_, _>>()?;

        let declarations = if matches!(
            container.kind(),
            ContainerKind::Root | ContainerKind::Module
        ) {
            Vec::new()
        } else {
            container
                .declarations()
                .iter()
                .map(|id| {
                    table
                        .declaration(*id)
                        .ok_or(DeclarationInspectionRenderError::Declaration)
                        .and_then(|declaration| {
                            InspectionDeclaration::from_record(sources, table, declaration)
                        })
                })
                .collect::<Result<_, _>>()?
        };

        let modules = if container.kind() == ContainerKind::Root {
            table
                .module_containers()
                .map(|module| Self::from_record(sources, table, module))
                .collect::<Result<_, _>>()?
        } else {
            Vec::new()
        };

        Ok(Self {
            container_kind: container.kind().as_str(),
            id: container.id().raw(),
            module_path: container.module_path().map(|path| path.dotted()),
            contributions,
            declarations,
            modules,
        })
    }

    fn push_text(&self, writer: &mut TreeWriter, is_last: bool) {
        writer.push_line(is_last, &self.text_line());
        writer.enter_children(is_last);

        let child_count = self.contributions.len() + self.declarations.len() + self.modules.len();

        let mut child_index = 0;

        for contribution in &self.contributions {
            child_index += 1;
            contribution.push_text(writer, child_index == child_count);
        }

        for declaration in &self.declarations {
            child_index += 1;
            declaration.push_text(writer, child_index == child_count);
        }

        for module in &self.modules {
            child_index += 1;
            module.push_text(writer, child_index == child_count);
        }

        writer.leave_children();
    }

    fn text_line(&self) -> String {
        match &self.module_path {
            Some(path) => format!("{} {path} [container:{}]", self.container_kind, self.id),
            None => format!("{} [container:{}]", self.container_kind, self.id),
        }
    }
}

#[derive(Serialize)]
struct InspectionModulePart {
    part_kind: &'static str,
    id: u32,
    declaration_id: u32,
    anchor: InspectionSyntaxAnchor,
    surface: InspectionSurface,
    declarations: Vec<InspectionDeclaration>,
}

impl InspectionModulePart {
    fn from_record(
        sources: &InspectionSources<'_>,
        table: &DeclarationTable,
        part: &ModulePartRecord,
    ) -> Result<Self, DeclarationInspectionRenderError> {
        let declarations = part
            .declarations()
            .iter()
            .map(|id| {
                table
                    .declaration(*id)
                    .ok_or(DeclarationInspectionRenderError::Declaration)
                    .and_then(|declaration| {
                        InspectionDeclaration::from_record(sources, table, declaration)
                    })
            })
            .collect::<Result<_, _>>()?;

        Ok(Self {
            part_kind: "module_contribution",
            id: part.id().raw(),
            declaration_id: part.declaration().raw(),
            anchor: InspectionSyntaxAnchor::from_anchor(sources, part.syntax_anchor())?,
            surface: InspectionSurface::from_surface(sources, part.surface())?,
            declarations,
        })
    }

    fn push_text(&self, writer: &mut TreeWriter, is_last: bool) {
        writer.push_line(
            is_last,
            &format!(
                "{} {} {} [part:{} declaration:{}]{}",
                self.part_kind,
                self.anchor.display_name(),
                self.anchor.location_text(),
                self.id,
                self.declaration_id,
                self.anchor.recovery_text()
            ),
        );

        writer.enter_children(is_last);

        let surface_entries = self.surface.text_entries();
        let child_count = surface_entries.len() + self.declarations.len();
        let mut child_index = 0;

        for entry in surface_entries {
            child_index += 1;
            writer.push_line(child_index == child_count, &entry);
        }

        for declaration in &self.declarations {
            child_index += 1;
            declaration.push_text(writer, child_index == child_count);
        }

        writer.leave_children();
    }
}

#[derive(Serialize)]
struct InspectionDeclaration {
    declaration_kind: &'static str,
    id: u32,
    name: Option<InspectionDeclarationName>,
    owning_container: u32,
    anchor: InspectionSyntaxAnchor,
    surface: InspectionSurface,
    child_container: Option<Box<InspectionContainer>>,
}

impl InspectionDeclaration {
    fn from_record(
        sources: &InspectionSources<'_>,
        table: &DeclarationTable,
        declaration: &DeclarationRecord,
    ) -> Result<Self, DeclarationInspectionRenderError> {
        let child_container = declaration
            .child_container()
            .map(|id| inspection_container(sources, table, id))
            .transpose()?
            .map(Box::new);

        Ok(Self {
            declaration_kind: declaration.kind().as_str(),
            id: declaration.id().raw(),
            name: declaration.name().map(InspectionDeclarationName::from_name),
            owning_container: declaration.owning_container().raw(),
            anchor: InspectionSyntaxAnchor::from_anchor(sources, declaration.syntax_anchor())?,
            surface: InspectionSurface::from_surface(sources, declaration.surface())?,
            child_container,
        })
    }

    fn push_text(&self, writer: &mut TreeWriter, is_last: bool) {
        let name = self
            .name
            .as_ref()
            .map(|name| format!(" {}", name.text()))
            .unwrap_or_default();

        writer.push_line(
            is_last,
            &format!(
                "{}{name} [declaration:{}] {} {}{}",
                self.declaration_kind,
                self.id,
                self.anchor.display_name(),
                self.anchor.location_text(),
                self.anchor.recovery_text()
            ),
        );

        let surface_entries = self.surface.text_entries();

        if surface_entries.is_empty() && self.child_container.is_none() {
            return;
        }

        writer.enter_children(is_last);

        let child_count = surface_entries.len() + usize::from(self.child_container.is_some());
        let mut child_index = 0;

        for entry in surface_entries {
            child_index += 1;
            writer.push_line(child_index == child_count, &entry);
        }

        if let Some(container) = &self.child_container {
            container.push_text(writer, true);
        }

        writer.leave_children();
    }
}

fn inspection_container(
    sources: &InspectionSources<'_>,
    table: &DeclarationTable,
    id: ContainerId,
) -> Result<InspectionContainer, DeclarationInspectionRenderError> {
    let container = table
        .container(id)
        .ok_or(DeclarationInspectionRenderError::Container)?;

    InspectionContainer::from_record(sources, table, container)
}

#[derive(Serialize)]
#[serde(tag = "name_kind", content = "value", rename_all = "snake_case")]
enum InspectionDeclarationName {
    Identifier(String),
    Keyword(&'static str),
    Path(String),
    Implementation {
        subject: String,
        trait_path: Option<String>,
    },
}

impl InspectionDeclarationName {
    fn from_name(name: &DeclarationName) -> Self {
        match name {
            DeclarationName::Identifier(name) => Self::Identifier(name.clone()),
            DeclarationName::Keyword(kind) => Self::Keyword(kind.as_str()),
            DeclarationName::Path(path) => Self::Path(path.dotted()),
            DeclarationName::Implementation(name) => Self::Implementation {
                subject: name.subject().dotted(),
                trait_path: name.trait_path().map(|path| path.dotted()),
            },
        }
    }

    fn text(&self) -> String {
        match self {
            Self::Identifier(name) | Self::Path(name) => name.clone(),
            Self::Keyword(kind) => String::from(*kind),
            Self::Implementation {
                subject,
                trait_path,
            } => match trait_path {
                Some(trait_path) => format!("{trait_path} for {subject}"),
                None => subject.clone(),
            },
        }
    }
}

#[derive(Serialize)]
struct InspectionSurface {
    visibility: Option<&'static str>,
    modifiers: Vec<&'static str>,
    directives: Vec<InspectionSyntaxAnchor>,
    constraints: Vec<InspectionSyntaxAnchor>,
    contract_clauses: Vec<InspectionSyntaxAnchor>,
    runtime_default: Option<InspectionSyntaxAnchor>,
    overload_arms: Vec<InspectionSyntaxAnchor>,
}

impl InspectionSurface {
    fn from_surface(
        sources: &InspectionSources<'_>,
        surface: &DeclarationSurface,
    ) -> Result<Self, DeclarationInspectionRenderError> {
        Ok(Self {
            visibility: surface.visibility().map(|kind| kind.as_str()),
            modifiers: surface
                .modifiers()
                .iter()
                .map(|kind| kind.as_str())
                .collect(),
            directives: inspection_anchors(sources, surface.directives())?,
            constraints: inspection_anchors(sources, surface.constraints())?,
            contract_clauses: inspection_anchors(sources, surface.contract_clauses())?,
            runtime_default: surface
                .runtime_default()
                .map(|anchor| InspectionSyntaxAnchor::from_anchor(sources, anchor))
                .transpose()?,
            overload_arms: inspection_anchors(sources, surface.overload_arms())?,
        })
    }

    fn text_entries(&self) -> Vec<String> {
        let mut entries = Vec::new();

        if let Some(visibility) = self.visibility {
            entries.push(format!("visibility {visibility}"));
        }

        if !self.modifiers.is_empty() {
            entries.push(format!("modifiers {}", self.modifiers.join(", ")));
        }

        push_anchor_entries(&mut entries, "directive", &self.directives);
        push_anchor_entries(&mut entries, "constraint", &self.constraints);
        push_anchor_entries(&mut entries, "contract_clause", &self.contract_clauses);

        if let Some(default) = &self.runtime_default {
            entries.push(anchor_text("runtime_default", default));
        }

        push_anchor_entries(&mut entries, "overload_arm", &self.overload_arms);

        entries
    }
}

fn inspection_anchors(
    sources: &InspectionSources<'_>,
    anchors: &[SyntaxAnchor],
) -> Result<Vec<InspectionSyntaxAnchor>, DeclarationInspectionRenderError> {
    anchors
        .iter()
        .map(|anchor| InspectionSyntaxAnchor::from_anchor(sources, *anchor).map_err(Into::into))
        .collect::<Result<_, _>>()
}

fn push_anchor_entries(entries: &mut Vec<String>, label: &str, anchors: &[InspectionSyntaxAnchor]) {
    entries.extend(anchors.iter().map(|anchor| anchor_text(label, anchor)));
}

fn anchor_text(label: &str, anchor: &InspectionSyntaxAnchor) -> String {
    format!("{label} {}", anchor.text())
}

fn render_text_report(report: &DeclarationInspectionReport) -> String {
    let mut output = String::new();

    push_report_value(&mut output, "kind", report.kind);
    push_report_value(&mut output, "declaration_count", report.declaration_count);
    push_report_value(&mut output, "container_count", report.container_count);
    push_report_value(&mut output, "module_part_count", report.module_part_count);
    push_report_value(&mut output, "has_errors", report.has_errors);
    output.push_str("tree:\n");

    let mut writer = TreeWriter::new("  ");

    report.root.push_text(&mut writer, true);
    output.push_str(&writer.into_string());
    output.push_str("diagnostics:\n");

    if report.diagnostics.is_empty() {
        output.push_str("  none\n");
    } else {
        for diagnostic in &report.diagnostics {
            push_text_diagnostic(&mut output, diagnostic);
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use bray_compilation::Compilation;
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};

    use super::render_declaration_inspection;
    use crate::OutputFormat;
    use crate::test_support::package_identity;

    #[test]
    fn text_inspection_merges_partial_modules_and_keeps_contributions_visible() {
        let compilation = compilation([
            concat!(
                "module app\n",
                "{\n",
                "    public struct first\n",
                "    {\n",
                "        value: i32;\n",
                "    }\n",
                "}\n",
            ),
            concat!(
                "module app\n",
                "{\n",
                "    func second()\n",
                "    {\n",
                "    }\n",
                "}\n",
            ),
        ]);

        let output = match render_declaration_inspection(&compilation, OutputFormat::Text) {
            Ok(output) => output,
            Err(error) => panic!("declaration inspection should render: {error:?}"),
        };

        let (text, diagnostics) = output.into_parts();

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(text.matches("module app [container:").count(), 1);
        assert_eq!(text.matches("module_contribution").count(), 2);
        assert!(text.contains("struct first"));
        assert!(text.contains("struct_field value"));
        assert!(text.contains("function second"));
        assert!(text.contains("├─ module_contribution"));
    }

    #[test]
    fn json_inspection_preserves_typed_names_surface_and_recovery() {
        let compilation = compilation([concat!(
            "module app\n",
            "{\n",
            "    public trusted func run(value: i32)\n",
            "    {\n",
            "    }\n",
        )]);

        let output = match render_declaration_inspection(&compilation, OutputFormat::Json) {
            Ok(output) => output,
            Err(error) => panic!("declaration inspection should render: {error:?}"),
        };

        let (json, diagnostics) = output.into_parts();

        assert!(!diagnostics.is_empty());

        let value: serde_json::Value = match serde_json::from_str(&json) {
            Ok(value) => value,
            Err(error) => panic!("declaration inspection should be JSON: {error:?}"),
        };

        let module = &value["root"]["modules"][0];
        let function = &module["contributions"][0]["declarations"][0];

        assert_eq!(module["container_kind"], "module");
        assert_eq!(function["declaration_kind"], "function");
        assert_eq!(function["name"]["name_kind"], "identifier");
        assert_eq!(function["name"]["value"], "run");
        assert_eq!(function["surface"]["visibility"], "public_keyword");
        assert_eq!(function["surface"]["modifiers"][0], "trusted_keyword");
        assert_eq!(module["contributions"][0]["anchor"]["recovered"], true);

        assert_eq!(
            function["child_container"]["declarations"][0]["declaration_kind"],
            "callable_parameter"
        );
    }

    fn compilation<const N: usize>(sources: [&str; N]) -> Compilation {
        let inputs = sources
            .into_iter()
            .enumerate()
            .map(|(index, text)| {
                SourceInput::virtual_text(
                    SourceIdentity::new(u32::try_from(index).unwrap_or(u32::MAX)),
                    format!("source-{index}.bray"),
                    SourceVersion::new(0),
                    text,
                )
            })
            .collect::<Vec<_>>();

        match Compilation::load_sources(package_identity(), inputs) {
            Ok(compilation) => compilation,
            Err(_) => panic!("test compilation should load"),
        }
    }
}
