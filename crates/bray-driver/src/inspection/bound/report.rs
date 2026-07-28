//! Bound inspection report construction and rendering.

use bray_bound_tree::{
    AnyBoundNodeId, BoundCallableBodyKind, BoundUnit, BoundWalkControl, BoundWalkEvent,
    BoundWalkOutcome, CheckedExpressionTypes, CheckedSemanticSelections, StoragePlan,
    walk_bound_unit_view,
};
use bray_compilation::Compilation;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{SemanticValueStore, SymbolGraph};
use serde::Serialize;

use crate::command::{DriverOutputFormat, UnitInspectionTarget};
use crate::inspection::{
    InspectionOutput, InspectionSourceError, InspectionSources, InspectionSymbolIdentity,
    InspectionSyntaxAnchor, InspectionType, TreeWriter, TypeInspectionError,
    push_text_diagnostic,
};
use crate::inspection::unit::{UnitInspectionSelectionError, select_units};
use crate::output::{DiagnosticJson, diagnostic_jsons};

use super::locals::InspectionLocals;
use super::selection::{
    InspectionSelectionEntry, SelectionInspectionError, selection_entries, selection_kind,
};
use super::storage::{InspectionStorage, InspectionStoragePlan, StorageInspectionError};

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

impl From<StorageInspectionError> for BoundInspectionRenderError {
    fn from(error: StorageInspectionError) -> Self {
        match error {
            StorageInspectionError::Source => Self::Source,
            StorageInspectionError::Type => Self::Type,
        }
    }
}

impl From<UnitInspectionSelectionError> for BoundInspectionRenderError {
    fn from(error: UnitInspectionSelectionError) -> Self {
        match error {
            UnitInspectionSelectionError::BoundFact => Self::BoundFact,
            UnitInspectionSelectionError::Source => Self::Source,
        }
    }
}

pub(crate) fn render_bound_inspection(
    compilation: &Compilation,
    target: UnitInspectionTarget,
    output_format: DriverOutputFormat,
) -> Result<InspectionOutput, BoundInspectionRenderError> {
    let selection = select_units(compilation, target)?;

    let (source_diagnostics, bounds) = selection.into_parts();

    if bounds.is_empty() {
        let report =
            BoundInspectionReport::new(target, Vec::new(), &source_diagnostics, compilation);

        return render_report(report, source_diagnostics, output_format);
    }

    let symbols = compilation
        .symbol_graph()
        .map_err(|_| BoundInspectionRenderError::SymbolFact)?;

    let semantic_values = compilation
        .semantic_value_store()
        .map_err(|_| BoundInspectionRenderError::SymbolFact)?;

    let sources = InspectionSources::new(compilation.sources())?;

    let mut diagnostics = source_diagnostics;
    let mut units = Vec::with_capacity(bounds.len());

    for bound in bounds {
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

        diagnostics = DiagnosticBag::merged_all([
            &diagnostics,
            bound.diagnostics(),
            expression_types.diagnostics(),
            selections.diagnostics(),
            storage.diagnostics(),
        ]);

        units.push(InspectionBoundUnit::new(
            bound.value(),
            expression_types.value(),
            selections.value(),
            storage.value(),
            symbols,
            semantic_values,
            &sources,
        )?);
    }

    let report = BoundInspectionReport::new(target, units, &diagnostics, compilation);

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
    offset: Option<u32>,
    units: Vec<InspectionBoundUnit>,
    has_errors: bool,
    diagnostics: Vec<DiagnosticJson>,
}

impl BoundInspectionReport {
    fn new(
        target: UnitInspectionTarget,
        units: Vec<InspectionBoundUnit>,
        diagnostics: &DiagnosticBag,
        compilation: &Compilation,
    ) -> Self {
        Self {
            kind: "bound_inspection",
            source_id: target.source_id(),
            offset: target.position().map(u32::from),
            units,
            has_errors: diagnostics.has_errors(),
            diagnostics: diagnostic_jsons(diagnostics, Some(compilation.sources())),
        }
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
        let source = InspectionSyntaxAnchor::from_anchor(sources, unit.key().source().syntax())?;

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
                    source: InspectionSyntaxAnchor::from_anchor(sources, key.source().syntax())?,
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
            storage: InspectionStorage::from_plan(storage, symbols, semantic_values, sources)?,
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
                    r#type: InspectionType::from_type(semantic_values, symbols, result.ty())?,
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

            ("block", block.origin(), block.is_recovered(), None, None)
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

    let synthesis = origin
        .synthesized_origin()
        .map(|origin| InspectionSynthesis {
            role: origin.role().as_str(),
            ordinal: origin.ordinal().raw(),
        });

    let storage_accesses = match id {
        AnyBoundNodeId::Expression(expression) => storage
            .expression_plans(expression)
            .map(InspectionStoragePlan::from)
            .collect(),
        AnyBoundNodeId::Pattern(_) | AnyBoundNodeId::Block(_) | AnyBoundNodeId::CallableBody(_) => {
            Vec::new()
        }
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
    let mut output = match report.offset {
        Some(offset) => format!(
            "Bound tree at source {} offset {offset}\n",
            report.source_id
        ),
        None => format!("Bound trees for source {}\n", report.source_id),
    };

    if report.units.is_empty() {
        let message = if report.offset.is_some() {
            "No independently checked bound unit covers this position.\n"
        } else {
            "No independently checked bound units originate in this source.\n"
        };

        output.push_str(message);
    }

    for (index, unit) in report.units.iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }

        push_text_unit(&mut output, unit);
    }

    if !report.diagnostics.is_empty() {
        output.push_str("\nDiagnostics:\n");

        for diagnostic in &report.diagnostics {
            push_text_diagnostic(&mut output, diagnostic);
        }
    }

    output
}

fn push_text_unit(output: &mut String, unit: &InspectionBoundUnit) {
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
        unit.storage.identity_count(),
        unit.storage.access_count(),
        unit.storage.plans().len()
    ));

    for access in unit.storage.accesses() {
        output.push_str(&format!("  {}\n", access.text()));
    }

    for plan in unit.storage.plans() {
        output.push_str(&format!(
            "  expression:{} {} -> access:{}\n",
            plan.expression(),
            plan.purpose(),
            plan.access()
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
    use bray_diagnostics::DiagnosticBag;
    use bray_source::{SourceIdentity, SourceInput, SourceVersion, TextSize};
    use bray_symbols::PackageIdentity;
    use serde_json::Value;

    use super::render_bound_inspection;
    use crate::command::{DriverOutputFormat, UnitInspectionTarget};

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

        let output = match render_bound_inspection(&compilation, target, DriverOutputFormat::Text) {
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

        let (value, diagnostics) = json_inspection(&compilation, target);

        let unit = value
            .get("units")
            .and_then(Value::as_array)
            .and_then(|units| units.first())
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
        assert!(unit.get("selections").is_some());

        let storage = unit
            .get("storage")
            .unwrap_or_else(|| panic!("storage must be present"));

        let access_ids = storage["accesses"]
            .as_array()
            .unwrap_or_else(|| panic!("storage accesses must be an array"))
            .iter()
            .filter_map(|access| access["id"].as_u64())
            .collect::<Vec<_>>();

        for plan in storage["plans"]
            .as_array()
            .unwrap_or_else(|| panic!("storage plans must be an array"))
        {
            let access = plan["access"]
                .as_u64()
                .unwrap_or_else(|| panic!("storage plan access must be an integer"));

            assert!(access_ids.contains(&access), "dangling access:{access}");
        }

        assert!(
            storage["accesses"]
                .as_array()
                .is_some_and(|accesses| accesses.iter().all(|access| {
                    access.get("root").is_some()
                        && access.get("reached_type").is_some()
                        && access.get("source").is_some()
                }))
        );

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn index_and_conversion_selections_expose_the_selected_operation_targets() {
        let source = concat!(
            "module example;\n",
            "\n",
            "func main(pos values: [i32; 2])\n",
            "{\n",
            "    let widened = values[0] as i64;\n",
            "}\n",
        );

        let compilation = compilation(source);
        let target = target_at(source, "let widened");

        let (value, diagnostics) = json_inspection(&compilation, target);

        let selections = value["units"][0]["selections"]
            .as_array()
            .unwrap_or_else(|| panic!("selections must be an array"));

        assert!(selections.iter().any(|selection| {
            selection["target"]["target_kind"].as_str() == Some("built_in_index")
                && selection["target"]["operation"].as_str() == Some("array_element")
        }));

        assert!(selections.iter().any(|selection| {
            selection["target"]["target_kind"].as_str() == Some("conversion")
                && selection["target"]["conversion"]["rule"]["rule_kind"].as_str()
                    == Some("built_in_scalar")
        }));

        assert!(
            value["units"][0]["storage"]["accesses"]
                .as_array()
                .is_some_and(|accesses| accesses.iter().any(|access| {
                    access["projections"]
                        .as_array()
                        .is_some_and(|projections| !projections.is_empty())
                }))
        );

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn inspection_outside_a_semantic_unit_is_explicit() {
        let compilation = compilation(SOURCE);
        let target = UnitInspectionTarget::at(0, TextSize::ZERO);

        let (value, diagnostics) = json_inspection(&compilation, target);

        assert!(
            value
                .get("units")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
        );

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn source_inspection_renders_every_bound_unit_in_source_order() {
        let source = concat!(
            "module example;\n",
            "\n",
            "func first()\n",
            "{\n",
            "}\n",
            "\n",
            "func second()\n",
            "{\n",
            "}\n",
        );

        let compilation = compilation(source);
        let target = UnitInspectionTarget::source(0);

        let (value, diagnostics) = json_inspection(&compilation, target);

        let units = value["units"]
            .as_array()
            .unwrap_or_else(|| panic!("bound units must be an array"));

        assert_eq!(value["offset"], Value::Null);
        assert_eq!(units.len(), 2);

        let starts = units
            .iter()
            .filter_map(|unit| unit["source"]["range"]["start"].as_u64())
            .collect::<Vec<_>>();

        assert!(starts.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn source_inspection_includes_nested_anonymous_callable_units() {
        let source = concat!(
            "module example;\n",
            "\n",
            "func main()\n",
            "{\n",
            "    let callable = lambda(value: i32)\n",
            "    {\n",
            "        value;\n",
            "    };\n",
            "}\n",
        );

        let compilation = compilation(source);
        let target = UnitInspectionTarget::source(0);

        let (value, diagnostics) = json_inspection(&compilation, target);

        let units = value["units"]
            .as_array()
            .unwrap_or_else(|| panic!("bound units must be an array"));

        assert_eq!(units.len(), 2);
        assert_eq!(units[0]["nested_units"][0]["unit_kind"], "anonymous_callable");
        assert_eq!(units[1]["unit_kind"], "anonymous_callable");
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn inspection_rejects_an_unknown_source() {
        let compilation = compilation(SOURCE);
        let target = UnitInspectionTarget::at(1, TextSize::ZERO);

        assert_eq!(
            render_bound_inspection(&compilation, target, DriverOutputFormat::Json).map(|_| ()),
            Err(super::BoundInspectionRenderError::Source)
        );
    }

    #[test]
    fn inspection_rejects_an_offset_past_the_source() {
        let compilation = compilation(SOURCE);

        let position = match TextSize::try_from(SOURCE.len() + 1) {
            Ok(position) => position,
            Err(_) => panic!("test source offset must fit"),
        };

        let target = UnitInspectionTarget::at(0, position);

        assert_eq!(
            render_bound_inspection(&compilation, target, DriverOutputFormat::Json).map(|_| ()),
            Err(super::BoundInspectionRenderError::Source)
        );
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

        let (value, diagnostics) = json_inspection(&compilation, target);

        assert!(
            value
                .get("units")
                .and_then(Value::as_array)
                .is_some_and(|units| units.len() == 1)
        );

        assert_eq!(value["units"][0]["root"]["recovered"], true);
        assert!(diagnostics.has_errors());
    }

    fn target_at(source: &str, needle: &str) -> UnitInspectionTarget {
        let offset = source
            .find(needle)
            .unwrap_or_else(|| panic!("test source must contain {needle}"));

        let position = match TextSize::try_from(offset) {
            Ok(position) => position,
            Err(_) => panic!("test source offset must fit"),
        };

        UnitInspectionTarget::at(0, position)
    }

    fn json_inspection(
        compilation: &Compilation,
        target: UnitInspectionTarget,
    ) -> (Value, DiagnosticBag) {
        let output = match render_bound_inspection(compilation, target, DriverOutputFormat::Json) {
            Ok(output) => output,
            Err(error) => panic!("bound inspection should render: {error:?}"),
        };

        let (json, diagnostics) = output.into_parts();

        let value = match serde_json::from_str(&json) {
            Ok(value) => value,
            Err(error) => panic!("bound inspection JSON must parse: {error}"),
        };

        (value, diagnostics)
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
