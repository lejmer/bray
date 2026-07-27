//! Bound inspection report construction and rendering.

use bray_bound_tree::{
    AnyBoundNodeId, BoundCallableBodyKind, BoundUnit, BoundWalkControl, BoundWalkEvent,
    BoundWalkOutcome, CheckedExpressionTypes, CheckedSemanticSelections, StoragePlan,
    walk_bound_unit_view,
};
use bray_compilation::{CancellationToken, Compilation, QueryPriority};
use bray_diagnostics::DiagnosticBag;
use bray_source::SourceId;
use bray_symbols::{SemanticValueStore, SymbolGraph};
use serde::Serialize;

use crate::command::{BoundInspectionTarget, DriverOutputFormat};
use crate::diagnostic_output::{DiagnosticJson, diagnostic_jsons};
use crate::inspection::{
    InspectionOutput, InspectionSourceError, InspectionSources, InspectionSymbolIdentity,
    InspectionSyntaxAnchor, TreeWriter, push_text_diagnostic,
};
use crate::type_inspection::{InspectionType, TypeInspectionError};

use super::locals::InspectionLocals;
use super::selection::{
    InspectionSelectionEntry, SelectionInspectionError, selection_entries, selection_kind,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BoundInspectionRenderError {
    BoundFact,
    Json,
    MissingNode,
    Source,
    SourceIndex,
    StorageFact,
    Symbol,
    SymbolFact,
    Type,
    TypeFact,
    SelectionFact,
    Selection,
}

impl From<InspectionSourceError> for BoundInspectionRenderError {
    fn from(error: InspectionSourceError) -> Self {
        match error {
            InspectionSourceError::Source => Self::Source,
            InspectionSourceError::SourceIndex => Self::SourceIndex,
        }
    }
}

impl From<TypeInspectionError> for BoundInspectionRenderError {
    fn from(_: TypeInspectionError) -> Self {
        Self::Type
    }
}

impl From<SelectionInspectionError> for BoundInspectionRenderError {
    fn from(_: SelectionInspectionError) -> Self {
        Self::Selection
    }
}

pub(crate) fn render_bound_inspection(
    compilation: &Compilation,
    target: BoundInspectionTarget,
    output_format: DriverOutputFormat,
) -> Result<InspectionOutput, BoundInspectionRenderError> {
    let source_id =
        SourceId::stored(target.source_id()).ok_or(BoundInspectionRenderError::Source)?;

    let cancellation = CancellationToken::new();

    let source_diagnostics = compilation
        .diagnostics_for_source(source_id, &cancellation, QueryPriority::Interactive)
        .map_err(|_| BoundInspectionRenderError::Source)?;

    let bound = compilation
        .bound_unit_at(
            source_id,
            target.position(),
            &cancellation,
            QueryPriority::Interactive,
        )
        .map_err(|_| BoundInspectionRenderError::BoundFact)?;

    let Some(bound) = bound else {
        let report = BoundInspectionReport::empty(
            target,
            &source_diagnostics,
            compilation,
        );

        return render_report(report, source_diagnostics, output_format);
    };

    // Each demanded fact retains the same small Arc-backed unit identity.
    let key = bound.value().key().clone();

    let expression_types = compilation
        .expression_types(key.clone())
        .map_err(|_| BoundInspectionRenderError::TypeFact)?;

    let selections = compilation
        .semantic_selections(key.clone())
        .map_err(|_| BoundInspectionRenderError::SelectionFact)?;

    let storage = compilation
        .storage_plan(key)
        .map_err(|_| BoundInspectionRenderError::StorageFact)?;

    let diagnostics = DiagnosticBag::merged_all([
        &source_diagnostics,
        bound.diagnostics(),
        expression_types.diagnostics(),
        selections.diagnostics(),
        storage.diagnostics(),
    ]);

    let symbols = compilation
        .symbol_graph()
        .map_err(|_| BoundInspectionRenderError::SymbolFact)?;

    let semantic_values = compilation
        .semantic_value_store()
        .map_err(|_| BoundInspectionRenderError::SymbolFact)?;

    let sources = InspectionSources::new(compilation.sources())?;

    let report = BoundInspectionReport::from_unit(
        target,
        bound.value(),
        expression_types.value(),
        selections.value(),
        storage.value(),
        symbols,
        semantic_values,
        &sources,
        &diagnostics,
        compilation,
    )?;

    render_report(report, diagnostics, output_format)
}

fn render_report(
    report: BoundInspectionReport,
    diagnostics: DiagnosticBag,
    output_format: DriverOutputFormat,
) -> Result<InspectionOutput, BoundInspectionRenderError> {
    let stdout = match output_format {
        DriverOutputFormat::Text => render_text_report(&report),
        DriverOutputFormat::Json => serde_json::to_string_pretty(&report)
            .map(|json| format!("{json}\n"))
            .map_err(|_| BoundInspectionRenderError::Json)?,
    };

    Ok(InspectionOutput::new(stdout, diagnostics))
}

#[derive(Serialize)]
struct BoundInspectionReport {
    kind: &'static str,
    source_id: u32,
    offset: u32,
    selected_unit: Option<InspectionBoundUnit>,
    has_errors: bool,
    diagnostics: Vec<DiagnosticJson>,
}

impl BoundInspectionReport {
    fn empty(
        target: BoundInspectionTarget,
        diagnostics: &DiagnosticBag,
        compilation: &Compilation,
    ) -> Self {
        Self {
            kind: "bound_inspection",
            source_id: target.source_id(),
            offset: u32::from(target.position()),
            selected_unit: None,
            has_errors: diagnostics.has_errors(),
            diagnostics: diagnostic_jsons(diagnostics, Some(compilation.sources())),
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the report joins the independently demanded facts for one selected bound unit"
    )]
    fn from_unit(
        target: BoundInspectionTarget,
        unit: &BoundUnit,
        types: &CheckedExpressionTypes,
        selections: &CheckedSemanticSelections,
        storage: &StoragePlan,
        symbols: &SymbolGraph,
        semantic_values: &SemanticValueStore,
        sources: &InspectionSources<'_>,
        diagnostics: &DiagnosticBag,
        compilation: &Compilation,
    ) -> Result<Self, BoundInspectionRenderError> {
        let selected_unit = InspectionBoundUnit::new(
            unit,
            types,
            selections,
            storage,
            symbols,
            semantic_values,
            sources,
        )?;

        Ok(Self {
            kind: "bound_inspection",
            source_id: target.source_id(),
            offset: u32::from(target.position()),
            selected_unit: Some(selected_unit),
            has_errors: diagnostics.has_errors(),
            diagnostics: diagnostic_jsons(diagnostics, Some(compilation.sources())),
        })
    }
}

#[derive(Serialize)]
struct InspectionBoundUnit {
    unit_kind: &'static str,
    unit_id: u32,
    owner: InspectionSymbolIdentity,
    source: InspectionSyntaxAnchor,
    root: InspectionBoundNode,
    locals: InspectionLocals,
    storage: InspectionStorage,
    selections: Vec<InspectionSelectionEntry>,
    nested_units: Vec<InspectionNestedUnit>,
}

impl InspectionBoundUnit {
    fn new(
        unit: &BoundUnit,
        types: &CheckedExpressionTypes,
        selections: &CheckedSemanticSelections,
        storage: &StoragePlan,
        symbols: &SymbolGraph,
        semantic_values: &SemanticValueStore,
        sources: &InspectionSources<'_>,
    ) -> Result<Self, BoundInspectionRenderError> {
        let source = InspectionSyntaxAnchor::from_anchor(
            sources,
            unit.key().source().syntax(),
        )?;

        let owner = unit_owner(unit.key(), symbols)?;

        let root = build_tree(
            unit,
            types,
            selections,
            storage,
            symbols,
            semantic_values,
            sources,
        )?;

        let nested_units = unit
            .nested_units()
            .iter()
            .map(|key| {
                Ok(InspectionNestedUnit {
                    unit_kind: key.kind().as_str(),
                    owner: unit_owner(key, symbols)?,
                    source: InspectionSyntaxAnchor::from_anchor(
                        sources,
                        key.source().syntax(),
                    )?,
                })
            })
            .collect::<Result<_, BoundInspectionRenderError>>()?;

        Ok(Self {
            unit_kind: unit.key().kind().as_str(),
            unit_id: unit.unit().raw(),
            owner,
            source,
            root,
            locals: InspectionLocals::from_snapshot(unit.local_symbols()),
            storage: InspectionStorage::from_plan(storage),
            selections: selection_entries(
                selections,
                unit.local_symbols(),
                symbols,
                semantic_values,
            )?,
            nested_units,
        })
    }
}

#[derive(Serialize)]
struct InspectionNestedUnit {
    unit_kind: &'static str,
    owner: InspectionSymbolIdentity,
    source: InspectionSyntaxAnchor,
}

fn unit_owner(
    key: &bray_bound_tree::BoundUnitKey,
    symbols: &SymbolGraph,
) -> Result<InspectionSymbolIdentity, BoundInspectionRenderError> {
    let owner = symbols
        .symbol_for_key(key.declared_owner())
        .ok_or(BoundInspectionRenderError::Symbol)?;

    Ok(InspectionSymbolIdentity::from_symbol(symbols, owner))
}

#[derive(Serialize)]
struct InspectionBoundNode {
    node_kind: &'static str,
    semantic_kind: &'static str,
    id: u32,
    source: InspectionSyntaxAnchor,
    synthesis: Option<InspectionSynthesis>,
    recovered: bool,
    checked_type: Option<InspectionCheckedType>,
    selection: Option<&'static str>,
    storage_accesses: Vec<InspectionStoragePlan>,
    children: Vec<InspectionBoundNode>,
}

#[derive(Serialize)]
struct InspectionSynthesis {
    role: &'static str,
    ordinal: u32,
}

#[derive(Serialize)]
struct InspectionCheckedType {
    status: &'static str,
    r#type: InspectionType,
}

#[derive(Serialize)]
struct InspectionStorage {
    identities: Vec<InspectionStorageIdentity>,
    access_count: usize,
    alternative_count: usize,
    borrow_capability_count: usize,
    binding_count: usize,
    plans: Vec<InspectionStoragePlan>,
}

impl InspectionStorage {
    fn from_plan(plan: &StoragePlan) -> Self {
        Self {
            identities: plan
                .identity_entries()
                .map(|(id, identity)| InspectionStorageIdentity {
                    id: id.ordinal(),
                    storage_kind: identity.kind_name(),
                })
                .collect(),
            access_count: plan.accesses().len(),
            alternative_count: plan.alternatives().len(),
            borrow_capability_count: plan.borrow_capabilities().len(),
            binding_count: plan.bindings().len(),
            plans: plan
                .access_plans()
                .iter()
                .copied()
                .map(InspectionStoragePlan::from)
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct InspectionStorageIdentity {
    id: u32,
    storage_kind: &'static str,
}

#[derive(Clone, Serialize)]
struct InspectionStoragePlan {
    expression: u32,
    purpose: &'static str,
    access: u32,
}

impl From<bray_bound_tree::StorageAccessPlan> for InspectionStoragePlan {
    fn from(plan: bray_bound_tree::StorageAccessPlan) -> Self {
        Self {
            expression: plan.expression().ordinal(),
            purpose: plan.purpose().as_str(),
            access: plan.access().ordinal(),
        }
    }
}

fn build_tree(
    unit: &BoundUnit,
    types: &CheckedExpressionTypes,
    selections: &CheckedSemanticSelections,
    storage: &StoragePlan,
    symbols: &SymbolGraph,
    semantic_values: &SemanticValueStore,
    sources: &InspectionSources<'_>,
) -> Result<InspectionBoundNode, BoundInspectionRenderError> {
    let mut stack = Vec::<InspectionBoundNode>::new();
    let mut root = None;
    let mut error = None;

    let outcome = walk_bound_unit_view(unit.view(), unit.root(), |event| {
        match event {
            BoundWalkEvent::Enter(id) => {
                match inspection_node(
                    unit,
                    id,
                    types,
                    selections,
                    storage,
                    symbols,
                    semantic_values,
                    sources,
                ) {
                    Ok(node) => stack.push(node),
                    Err(node_error) => {
                        error = Some(node_error);

                        return BoundWalkControl::Stop;
                    }
                }
            }
            BoundWalkEvent::Exit(_) => {
                let Some(node) = stack.pop() else {
                    error = Some(BoundInspectionRenderError::MissingNode);

                    return BoundWalkControl::Stop;
                };

                match stack.last_mut() {
                    Some(parent) => parent.children.push(node),
                    None if root.is_none() => root = Some(node),
                    None => {
                        error = Some(BoundInspectionRenderError::MissingNode);

                        return BoundWalkControl::Stop;
                    }
                }
            }
        }

        BoundWalkControl::Continue
    });

    if let Some(error) = error {
        return Err(error);
    }

    if outcome != BoundWalkOutcome::Completed || !stack.is_empty() {
        return Err(BoundInspectionRenderError::MissingNode);
    }

    root.ok_or(BoundInspectionRenderError::MissingNode)
}

#[expect(
    clippy::too_many_arguments,
    reason = "node inspection correlates one node with every explicitly requested unit fact"
)]
fn inspection_node(
    unit: &BoundUnit,
    id: AnyBoundNodeId,
    types: &CheckedExpressionTypes,
    selections: &CheckedSemanticSelections,
    storage: &StoragePlan,
    symbols: &SymbolGraph,
    semantic_values: &SemanticValueStore,
    sources: &InspectionSources<'_>,
) -> Result<InspectionBoundNode, BoundInspectionRenderError> {
    let (semantic_kind, origin, recovered, checked_type, selection) = match id {
        AnyBoundNodeId::Expression(expression_id) => {
            let expression = unit
                .view()
                .expression(expression_id)
                .ok_or(BoundInspectionRenderError::MissingNode)?;

            let checked_type = match types.expression(expression_id) {
                Some(result) => Some(InspectionCheckedType {
                    status: result.status().as_str(),
                    r#type: InspectionType::from_type(
                        semantic_values,
                        symbols,
                        result.ty(),
                    )?,
                }),
                None => None,
            };

            (
                expression.kind_name(),
                expression.origin(),
                expression.is_recovered(),
                checked_type,
                selections.expression(expression_id).map(selection_kind),
            )
        }
        AnyBoundNodeId::Pattern(pattern_id) => {
            let pattern = unit
                .view()
                .pattern(pattern_id)
                .ok_or(BoundInspectionRenderError::MissingNode)?;

            (
                pattern.kind().as_str(),
                pattern.origin(),
                pattern.is_recovered(),
                Some(InspectionCheckedType {
                    status: if pattern.is_recovered() {
                        "recovered"
                    } else {
                        "valid"
                    },
                    r#type: InspectionType::from_type(
                        semantic_values,
                        symbols,
                        pattern.input_type(),
                    )?,
                }),
                None,
            )
        }
        AnyBoundNodeId::Block(block_id) => {
            let block = unit
                .view()
                .block(block_id)
                .ok_or(BoundInspectionRenderError::MissingNode)?;

            (
                "block",
                block.origin(),
                block.is_recovered(),
                None,
                None,
            )
        }
        AnyBoundNodeId::CallableBody(body_id) => {
            let body = unit
                .view()
                .callable_body(body_id)
                .ok_or(BoundInspectionRenderError::MissingNode)?;

            let (semantic_kind, recovered) = match body.kind() {
                BoundCallableBodyKind::Block(_) => ("block_body", false),
                BoundCallableBodyKind::Error(_) => ("error_body", true),
            };

            (semantic_kind, body.origin(), recovered, None, None)
        }
    };

    let source = InspectionSyntaxAnchor::from_anchor(sources, origin.source_anchor().syntax())?;

    let synthesis = origin.synthesized_origin().map(|origin| InspectionSynthesis {
        role: origin.role().as_str(),
        ordinal: origin.ordinal().raw(),
    });

    let storage_accesses = match id {
        AnyBoundNodeId::Expression(expression) => storage
            .expression_plans(expression)
            .map(InspectionStoragePlan::from)
            .collect(),
        AnyBoundNodeId::Pattern(_)
        | AnyBoundNodeId::Block(_)
        | AnyBoundNodeId::CallableBody(_) => Vec::new(),
    };

    Ok(InspectionBoundNode {
        node_kind: id.kind().as_str(),
        semantic_kind,
        id: id.ordinal(),
        source,
        synthesis,
        recovered,
        checked_type,
        selection,
        storage_accesses,
        children: Vec::new(),
    })
}

fn render_text_report(report: &BoundInspectionReport) -> String {
    let mut output = format!(
        "Bound tree at source {} offset {}\n",
        report.source_id, report.offset
    );

    match &report.selected_unit {
        Some(unit) => {
            output.push_str(&format!(
                "Unit {} [unit:{}] {}\n",
                unit.unit_kind,
                unit.unit_id,
                unit.source.location_text()
            ));

            output.push_str(&format!("Owner: {}\n", unit.owner.text()));

            let mut writer = TreeWriter::new("  ");

            push_text_node(&mut writer, &unit.root, true);

            output.push_str(&writer.into_string());

            output.push_str(&format!(
                "\nLocals: {} symbols, {} scopes\n",
                unit.locals.symbols.len(),
                unit.locals.scope_count
            ));

            for symbol in &unit.locals.symbols {
                let name = symbol
                    .name
                    .as_ref()
                    .map(|name| format!(" {name}"))
                    .unwrap_or_default();

                let recovered = if symbol.recovered { " [recovered]" } else { "" };

                output.push_str(&format!(
                    "  {}{name} [local:{}]{recovered}\n",
                    symbol.symbol_kind, symbol.id
                ));
            }

            output.push_str(&format!(
                "\nStorage: {} identities, {} accesses, {} plans\n",
                unit.storage.identities.len(),
                unit.storage.access_count,
                unit.storage.plans.len()
            ));

            for plan in &unit.storage.plans {
                output.push_str(&format!(
                    "  expression:{} {} -> access:{}\n",
                    plan.expression, plan.purpose, plan.access
                ));
            }

            output.push_str(&format!("\nSelections: {}\n", unit.selections.len()));

            for selection in &unit.selections {
                let target = selection
                    .target_text()
                    .map(|target| format!(" -> {target}"))
                    .unwrap_or_default();

                output.push_str(&format!(
                    "  expression:{} {}{target}\n",
                    selection.expression(),
                    selection.selection_kind()
                ));
            }

            output.push_str(&format!("\nNested units: {}\n", unit.nested_units.len()));

            for nested in &unit.nested_units {
                output.push_str(&format!(
                    "  {} {} {}\n",
                    nested.unit_kind,
                    nested.owner.text(),
                    nested.source.location_text()
                ));
            }
        }
        None => output.push_str("No independently checked bound unit covers this position.\n"),
    }

    if !report.diagnostics.is_empty() {
        output.push_str("\nDiagnostics:\n");

        for diagnostic in &report.diagnostics {
            push_text_diagnostic(&mut output, diagnostic);
        }
    }

    output
}

fn push_text_node(writer: &mut TreeWriter, node: &InspectionBoundNode, is_last: bool) {
    let checked_type = node
        .checked_type
        .as_ref()
        .map(|checked| format!(" type={}", checked.r#type.text()))
        .unwrap_or_default();

    let selection = node
        .selection
        .map(|selection| format!(" selection={selection}"))
        .unwrap_or_default();

    let recovered = if node.recovered { " [recovered]" } else { "" };

    let synthesized = node
        .synthesis
        .as_ref()
        .map(|synthesis| format!(" synthesized={}:{}", synthesis.role, synthesis.ordinal))
        .unwrap_or_default();

    writer.push_line(
        is_last,
        &format!(
            "{} {} [node:{}] {}{checked_type}{selection}{synthesized}{recovered}",
            node.node_kind,
            node.semantic_kind,
            node.id,
            node.source.location_text()
        ),
    );

    writer.enter_children(is_last);

    let child_count = node.children.len();

    for (index, child) in node.children.iter().enumerate() {
        push_text_node(writer, child, index + 1 == child_count);
    }

    writer.leave_children();
}

#[cfg(test)]
mod tests {
    use bray_compilation::Compilation;
    use bray_source::{SourceIdentity, SourceInput, SourceVersion, TextSize};
    use bray_symbols::PackageIdentity;
    use serde_json::Value;

    use super::render_bound_inspection;
    use crate::command::{BoundInspectionTarget, DriverOutputFormat};

    const SOURCE: &str = concat!(
        "module example;\n",
        "\n",
        "func main(value: i32) -> i32\n",
        "{\n",
        "    let result: i32 = value;\n",
        "\n",
        "    result;\n",
        "}\n",
    );

    #[test]
    fn text_inspection_renders_one_selected_unit_with_tree_guides() {
        let compilation = compilation(SOURCE);
        let target = target_at(SOURCE, "let result");

        let output =
            match render_bound_inspection(&compilation, target, DriverOutputFormat::Text) {
                Ok(output) => output,
                Err(error) => panic!("bound inspection should render: {error:?}"),
            };

        let (text, diagnostics) = output.into_parts();

        assert!(text.contains("Unit callable_body"));
        assert!(text.contains("└─ "));
        assert!(text.contains("block_body"));
        assert!(text.contains("├─ "));
        assert!(text.contains("type=i32"));
        assert!(text.contains("Locals:"));
        assert!(text.contains("Storage:"));
        assert!(text.contains("Selections:"));
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn json_inspection_exposes_typed_nodes_and_side_tables() {
        let compilation = compilation(SOURCE);
        let target = target_at(SOURCE, "let result");

        let output =
            match render_bound_inspection(&compilation, target, DriverOutputFormat::Json) {
                Ok(output) => output,
                Err(error) => panic!("bound inspection should render: {error:?}"),
            };

        let (json, diagnostics) = output.into_parts();

        let value: Value = match serde_json::from_str(&json) {
            Ok(value) => value,
            Err(error) => panic!("bound inspection JSON must parse: {error}"),
        };

        let unit = value
            .get("selected_unit")
            .unwrap_or_else(|| panic!("selected unit must be present"));

        let root = unit
            .get("root")
            .unwrap_or_else(|| panic!("bound root must be present"));

        assert_eq!(
            unit.get("unit_kind").and_then(Value::as_str),
            Some("callable_body")
        );

        assert_eq!(
            root.get("node_kind").and_then(Value::as_str),
            Some("callable_body")
        );

        assert!(unit.get("locals").is_some());
        assert!(unit.get("storage").is_some());
        assert!(unit.get("selections").is_some());
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn inspection_outside_a_semantic_unit_is_explicit() {
        let compilation = compilation(SOURCE);
        let target = BoundInspectionTarget::new(0, TextSize::ZERO);

        let output =
            match render_bound_inspection(&compilation, target, DriverOutputFormat::Json) {
                Ok(output) => output,
                Err(error) => panic!("empty bound inspection should render: {error:?}"),
            };

        let (json, diagnostics) = output.into_parts();

        let value: Value = match serde_json::from_str(&json) {
            Ok(value) => value,
            Err(error) => panic!("bound inspection JSON must parse: {error}"),
        };

        assert!(value.get("selected_unit").is_some_and(Value::is_null));
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn recovered_bound_units_remain_inspectable() {
        let source = concat!(
            "module example;\n",
            "\n",
            "func main()\n",
            "{\n",
            "    let broken: i32 = ;\n",
            "}\n",
        );

        let compilation = compilation(source);
        let target = target_at(source, "let broken");

        let output =
            match render_bound_inspection(&compilation, target, DriverOutputFormat::Json) {
                Ok(output) => output,
                Err(error) => panic!("recovered bound inspection should render: {error:?}"),
            };

        let (json, diagnostics) = output.into_parts();

        let value: Value = match serde_json::from_str(&json) {
            Ok(value) => value,
            Err(error) => panic!("bound inspection JSON must parse: {error}"),
        };

        assert!(value.get("selected_unit").is_some_and(Value::is_object));
        assert!(json.contains("\"recovered\": true"));
        assert!(diagnostics.has_errors());
    }

    fn target_at(source: &str, needle: &str) -> BoundInspectionTarget {
        let offset = source
            .find(needle)
            .unwrap_or_else(|| panic!("test source must contain {needle}"));

        let position = match TextSize::try_from(offset) {
            Ok(position) => position,
            Err(_) => panic!("test source offset must fit"),
        };

        BoundInspectionTarget::new(0, position)
    }

    fn compilation(source: &str) -> Compilation {
        let input = SourceInput::virtual_text(
            SourceIdentity::new(0),
            "test.bray",
            SourceVersion::new(0),
            source,
        );

        let package = match PackageIdentity::try_new("test.package") {
            Some(package) => package,
            None => panic!("test package identity must be valid"),
        };

        match Compilation::load_sources(package, vec![input]) {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation must load: {error:?}"),
        }
    }
}
