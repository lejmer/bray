//! Symbol inspection report construction and rendering.

use std::collections::{BTreeMap, BTreeSet};

use bray_compilation::{Compilation, FactQueryError};
use bray_declarations::{DeclarationId, DeclarationTable, SyntaxAnchor};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{AnySymbolId, SemanticValueStore, SymbolGraph, SymbolOrigin};
use serde::Serialize;

use crate::OutputFormat;
use crate::inspection::{
    InspectionOutput, InspectionSourceError, InspectionSources, InspectionSymbolIdentity,
    InspectionSyntaxAnchor, InspectionType, TreeWriter, TypeInspectionError, push_report_value,
    push_text_diagnostic, render_pretty_json,
};
use crate::output::{DiagnosticJson, diagnostic_jsons};

use super::relationship::{
    InspectionRelationship, InspectionRelationshipKind, InspectionSymbolReference,
    relationship_kind,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SymbolInspectionRenderError {
    Declaration,
    Evaluation(FactQueryError),
    Json(String),
    Source(InspectionSourceError),
    SourceIndex,
    SourceIndexOverflow(bray_source::TextSizeOverflow),
    Symbol,
    SymbolCycle,
    Type(TypeInspectionError),
    UnsupportedRelationship,
}

impl From<InspectionSourceError> for SymbolInspectionRenderError {
    fn from(error: InspectionSourceError) -> Self {
        match error {
            InspectionSourceError::Source => Self::Source(error),
            InspectionSourceError::SourceIndex => Self::SourceIndex,
            InspectionSourceError::SourceIndexOverflow(error) => Self::SourceIndexOverflow(error),
        }
    }
}

impl From<TypeInspectionError> for SymbolInspectionRenderError {
    fn from(error: TypeInspectionError) -> Self {
        Self::Type(error)
    }
}

pub(crate) fn render_symbol_inspection(
    compilation: &Compilation,
    output_format: OutputFormat,
) -> Result<InspectionOutput, SymbolInspectionRenderError> {
    let declaration_result = compilation.declaration_table_result();

    let symbols = compilation
        .symbol_graph()
        .map_err(SymbolInspectionRenderError::Evaluation)?;

    let diagnostics = compilation
        .syntax_tree_result()
        .diagnostics()
        .merged(declaration_result.diagnostics());

    let (report, diagnostics) = SymbolInspectionReport::from_compilation(
        compilation,
        declaration_result.table(),
        symbols,
        diagnostics,
    )?;

    let stdout = match output_format {
        OutputFormat::Text => render_text_report(&report),
        OutputFormat::Json => {
            render_pretty_json(&report)
                .map_err(|error| SymbolInspectionRenderError::Json(error.to_string()))?
        }
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
        diagnostics: DiagnosticBag,
    ) -> Result<(Self, DiagnosticBag), SymbolInspectionRenderError> {
        let sources = InspectionSources::new(compilation.sources())?;

        let semantic_values = compilation
            .semantic_value_store()
            .map_err(SymbolInspectionRenderError::Evaluation)?;

        let mut context = SymbolInspectionContext::new(
            compilation,
            symbols,
            declarations,
            semantic_values,
            sources,
            diagnostics,
        );

        let roots = context.roots()?;
        let root_count = roots.iter().map(|group| group.symbols.len()).sum();
        let diagnostics = context.into_diagnostics();

        Ok((
            Self {
                kind: "symbol_inspection",
                symbol_count: symbols.symbols().count(),
                root_count,
                has_errors: diagnostics.has_errors(),
                roots,
                diagnostics: diagnostic_jsons(&diagnostics, Some(compilation.sources())),
            },
            diagnostics,
        ))
    }
}

struct SymbolInspectionContext<'model, 'source> {
    compilation: &'model Compilation,
    symbols: &'model SymbolGraph,
    declarations: &'model DeclarationTable,
    semantic_values: &'model SemanticValueStore,
    sources: InspectionSources<'source>,
    children: BTreeMap<AnySymbolId, Vec<AnySymbolId>>,
    diagnostics: DiagnosticBag,
}

impl<'model, 'source> SymbolInspectionContext<'model, 'source> {
    fn new(
        compilation: &'model Compilation,
        symbols: &'model SymbolGraph,
        declarations: &'model DeclarationTable,
        semantic_values: &'model SemanticValueStore,
        sources: InspectionSources<'source>,
        diagnostics: DiagnosticBag,
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
            compilation,
            symbols,
            declarations,
            semantic_values,
            sources,
            children,
            diagnostics,
        }
    }

    fn into_diagnostics(self) -> DiagnosticBag {
        self.diagnostics
    }

    fn roots(&mut self) -> Result<Vec<InspectionRootGroup>, SymbolInspectionRenderError> {
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

            for symbol in root_ids
                .iter()
                .copied()
                .filter(|symbol| self.symbols.symbol_origin(*symbol) == Some(origin))
            {
                group
                    .symbols
                    .push(self.symbol(symbol, &mut BTreeSet::new())?);
            }

            if !group.symbols.is_empty() {
                groups.push(group);
            }
        }

        Ok(groups)
    }

    fn symbol(
        &mut self,
        id: AnySymbolId,
        ancestors: &mut BTreeSet<AnySymbolId>,
    ) -> Result<InspectionSymbol, SymbolInspectionRenderError> {
        if !ancestors.insert(id) {
            return Err(SymbolInspectionRenderError::SymbolCycle);
        }

        let mut relationships = Vec::<InspectionRelationship>::new();
        let children = self.children.get(&id).cloned().unwrap_or_default();

        for child in children {
            let relationship_kind = relationship_kind(id.kind(), child.kind())?;
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
                InspectionRelationshipKind::OverloadArm,
                overload_references,
            ));
        }

        ancestors.remove(&id);

        let origin = self
            .symbols
            .symbol_origin(id)
            .ok_or(SymbolInspectionRenderError::Symbol)?;

        let declaration_origins = self.declaration_origins(id)?;
        let surface = self.surface(id)?;

        Ok(InspectionSymbol {
            identity: InspectionSymbolIdentity::from_symbol(self.symbols, id),
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
            surface,
            declaration_origins,
            relationships,
        })
    }

    fn surface(
        &mut self,
        id: AnySymbolId,
    ) -> Result<Option<InspectionSymbolSurface>, SymbolInspectionRenderError> {
        if let Some(signature) = self
            .compilation
            .callable_signature_template(id)
            .map_err(SymbolInspectionRenderError::Evaluation)?
        {
            self.diagnostics
                .add_range(signature.diagnostics().iter().cloned());

            let signature_type = InspectionType::from_template(
                self.semantic_values,
                self.symbols,
                signature.value().callable_type(),
            )?;

            return Ok(Some(InspectionSymbolSurface::Callable {
                signature: signature_type,
                has_body: signature.value().has_body(),
            }));
        }

        if let Some(signature) = self
            .compilation
            .predicate_signature_template(id)
            .map_err(SymbolInspectionRenderError::Evaluation)?
        {
            self.diagnostics
                .add_range(signature.diagnostics().iter().cloned());

            let parameters = signature
                .value()
                .parameters()
                .iter()
                .map(|parameter| {
                    Ok(InspectionPredicateParameter {
                        symbol: InspectionSymbolIdentity::from_symbol(
                            self.symbols,
                            parameter.parameter().into(),
                        ),
                        ty: InspectionType::from_template(
                            self.semantic_values,
                            self.symbols,
                            parameter.ty(),
                        )?,
                    })
                })
                .collect::<Result<Vec<_>, TypeInspectionError>>()?;

            return Ok(Some(InspectionSymbolSurface::Predicate {
                parameters,
                trusted: signature.value().is_trusted(),
            }));
        }

        if let Some(ty) = self
            .compilation
            .symbol_type_template(id)
            .map_err(SymbolInspectionRenderError::Evaluation)?
        {
            self.diagnostics.add_range(ty.diagnostics().iter().cloned());

            return InspectionType::from_template(self.semantic_values, self.symbols, ty.value())
                .map(InspectionSymbolSurface::DeclaredType)
                .map(Some)
                .map_err(Into::into);
        }

        Ok(None)
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
            _ => self.symbols.symbol_declaration(id).into_iter().collect(),
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

    fn modifiers(&self, id: AnySymbolId) -> Result<Vec<&'static str>, SymbolInspectionRenderError> {
        let declarations = match id {
            AnySymbolId::Module(module) => self
                .symbols
                .module(module)
                .ok_or(SymbolInspectionRenderError::Symbol)?
                .declarations()
                .to_vec(),
            _ => self.symbols.symbol_declaration(id).into_iter().collect(),
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

        references.extend(symbols.iter().copied().map(|symbol| {
            InspectionSymbolReference::Symbol(InspectionSymbolIdentity::from_symbol(
                self.symbols,
                symbol,
            ))
        }));

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
pub(super) struct InspectionSymbol {
    #[serde(flatten)]
    identity: InspectionSymbolIdentity,
    origin: &'static str,
    visibility: Option<&'static str>,
    modifiers: Vec<&'static str>,
    recovered: bool,
    surface: Option<InspectionSymbolSurface>,
    declaration_origins: Vec<InspectionSyntaxAnchor>,
    relationships: Vec<InspectionRelationship>,
}

impl InspectionSymbol {
    pub(super) fn push_text(&self, writer: &mut TreeWriter, is_last: bool) {
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

        let surface = self
            .surface
            .as_ref()
            .map(InspectionSymbolSurface::text)
            .map(|surface| format!(" {surface}"))
            .unwrap_or_default();

        format!(
            "{}{surface} [origin:{}{visibility}{modifiers}{recovered}]",
            self.identity.text(),
            self.origin
        )
    }
}

#[derive(Serialize)]
#[serde(tag = "surface_kind", rename_all = "snake_case")]
enum InspectionSymbolSurface {
    Callable {
        signature: InspectionType,
        has_body: bool,
    },
    Predicate {
        parameters: Vec<InspectionPredicateParameter>,
        trusted: bool,
    },
    DeclaredType(InspectionType),
}

impl InspectionSymbolSurface {
    fn text(&self) -> String {
        match self {
            Self::Callable { signature, .. } => signature.text().to_owned(),
            Self::Predicate {
                parameters,
                trusted,
            } => {
                let parameters = parameters
                    .iter()
                    .map(InspectionPredicateParameter::text)
                    .collect::<Vec<_>>()
                    .join(", ");

                let trusted = if *trusted { "trusted " } else { "" };

                format!("{trusted}predicate({parameters})")
            }
            Self::DeclaredType(ty) => format!(": {}", ty.text()),
        }
    }
}

#[derive(Serialize)]
struct InspectionPredicateParameter {
    symbol: InspectionSymbolIdentity,
    ty: InspectionType,
}

impl InspectionPredicateParameter {
    fn text(&self) -> String {
        format!("{}: {}", self.symbol.display_name(), self.ty.text())
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

fn render_text_report(report: &SymbolInspectionReport) -> String {
    let mut output = String::new();

    push_report_value(&mut output, "kind", report.kind);
    push_report_value(&mut output, "symbol_count", report.symbol_count);
    push_report_value(&mut output, "root_count", report.root_count);
    push_report_value(&mut output, "has_errors", report.has_errors);
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

#[cfg(test)]
mod tests {
    use bray_compilation::Compilation;
    use bray_testing::test_source_inputs;
    use serde_json::Value;

    use super::render_symbol_inspection;
    use crate::OutputFormat;
    use crate::test_support::package_identity;

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

        let output = match render_symbol_inspection(&compilation, OutputFormat::Text) {
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
        assert!(text.contains("func() -> T"));
        assert!(text.contains(": T"));
        assert!(text.contains("overload_arms"));
        assert!(text.contains("syntax_reference"));
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn json_inspection_exposes_typed_declaration_surfaces() {
        let compilation = compilation([SOURCE]);

        let output = match render_symbol_inspection(&compilation, OutputFormat::Json) {
            Ok(output) => output,
            Err(error) => panic!("symbol inspection should render: {error:?}"),
        };

        let (json, diagnostics) = output.into_parts();

        let value: Value = match serde_json::from_str(&json) {
            Ok(value) => value,
            Err(error) => panic!("symbol inspection JSON must parse: {error}"),
        };

        let field = find_symbol(&value, "struct_field", Some("value"));
        let field_surface = field.get("surface").unwrap_or(&Value::Null);

        assert_eq!(
            field_surface.get("surface_kind").and_then(Value::as_str),
            Some("declared_type")
        );

        assert_eq!(field_surface.get("text").and_then(Value::as_str), Some("T"));

        let callable = find_symbol(&value, "type_callable_member", Some("get"));
        let callable_surface = callable.get("surface").unwrap_or(&Value::Null);

        assert_eq!(
            callable_surface.get("surface_kind").and_then(Value::as_str),
            Some("callable")
        );

        assert_eq!(
            callable_surface
                .get("signature")
                .and_then(|signature| signature.get("text"))
                .and_then(Value::as_str),
            Some("func() -> T")
        );

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn json_inspection_preserves_partial_module_origins_and_symbol_identity() {
        let compilation = compilation([
            "module example { const first: i32 = 1; }\n",
            "module example { const second: i32 = 2; }\n",
        ]);

        let output = match render_symbol_inspection(&compilation, OutputFormat::Json) {
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

        assert_eq!(module.get("origin").and_then(Value::as_str), Some("source"));

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    fn find_symbol<'value>(value: &'value Value, kind: &str, name: Option<&str>) -> &'value Value {
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
        let inputs = test_source_inputs("test", sources);
        let package = package_identity();

        match Compilation::load_sources(package, inputs) {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation must load: {error:?}"),
        }
    }
}
