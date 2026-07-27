use std::collections::{BTreeMap, BTreeSet};

use bray_compilation::Compilation;
use bray_declarations::{DeclarationId, DeclarationTable, SyntaxAnchor};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnySymbolId, SymbolGraph, SymbolKind, SymbolOrigin, SymbolRelationshipKind,
};
use serde::Serialize;

use crate::command::DriverOutputFormat;
use crate::diagnostic_output::{DiagnosticJson, diagnostic_jsons};
use crate::inspection::{
    InspectionOutput, InspectionSourceError, InspectionSources, InspectionSyntaxAnchor, TreeWriter,
    push_text_diagnostic,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SymbolInspectionRenderError {
    Declaration,
    Graph,
    Json,
    Source,
    SourceIndex,
    Symbol,
    SymbolCycle,
}

impl From<InspectionSourceError> for SymbolInspectionRenderError {
    fn from(error: InspectionSourceError) -> Self {
        match error {
            InspectionSourceError::Source => Self::Source,
            InspectionSourceError::SourceIndex => Self::SourceIndex,
        }
    }
}

pub(crate) fn render_symbol_inspection(
    compilation: &Compilation,
    output_format: DriverOutputFormat,
) -> Result<InspectionOutput, SymbolInspectionRenderError> {
    let declaration_result = compilation.declaration_table_result();

    let symbols = compilation
        .symbol_graph()
        .map_err(|_| SymbolInspectionRenderError::Graph)?;

    let diagnostics = compilation
        .syntax_tree_result()
        .diagnostics()
        .merged(declaration_result.diagnostics());

    let report = SymbolInspectionReport::from_compilation(
        compilation,
        declaration_result.table(),
        symbols,
        &diagnostics,
    )?;

    let stdout = match output_format {
        DriverOutputFormat::Text => render_text_report(&report),
        DriverOutputFormat::Json => render_json_report(&report)?,
    };

    Ok(InspectionOutput::new(stdout, diagnostics))
}

#[derive(Serialize)]
struct SymbolInspectionReport {
    kind: &'static str,
    symbol_count: usize,
    root_count: usize,
    has_errors: bool,
    roots: Vec<InspectionRootGroup>,
    diagnostics: Vec<DiagnosticJson>,
}

impl SymbolInspectionReport {
    fn from_compilation(
        compilation: &Compilation,
        declarations: &DeclarationTable,
        symbols: &SymbolGraph,
        diagnostics: &DiagnosticBag,
    ) -> Result<Self, SymbolInspectionRenderError> {
        let sources = InspectionSources::new(compilation.sources())?;
        let context = SymbolInspectionContext::new(symbols, declarations, sources);

        let roots = context.roots()?;
        let root_count = roots.iter().map(|group| group.symbols.len()).sum();

        Ok(Self {
            kind: "symbol_inspection",
            symbol_count: symbols.symbols().count(),
            root_count,
            has_errors: diagnostics.has_errors(),
            roots,
            diagnostics: diagnostic_jsons(diagnostics, Some(compilation.sources())),
        })
    }
}

struct SymbolInspectionContext<'model, 'source> {
    symbols: &'model SymbolGraph,
    declarations: &'model DeclarationTable,
    sources: InspectionSources<'source>,
    children: BTreeMap<AnySymbolId, Vec<AnySymbolId>>,
}

impl<'model, 'source> SymbolInspectionContext<'model, 'source> {
    fn new(
        symbols: &'model SymbolGraph,
        declarations: &'model DeclarationTable,
        sources: InspectionSources<'source>,
    ) -> Self {
        let mut children = BTreeMap::<_, Vec<_>>::new();

        for symbol in symbols.symbols() {
            if let Some(owner) = symbols.containing_symbol(symbol) {
                children.entry(owner).or_default().push(symbol);
            }
        }

        for members in children.values_mut() {
            members.sort_by_key(|symbol| symbol.symbol_id().raw());
        }

        Self {
            symbols,
            declarations,
            sources,
            children,
        }
    }

    fn roots(&self) -> Result<Vec<InspectionRootGroup>, SymbolInspectionRenderError> {
        let mut root_ids = self
            .symbols
            .symbols()
            .filter(|symbol| self.symbols.containing_symbol(*symbol).is_none())
            .collect::<Vec<_>>();

        root_ids.sort_by_key(|symbol| symbol.symbol_id().raw());

        let mut groups = Vec::<InspectionRootGroup>::new();

        for origin in [
            SymbolOrigin::CompilerKnown,
            SymbolOrigin::Source,
            SymbolOrigin::Imported,
            SymbolOrigin::CompilerProvided,
            SymbolOrigin::Synthesized,
        ] {
            let mut group = InspectionRootGroup::new(origin);

            for symbol in root_ids.iter().copied().filter(|symbol| {
                self.symbols.symbol_origin(*symbol) == Some(origin)
            }) {
                group.symbols.push(self.symbol(symbol, &mut BTreeSet::new())?);
            }

            if !group.symbols.is_empty() {
                groups.push(group);
            }
        }

        Ok(groups)
    }

    fn symbol(
        &self,
        id: AnySymbolId,
        ancestors: &mut BTreeSet<AnySymbolId>,
    ) -> Result<InspectionSymbol, SymbolInspectionRenderError> {
        if !ancestors.insert(id) {
            return Err(SymbolInspectionRenderError::SymbolCycle);
        }

        let mut relationships = Vec::<InspectionRelationship>::new();

        for child in self.children.get(&id).into_iter().flatten().copied() {
            let relationship_kind = relationship_kind(id.kind(), child.kind());
            let child = self.symbol(child, ancestors)?;

            match relationships
                .iter_mut()
                .find(|relationship| relationship.relationship_kind == relationship_kind)
            {
                Some(relationship) => relationship.symbols.push(child),
                None => relationships.push(InspectionRelationship::with_symbol(
                    relationship_kind,
                    child,
                )),
            }
        }

        let overload_references = self.overload_references(id)?;

        if !overload_references.is_empty() {
            relationships.push(InspectionRelationship::with_references(
                SymbolRelationshipKind::OverloadArm.as_str(),
                overload_references,
            ));
        }

        ancestors.remove(&id);

        let origin = self
            .symbols
            .symbol_origin(id)
            .ok_or(SymbolInspectionRenderError::Symbol)?;

        let declaration_origins = self.declaration_origins(id)?;

        Ok(InspectionSymbol {
            symbol_kind: id.kind().as_str(),
            id: id.symbol_id().raw(),
            name: symbol_name(self.symbols, id),
            origin: origin.as_str(),
            visibility: self
                .symbols
                .symbol_visibility(id)
                .map(|visibility| visibility.as_str()),
            modifiers: self.modifiers(id)?,
            recovered: self
                .symbols
                .symbol_is_recovered(id)
                .ok_or(SymbolInspectionRenderError::Symbol)?,
            declaration_origins,
            relationships,
        })
    }

    fn declaration_origins(
        &self,
        id: AnySymbolId,
    ) -> Result<Vec<InspectionSyntaxAnchor>, SymbolInspectionRenderError> {
        let declarations = match id {
            AnySymbolId::Module(module) => self
                .symbols
                .module(module)
                .ok_or(SymbolInspectionRenderError::Symbol)?
                .declarations()
                .to_vec(),
            _ => self
                .symbols
                .symbol_declaration(id)
                .into_iter()
                .collect(),
        };

        declarations
            .into_iter()
            .map(|declaration| self.declaration_origin(declaration))
            .collect()
    }

    fn declaration_origin(
        &self,
        declaration: DeclarationId,
    ) -> Result<InspectionSyntaxAnchor, SymbolInspectionRenderError> {
        let declaration = self
            .declarations
            .declaration(declaration)
            .ok_or(SymbolInspectionRenderError::Declaration)?;

        InspectionSyntaxAnchor::from_anchor(&self.sources, declaration.syntax_anchor())
            .map_err(Into::into)
    }

    fn modifiers(
        &self,
        id: AnySymbolId,
    ) -> Result<Vec<&'static str>, SymbolInspectionRenderError> {
        let declarations = match id {
            AnySymbolId::Module(module) => self
                .symbols
                .module(module)
                .ok_or(SymbolInspectionRenderError::Symbol)?
                .declarations()
                .to_vec(),
            _ => self
                .symbols
                .symbol_declaration(id)
                .into_iter()
                .collect(),
        };

        let mut modifiers = BTreeSet::new();

        for declaration in declarations {
            let declaration = self
                .declarations
                .declaration(declaration)
                .ok_or(SymbolInspectionRenderError::Declaration)?;

            modifiers.extend(
                declaration
                    .surface()
                    .modifiers()
                    .iter()
                    .map(|modifier| modifier.as_str()),
            );
        }

        Ok(modifiers.into_iter().collect())
    }

    fn overload_references(
        &self,
        id: AnySymbolId,
    ) -> Result<Vec<InspectionSymbolReference>, SymbolInspectionRenderError> {
        match id {
            AnySymbolId::CallableOverload(id) => {
                let overload = self
                    .symbols
                    .callable_overload(id)
                    .ok_or(SymbolInspectionRenderError::Symbol)?;

                self.references(overload.arm_syntax(), overload.arms())
            }
            AnySymbolId::ImplementationOverload(id) => {
                let overload = self
                    .symbols
                    .implementation_overload(id)
                    .ok_or(SymbolInspectionRenderError::Symbol)?;

                self.references(overload.arm_syntax(), overload.arms())
            }
            _ => Ok(Vec::new()),
        }
    }

    fn references(
        &self,
        syntax: &[SyntaxAnchor],
        symbols: &[AnySymbolId],
    ) -> Result<Vec<InspectionSymbolReference>, SymbolInspectionRenderError> {
        let mut references = syntax
            .iter()
            .map(|anchor| {
                InspectionSyntaxAnchor::from_anchor(&self.sources, *anchor)
                    .map(InspectionSymbolReference::Syntax)
                    .map_err(Into::into)
            })
            .collect::<Result<Vec<_>, SymbolInspectionRenderError>>()?;

        references.extend(
            symbols
                .iter()
                .copied()
                .map(|symbol| InspectionSymbolReference::Symbol {
                    symbol_kind: symbol.kind().as_str(),
                    id: symbol.symbol_id().raw(),
                    name: symbol_name(self.symbols, symbol),
                }),
        );

        Ok(references)
    }
}

#[derive(Serialize)]
struct InspectionRootGroup {
    origin: &'static str,
    symbols: Vec<InspectionSymbol>,
}

impl InspectionRootGroup {
    fn new(origin: SymbolOrigin) -> Self {
        Self {
            origin: origin.as_str(),
            symbols: Vec::new(),
        }
    }

    fn push_text(&self, writer: &mut TreeWriter, is_last: bool) {
        writer.push_line(is_last, self.origin);
        writer.enter_children(is_last);

        let last_index = self.symbols.len().saturating_sub(1);

        for (index, symbol) in self.symbols.iter().enumerate() {
            symbol.push_text(writer, index == last_index);
        }

        writer.leave_children();
    }
}

#[derive(Serialize)]
struct InspectionSymbol {
    symbol_kind: &'static str,
    id: u32,
    name: Option<String>,
    origin: &'static str,
    visibility: Option<&'static str>,
    modifiers: Vec<&'static str>,
    recovered: bool,
    declaration_origins: Vec<InspectionSyntaxAnchor>,
    relationships: Vec<InspectionRelationship>,
}

impl InspectionSymbol {
    fn push_text(&self, writer: &mut TreeWriter, is_last: bool) {
        writer.push_line(is_last, &self.text_line());

        if self.declaration_origins.is_empty() && self.relationships.is_empty() {
            return;
        }

        writer.enter_children(is_last);

        let child_count =
            usize::from(!self.declaration_origins.is_empty()) + self.relationships.len();

        let mut child_index = 0;

        if !self.declaration_origins.is_empty() {
            child_index += 1;

            push_declaration_origins(
                writer,
                child_index == child_count,
                &self.declaration_origins,
            );
        }

        for relationship in &self.relationships {
            child_index += 1;
            relationship.push_text(writer, child_index == child_count);
        }

        writer.leave_children();
    }

    fn text_line(&self) -> String {
        let name = self
            .name
            .as_ref()
            .map(|name| format!(" {name}"))
            .unwrap_or_default();

        let visibility = self
            .visibility
            .map(|visibility| format!(" visibility:{visibility}"))
            .unwrap_or_default();

        let modifiers = if self.modifiers.is_empty() {
            String::new()
        } else {
            format!(" modifiers:{}", self.modifiers.join(","))
        };

        let recovered = if self.recovered { " recovered" } else { "" };

        format!(
            "{}{name} [symbol:{} origin:{}{visibility}{modifiers}{recovered}]",
            self.symbol_kind, self.id, self.origin
        )
    }
}

fn push_declaration_origins(
    writer: &mut TreeWriter,
    is_last: bool,
    origins: &[InspectionSyntaxAnchor],
) {
    writer.push_line(is_last, "declaration_origins");
    writer.enter_children(is_last);

    let last_index = origins.len().saturating_sub(1);

    for (index, origin) in origins.iter().enumerate() {
        writer.push_line(index == last_index, &origin.text());
    }

    writer.leave_children();
}

#[derive(Serialize)]
struct InspectionRelationship {
    relationship_kind: &'static str,
    symbols: Vec<InspectionSymbol>,
    references: Vec<InspectionSymbolReference>,
}

impl InspectionRelationship {
    fn with_symbol(relationship_kind: &'static str, symbol: InspectionSymbol) -> Self {
        Self {
            relationship_kind,
            symbols: vec![symbol],
            references: Vec::new(),
        }
    }

    fn with_references(
        relationship_kind: &'static str,
        references: Vec<InspectionSymbolReference>,
    ) -> Self {
        Self {
            relationship_kind,
            symbols: Vec::new(),
            references,
        }
    }

    fn push_text(&self, writer: &mut TreeWriter, is_last: bool) {
        writer.push_line(is_last, relationship_group_text(self.relationship_kind));
        writer.enter_children(is_last);

        let child_count = self.symbols.len() + self.references.len();
        let mut child_index = 0;

        for symbol in &self.symbols {
            child_index += 1;
            symbol.push_text(writer, child_index == child_count);
        }

        for reference in &self.references {
            child_index += 1;
            writer.push_line(child_index == child_count, &reference.text());
        }

        writer.leave_children();
    }
}

#[derive(Serialize)]
#[serde(tag = "reference_kind", rename_all = "snake_case")]
enum InspectionSymbolReference {
    Symbol {
        symbol_kind: &'static str,
        id: u32,
        name: Option<String>,
    },
    Syntax(InspectionSyntaxAnchor),
}

impl InspectionSymbolReference {
    fn text(&self) -> String {
        match self {
            Self::Symbol {
                symbol_kind,
                id,
                name,
            } => {
                let name = name
                    .as_ref()
                    .map(|name| format!(" {name}"))
                    .unwrap_or_default();

                format!("symbol_reference {symbol_kind}{name} [symbol:{id}]")
            }
            Self::Syntax(anchor) => format!("syntax_reference {}", anchor.text()),
        }
    }
}

fn symbol_name(symbols: &SymbolGraph, id: AnySymbolId) -> Option<String> {
    match id {
        AnySymbolId::CompilerKnownEnvironment(_) => Some(String::from("compiler-known")),
        AnySymbolId::Package(id) => symbols
            .package(id)
            .map(|package| package.identity().as_str().to_owned()),
        AnySymbolId::Module(id) => symbols.module(id).map(|module| {
            let path = module.path().segments().collect::<Vec<_>>().join(".");

            if path.is_empty() {
                String::from("<recovered>")
            } else {
                path
            }
        }),
        _ => symbols
            .member_name(id)
            .map(|name| name.as_str().to_owned()),
    }
}

fn relationship_kind(owner: SymbolKind, member: SymbolKind) -> &'static str {
    if let Some(relationship) = SymbolRelationshipKind::between(owner, member) {
        return relationship.as_str();
    }

    if owner == SymbolKind::CompilerKnownEnvironment && member == SymbolKind::Module {
        return "compiler_known_module";
    }

    if owner == SymbolKind::CompilerKnownEnvironment {
        return "compiler_known_member";
    }

    if owner.is_implementation() {
        return "implementation_member";
    }

    "member"
}

fn relationship_group_text(relationship: &str) -> &str {
    match relationship {
        "package_module" | "compiler_known_module" => "modules",
        "module_member" | "compiler_known_member" | "member" => "members",
        "type_member" => "type_members",
        "trait_member" => "trait_members",
        "implementation_member" => "implementation_members",
        "struct_field" => "fields",
        "union_variant" => "variants",
        "union_payload_field" => "payload_fields",
        "generic_parameter" => "generic_parameters",
        "callable_parameter" => "parameters",
        "predicate_parameter" => "predicate_parameters",
        "overload_arm" => "overload_arms",
        "implementation_fulfillment" => "fulfillments",
        "default_provider" => "default_providers",
        _ => relationship,
    }
}

fn render_text_report(report: &SymbolInspectionReport) -> String {
    let mut output = String::new();

    output.push_str("kind: ");
    output.push_str(report.kind);
    output.push('\n');
    output.push_str("symbol_count: ");
    output.push_str(&report.symbol_count.to_string());
    output.push('\n');
    output.push_str("root_count: ");
    output.push_str(&report.root_count.to_string());
    output.push('\n');
    output.push_str("has_errors: ");
    output.push_str(&report.has_errors.to_string());
    output.push('\n');
    output.push_str("tree:\n");

    let mut writer = TreeWriter::new("  ");
    let last_index = report.roots.len().saturating_sub(1);

    for (index, root) in report.roots.iter().enumerate() {
        root.push_text(&mut writer, index == last_index);
    }

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

fn render_json_report(
    report: &SymbolInspectionReport,
) -> Result<String, SymbolInspectionRenderError> {
    let mut output =
        serde_json::to_string_pretty(report).map_err(|_| SymbolInspectionRenderError::Json)?;

    output.push('\n');

    Ok(output)
}

#[cfg(test)]
mod tests {
    use bray_compilation::Compilation;
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};
    use bray_symbols::PackageIdentity;
    use serde_json::Value;

    use super::render_symbol_inspection;
    use crate::command::DriverOutputFormat;

    const SOURCE: &str = concat!(
        "module example;\n",
        "\n",
        "struct Value<T>\n",
        "{\n",
        "    value: T;\n",
        "\n",
        "    func get() -> T\n",
        "    {\n",
        "    }\n",
        "}\n",
        "\n",
        "overload choose = {pick}\n",
    );

    #[test]
    fn text_inspection_renders_origins_and_typed_relationships() {
        let compilation = compilation([SOURCE]);

        let output = match render_symbol_inspection(&compilation, DriverOutputFormat::Text) {
            Ok(output) => output,
            Err(error) => panic!("symbol inspection should render: {error:?}"),
        };

        let (text, diagnostics) = output.into_parts();

        assert!(text.contains("├─ compiler_known"));
        assert!(text.contains("└─ source"));
        assert!(text.contains("origin:synthesized"));
        assert!(text.contains("struct Value"));
        assert!(text.contains("generic_parameters"));
        assert!(text.contains("fields"));
        assert!(text.contains("type_members"));
        assert!(text.contains("overload_arms"));
        assert!(text.contains("syntax_reference"));
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn json_inspection_preserves_partial_module_origins_and_symbol_identity() {
        let compilation = compilation([
            "module example { const first: i32 = 1; }\n",
            "module example { const second: i32 = 2; }\n",
        ]);

        let output = match render_symbol_inspection(&compilation, DriverOutputFormat::Json) {
            Ok(output) => output,
            Err(error) => panic!("symbol inspection should render: {error:?}"),
        };

        let (json, diagnostics) = output.into_parts();

        let value: Value = match serde_json::from_str(&json) {
            Ok(value) => value,
            Err(error) => panic!("symbol inspection JSON must parse: {error}"),
        };

        let module = find_symbol(&value, "module", Some("example"));

        let origins = match module.get("declaration_origins").and_then(Value::as_array) {
            Some(origins) => origins,
            None => panic!("module must expose declaration origins"),
        };

        assert_eq!(origins.len(), 2);
        assert!(module.get("id").and_then(Value::as_u64).is_some());

        assert_eq!(
            module.get("origin").and_then(Value::as_str),
            Some("source")
        );

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    fn find_symbol<'value>(
        value: &'value Value,
        kind: &str,
        name: Option<&str>,
    ) -> &'value Value {
        if value.get("symbol_kind").and_then(Value::as_str) == Some(kind)
            && value.get("name").and_then(Value::as_str) == name
        {
            return value;
        }

        match value {
            Value::Array(values) => {
                for value in values {
                    if let Some(found) = find_symbol_optional(value, kind, name) {
                        return found;
                    }
                }
            }
            Value::Object(fields) => {
                for value in fields.values() {
                    if let Some(found) = find_symbol_optional(value, kind, name) {
                        return found;
                    }
                }
            }
            _ => {}
        }

        panic!("expected symbol {kind} {name:?}");
    }

    fn find_symbol_optional<'value>(
        value: &'value Value,
        kind: &str,
        name: Option<&str>,
    ) -> Option<&'value Value> {
        if value.get("symbol_kind").and_then(Value::as_str) == Some(kind)
            && value.get("name").and_then(Value::as_str) == name
        {
            return Some(value);
        }

        match value {
            Value::Array(values) => values
                .iter()
                .find_map(|value| find_symbol_optional(value, kind, name)),
            Value::Object(fields) => fields
                .values()
                .find_map(|value| find_symbol_optional(value, kind, name)),
            _ => None,
        }
    }

    fn compilation<const N: usize>(sources: [&str; N]) -> Compilation {
        let inputs = sources
            .into_iter()
            .enumerate()
            .map(|(index, text)| {
                let identity = match u32::try_from(index) {
                    Ok(index) => SourceIdentity::new(index),
                    Err(_) => panic!("test source index must fit"),
                };

                SourceInput::virtual_text(
                    identity,
                    format!("test-{index}.bray"),
                    SourceVersion::new(0),
                    text,
                )
            })
            .collect::<Vec<_>>();

        let package = match PackageIdentity::try_new("test.package") {
            Some(package) => package,
            None => panic!("test package identity must be valid"),
        };

        match Compilation::load_sources(package, inputs) {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation must load: {error:?}"),
        }
    }
}
