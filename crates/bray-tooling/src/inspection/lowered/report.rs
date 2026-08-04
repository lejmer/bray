use bray_compilation::Compilation;
use bray_diagnostics::DiagnosticBag;
use bray_lowering::LoweredUnit;
use serde::Serialize;

use crate::inspection::lowered::model::{
    InspectionMirOperation, InspectionMirSource, InspectionMirTerminator, InspectionMirUnit,
    MirInspectionModelError,
};
use crate::inspection::lowered::notation;
use crate::inspection::unit::{UnitInspectionSelectionError, select_units};
use crate::inspection::{
    InspectionOutput, InspectionSourceError, InspectionSources, InspectionSyntaxAnchor, TreeWriter,
    push_text_diagnostic, render_pretty_json,
};
use crate::output::{DiagnosticJson, diagnostic_jsons};
use crate::{InspectionTarget, OutputFormat};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LoweredInspectionRenderError {
    Json,
    LoweringFact,
    Model,
    Source,
    SymbolFact,
    UnitFact,
}

impl From<InspectionSourceError> for LoweredInspectionRenderError {
    fn from(_: InspectionSourceError) -> Self {
        Self::Source
    }
}

impl From<MirInspectionModelError> for LoweredInspectionRenderError {
    fn from(_: MirInspectionModelError) -> Self {
        Self::Model
    }
}

impl From<UnitInspectionSelectionError> for LoweredInspectionRenderError {
    fn from(error: UnitInspectionSelectionError) -> Self {
        match error {
            UnitInspectionSelectionError::BoundFact => Self::UnitFact,
            UnitInspectionSelectionError::Source => Self::Source,
        }
    }
}

pub(crate) fn render_lowered_inspection(
    compilation: &Compilation,
    target: InspectionTarget,
    output_format: OutputFormat,
) -> Result<InspectionOutput, LoweredInspectionRenderError> {
    let (units, diagnostics) = inspect_units(compilation, target)?;

    let report = LoweredInspectionReport::new(target, units, &diagnostics, compilation);

    let stdout = match output_format {
        OutputFormat::Text => render_text_report(&report),
        OutputFormat::Json => {
            render_pretty_json(&report).map_err(|_| LoweredInspectionRenderError::Json)?
        }
    };

    Ok(InspectionOutput::new(stdout, diagnostics))
}

pub(crate) fn render_mir_inspection(
    compilation: &Compilation,
    target: InspectionTarget,
    output_format: OutputFormat,
) -> Result<InspectionOutput, LoweredInspectionRenderError> {
    let (units, diagnostics) = inspect_units(compilation, target)?;

    let notations = units
        .iter()
        .map(InspectionMirNotation::from_unit)
        .collect::<Vec<_>>();

    let stdout = match output_format {
        OutputFormat::Text => render_notation_text(&notations, &diagnostics, compilation),
        OutputFormat::Json => {
            let report = MirNotationReport {
                kind: "mir_inspection",
                source_id: target.source_id(),
                offset: target.position().map(u32::from),
                units: notations,
                has_errors: diagnostics.has_errors(),
                diagnostics: diagnostic_jsons(&diagnostics, Some(compilation.sources())),
            };

            render_pretty_json(&report).map_err(|_| LoweredInspectionRenderError::Json)?
        }
    };

    Ok(InspectionOutput::new(stdout, diagnostics))
}

fn inspect_units(
    compilation: &Compilation,
    target: InspectionTarget,
) -> Result<(Vec<InspectionLoweredUnit>, DiagnosticBag), LoweredInspectionRenderError> {
    let selection = select_units(compilation, target)?;

    let (source_diagnostics, bounds) = selection.into_parts();

    if bounds.is_empty() {
        return Ok((Vec::new(), source_diagnostics));
    }

    let symbols = compilation
        .symbol_graph()
        .map_err(|_| LoweredInspectionRenderError::SymbolFact)?;

    let semantic_values = compilation
        .semantic_value_store()
        .map_err(|_| LoweredInspectionRenderError::SymbolFact)?;

    let sources = InspectionSources::new(compilation.sources())?;
    let mut diagnostics = source_diagnostics;
    let mut units = Vec::with_capacity(bounds.len());

    for bound in bounds {
        // The lowering fact owns its unit key independently of the selected bound fact.
        let key = bound.value().key().clone();

        let lowered = compilation
            .lowered_unit(key)
            .map_err(|_| LoweredInspectionRenderError::LoweringFact)?;

        diagnostics =
            DiagnosticBag::merged_all([&diagnostics, bound.diagnostics(), lowered.diagnostics()]);

        let source =
            InspectionSyntaxAnchor::from_anchor(&sources, bound.value().key().source().syntax())?;

        let unit_kind = bound.value().key().kind().as_str();

        let unit = match lowered.value() {
            Some(LoweredUnit::Mir(mir)) => InspectionLoweredUnit::Mir {
                mir: Box::new(InspectionMirUnit::from_mir(
                    mir,
                    symbols,
                    semantic_values,
                    &sources,
                )?),
            },
            Some(LoweredUnit::CompileTime(_)) => {
                InspectionLoweredUnit::CompileTime { unit_kind, source }
            }
            None => InspectionLoweredUnit::Unavailable { unit_kind, source },
        };

        units.push(unit);
    }

    Ok((units, diagnostics))
}

#[derive(Serialize)]
struct LoweredInspectionReport {
    kind: &'static str,
    source_id: u32,
    offset: Option<u32>,
    units: Vec<InspectionLoweredUnit>,
    has_errors: bool,
    diagnostics: Vec<DiagnosticJson>,
}

impl LoweredInspectionReport {
    fn new(
        target: InspectionTarget,
        units: Vec<InspectionLoweredUnit>,
        diagnostics: &DiagnosticBag,
        compilation: &Compilation,
    ) -> Self {
        Self {
            kind: "lowered_inspection",
            source_id: target.source_id(),
            offset: target.position().map(u32::from),
            units,
            has_errors: diagnostics.has_errors(),
            diagnostics: diagnostic_jsons(diagnostics, Some(compilation.sources())),
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "representation", rename_all = "snake_case")]
enum InspectionLoweredUnit {
    Mir {
        mir: Box<InspectionMirUnit>,
    },
    CompileTime {
        unit_kind: &'static str,
        source: InspectionSyntaxAnchor,
    },
    Unavailable {
        unit_kind: &'static str,
        source: InspectionSyntaxAnchor,
    },
}

#[derive(Serialize)]
struct MirNotationReport {
    kind: &'static str,
    source_id: u32,
    offset: Option<u32>,
    units: Vec<InspectionMirNotation>,
    has_errors: bool,
    diagnostics: Vec<DiagnosticJson>,
}

#[derive(Serialize)]
struct InspectionMirNotation {
    unit_kind: &'static str,
    unit_id: Option<u32>,
    representation: &'static str,
    notation: Option<String>,
}

impl InspectionMirNotation {
    fn from_unit(unit: &InspectionLoweredUnit) -> Self {
        match unit {
            InspectionLoweredUnit::Mir { mir } => Self {
                unit_kind: mir.unit_kind,
                unit_id: Some(mir.unit_id),
                representation: "mir",
                notation: Some(notation::render_unit(mir)),
            },
            InspectionLoweredUnit::CompileTime { unit_kind, .. } => Self {
                unit_kind,
                unit_id: None,
                representation: "compile_time",
                notation: None,
            },
            InspectionLoweredUnit::Unavailable { unit_kind, .. } => Self {
                unit_kind,
                unit_id: None,
                representation: "unavailable",
                notation: None,
            },
        }
    }
}

fn render_text_report(report: &LoweredInspectionReport) -> String {
    let mut output = match report.offset {
        Some(offset) => format!(
            "Lowered units at source {} offset {offset}\n",
            report.source_id
        ),
        None => format!("Lowered units for source {}\n", report.source_id),
    };

    if report.units.is_empty() {
        output.push_str("No independently lowered units were selected.\n");
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

fn push_text_unit(output: &mut String, unit: &InspectionLoweredUnit) {
    match unit {
        InspectionLoweredUnit::Mir { mir } => push_text_mir(output, mir),
        InspectionLoweredUnit::CompileTime { unit_kind, source } => {
            output.push_str(&format!(
                "Unit {unit_kind} [compile-time] {}\n",
                source.location_text()
            ));
        }
        InspectionLoweredUnit::Unavailable { unit_kind, source } => {
            output.push_str(&format!(
                "Unit {unit_kind} [unavailable] {}\n",
                source.location_text()
            ));
        }
    }
}

fn push_text_mir(output: &mut String, mir: &InspectionMirUnit) {
    output.push_str(&format!(
        "Unit {} [unit:{}] target={} abi={}\n",
        mir.unit_kind, mir.unit_id, mir.target, mir.runtime_abi
    ));

    let mut writer = TreeWriter::new("  ");
    let has_frame = mir.frame.is_some();

    writer.push_line(false, &format!("Source {}", source_text(&mir.source)));
    writer.push_line(false, &format!("Entry block:{}", mir.entry));

    writer.push_line(false, &format!("Storage ({})", mir.storages.len()));
    writer.enter_children(false);

    for (index, storage) in mir.storages.iter().enumerate() {
        writer.push_line(
            index + 1 == mir.storages.len(),
            &format!(
                "{} storage:{} {}",
                storage.storage_kind,
                storage.id,
                storage.r#type.text()
            ),
        );
    }

    writer.leave_children();

    writer.push_line(false, &format!("Values ({})", mir.values.len()));
    writer.enter_children(false);

    for (index, value) in mir.values.iter().enumerate() {
        writer.push_line(
            index + 1 == mir.values.len(),
            &format!(
                "value:{} {} <- {}:{}",
                value.id,
                value.r#type.text(),
                value.origin.kind,
                value.origin.id
            ),
        );
    }

    writer.leave_children();

    writer.push_line(!has_frame, &format!("Blocks ({})", mir.blocks.len()));
    writer.enter_children(!has_frame);

    for (index, block) in mir.blocks.iter().enumerate() {
        let block_is_last = index + 1 == mir.blocks.len();

        writer.push_line(
            block_is_last,
            &format!("{} block:{}", block.block_kind, block.id),
        );

        writer.enter_children(block_is_last);

        for operation in &block.operations {
            push_text_operation(&mut writer, operation, false);
        }

        push_text_terminator(&mut writer, &block.terminator);
        writer.leave_children();
    }

    writer.leave_children();

    if let Some(frame) = &mir.frame {
        writer.push_line(true, &format!("Frame {} abi={}", frame.id, frame.abi));
        writer.enter_children(true);

        for (index, state) in frame.states.iter().enumerate() {
            writer.push_line(
                index + 1 == frame.states.len(),
                &format!("state:{} -> block:{}", state.id, state.entry),
            );
        }

        writer.leave_children();
    }

    output.push_str(&writer.into_string());
}

fn push_text_operation(writer: &mut TreeWriter, operation: &InspectionMirOperation, is_last: bool) {
    let result = operation
        .result
        .map(|value| format!(" -> value:{value}"))
        .unwrap_or_default();

    writer.push_line(
        is_last,
        &format!(
            "{} operation:{}{result}",
            operation.operation_kind, operation.id
        ),
    );

    writer.enter_children(is_last);

    let detail_count = operation.attributes.len()
        + operation.operands.len()
        + operation.places.len()
        + operation.symbols.len()
        + operation.types.len()
        + operation.semantic_values.len();

    let mut detail_index = 0;

    for attribute in &operation.attributes {
        detail_index += 1;

        writer.push_line(
            detail_index == detail_count,
            &format!("{}={}", attribute.name, attribute.value.text()),
        );
    }

    for operand in &operation.operands {
        detail_index += 1;

        writer.push_line(
            detail_index == detail_count,
            &format!("{}={}", operand.role, operand_text(&operand.operand)),
        );
    }

    for place in &operation.places {
        detail_index += 1;

        writer.push_line(
            detail_index == detail_count,
            &format!("{}=storage:{}", place.role, place.place.storage),
        );
    }

    for symbol in &operation.symbols {
        detail_index += 1;

        writer.push_line(
            detail_index == detail_count,
            &format!("{}={}", symbol.role, symbol.symbol.text()),
        );
    }

    for r#type in &operation.types {
        detail_index += 1;

        writer.push_line(
            detail_index == detail_count,
            &format!("{}={}", r#type.role, r#type.r#type.text()),
        );
    }

    for semantic_value in &operation.semantic_values {
        detail_index += 1;

        writer.push_line(
            detail_index == detail_count,
            &semantic_value_text(semantic_value),
        );
    }

    writer.leave_children();
}

fn push_text_terminator(writer: &mut TreeWriter, terminator: &InspectionMirTerminator) {
    writer.push_line(true, &format!("{} terminator", terminator.terminator_kind));

    let detail_count = terminator.attributes.len()
        + terminator.operands.len()
        + terminator.places.len()
        + terminator.symbols.len()
        + terminator.types.len()
        + terminator.semantic_values.len()
        + terminator.edges.len();

    if detail_count == 0 {
        return;
    }

    writer.enter_children(true);

    let mut detail_index = 0;

    for attribute in &terminator.attributes {
        detail_index += 1;

        writer.push_line(
            detail_index == detail_count,
            &format!("{}={}", attribute.name, attribute.value.text()),
        );
    }

    for operand in &terminator.operands {
        detail_index += 1;

        writer.push_line(
            detail_index == detail_count,
            &format!("{}={}", operand.role, operand_text(&operand.operand)),
        );
    }

    for place in &terminator.places {
        detail_index += 1;

        writer.push_line(
            detail_index == detail_count,
            &format!("{}=storage:{}", place.role, place.place.storage),
        );
    }

    for symbol in &terminator.symbols {
        detail_index += 1;

        writer.push_line(
            detail_index == detail_count,
            &format!("{}={}", symbol.role, symbol.symbol.text()),
        );
    }

    for r#type in &terminator.types {
        detail_index += 1;

        writer.push_line(
            detail_index == detail_count,
            &format!("{}={}", r#type.role, r#type.r#type.text()),
        );
    }

    for semantic_value in &terminator.semantic_values {
        detail_index += 1;

        writer.push_line(
            detail_index == detail_count,
            &semantic_value_text(semantic_value),
        );
    }

    for edge in &terminator.edges {
        detail_index += 1;

        let cleanup = edge
            .cleanup_phase
            .map(|phase| format!(" [{phase}]"))
            .unwrap_or_default();

        let arguments = edge
            .arguments
            .iter()
            .map(operand_text)
            .collect::<Vec<_>>()
            .join(", ");

        writer.push_line(
            detail_index == detail_count,
            &format!(
                "{} -> block:{}({arguments}){cleanup}",
                edge.role, edge.target
            ),
        );
    }

    writer.leave_children();
}

fn semantic_value_text(value: &super::model::InspectionMirSemanticValue) -> String {
    let text = value
        .text
        .as_ref()
        .map(|text| format!(" {text}"))
        .unwrap_or_default();

    format!("{}={}:{}{}", value.role, value.value_kind, value.id, text)
}

fn render_notation_text(
    units: &[InspectionMirNotation],
    diagnostics: &DiagnosticBag,
    compilation: &Compilation,
) -> String {
    let mut output = String::new();

    for (index, unit) in units.iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }

        match &unit.notation {
            Some(notation) => output.push_str(notation),
            None => output.push_str(&format!(
                "// {} unit has {} representation\n",
                unit.unit_kind, unit.representation
            )),
        }
    }

    let diagnostics = diagnostic_jsons(diagnostics, Some(compilation.sources()));

    if !diagnostics.is_empty() {
        output.push_str("\nDiagnostics:\n");

        for diagnostic in &diagnostics {
            push_text_diagnostic(&mut output, diagnostic);
        }
    }

    output
}

fn source_text(source: &InspectionMirSource) -> String {
    match source {
        InspectionMirSource::Source { syntax, synthesis } => {
            let synthesis = synthesis
                .as_ref()
                .map(|synthesis| format!(" synthesized {}:{}", synthesis.role, synthesis.ordinal))
                .unwrap_or_default();

            format!("{}{}", syntax.location_text(), synthesis)
        }
        InspectionMirSource::ExecutableHost { package, product } => {
            format!("generated {package}/{product}")
        }
        InspectionMirSource::GeneratedLifecycle { role } => {
            format!("generated lifecycle {role}")
        }
    }
}

fn operand_text(operand: &super::model::InspectionMirOperand) -> String {
    use super::model::InspectionMirOperand;

    match operand {
        InspectionMirOperand::Value { value } => format!("value:{value}"),
        InspectionMirOperand::Constant { value, r#type } => {
            format!("constant {value}: {}", r#type.text())
        }
        InspectionMirOperand::Immediate { value, r#type } => {
            format!("immediate {value}: {}", r#type.text())
        }
        InspectionMirOperand::Copy { place } => format!("copy storage:{}", place.storage),
        InspectionMirOperand::Move { place } => format!("move storage:{}", place.storage),
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundUnitKind;
    use bray_compilation::Compilation;
    use bray_ir::MirUnitId;
    use bray_lowering::{ExecutableHostLoweringInput, lower_executable_host};
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};
    use bray_symbols::PackageIdentity;
    use bray_testing::{test_executable_host_contract, test_mir_target};
    use serde_json::Value;

    use super::{InspectionMirUnit, render_lowered_inspection, render_mir_inspection};
    use crate::inspection::InspectionSources;
    use crate::{InspectionTarget, OutputFormat};

    const SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "func main() -> i32\n",
        "{\n",
        "    let value: i32 = 1;\n",
        "\n",
        "    return value;\n",
        "}\n",
    );

    #[test]
    fn lowered_inspection_exposes_typed_cross_references() {
        let compilation = compilation(SOURCE);

        let output = render_lowered_inspection(
            &compilation,
            InspectionTarget::source(0),
            OutputFormat::Json,
        )
        .unwrap_or_else(|error| panic!("lowered inspection should render: {error:?}"));

        let (json, diagnostics) = output.into_parts();

        let value: Value = serde_json::from_str(&json)
            .unwrap_or_else(|error| panic!("lowered inspection JSON must parse: {error}"));

        assert_eq!(value["kind"], "lowered_inspection");
        assert_eq!(value["units"][0]["representation"], "mir");
        assert!(value["units"][0]["mir"]["blocks"].is_array());
        assert!(value["units"][0]["mir"]["values"].is_array());
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn structural_text_uses_final_connector_for_block_terminators() {
        let compilation = compilation(SOURCE);

        let output = render_lowered_inspection(
            &compilation,
            InspectionTarget::source(0),
            OutputFormat::Text,
        )
        .unwrap_or_else(|error| panic!("lowered inspection should render: {error:?}"));

        let (text, diagnostics) = output.into_parts();

        assert!(
            text.lines()
                .any(|line| { line.trim_start() == "└─ return terminator" })
        );

        assert!(
            !text
                .lines()
                .any(|line| { line.trim_start() == "├─ return terminator" })
        );

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn mir_notation_is_deterministic_and_code_like() {
        let compilation = compilation(SOURCE);
        let target = InspectionTarget::source(0);

        let first = render_mir_inspection(&compilation, target, OutputFormat::Text)
            .unwrap_or_else(|error| panic!("MIR notation should render: {error:?}"));

        let second = render_mir_inspection(&compilation, target, OutputFormat::Text)
            .unwrap_or_else(|error| panic!("MIR notation should remain available: {error:?}"));

        let (first, first_diagnostics) = first.into_parts();

        let (second, second_diagnostics) = second.into_parts();

        assert_eq!(first, second);
        assert!(first.contains("mir unit"));
        assert!(first.contains("bb0("));
        assert!(first.contains("return"));
        assert!(first_diagnostics.is_empty(), "{first_diagnostics:?}");
        assert!(second_diagnostics.is_empty(), "{second_diagnostics:?}");
    }

    #[test]
    fn mir_json_wraps_notation_without_flattening_the_structural_report() {
        let compilation = compilation(SOURCE);

        let output = render_mir_inspection(
            &compilation,
            InspectionTarget::source(0),
            OutputFormat::Json,
        )
        .unwrap_or_else(|error| panic!("MIR JSON should render: {error:?}"));

        let (json, diagnostics) = output.into_parts();

        let value: Value = serde_json::from_str(&json)
            .unwrap_or_else(|error| panic!("MIR JSON must parse: {error}"));

        assert_eq!(value["kind"], "mir_inspection");

        assert!(
            value["units"][0]["notation"]
                .as_str()
                .is_some_and(|notation| notation.contains("mir unit"))
        );

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn recovered_lowering_outcomes_remain_inspectable() {
        let source = concat!(
            "module app;\n",
            "\n",
            "func main() -> i32\n",
            "{\n",
            "    return ;\n",
            "}\n",
        );

        let compilation = compilation(source);

        let output = render_lowered_inspection(
            &compilation,
            InspectionTarget::source(0),
            OutputFormat::Json,
        )
        .unwrap_or_else(|error| panic!("recovered lowering should remain inspectable: {error:?}"));

        let (json, diagnostics) = output.into_parts();

        let value: Value = serde_json::from_str(&json)
            .unwrap_or_else(|error| panic!("recovered lowered JSON must parse: {error}"));

        assert!(value["units"].is_array());
        assert!(diagnostics.has_errors());
    }

    #[test]
    fn source_inspection_keeps_compile_time_and_synthesized_units_visible() {
        let source = concat!(
            "module app;\n",
            "\n",
            "const answer: i32 = 42;\n",
            "\n",
            "func main()\n",
            "{\n",
            "    lambda() -> i32\n",
            "    {\n",
            "        return answer;\n",
            "    };\n",
            "}\n",
        );

        let compilation = compilation(source);

        let output = render_lowered_inspection(
            &compilation,
            InspectionTarget::source(0),
            OutputFormat::Json,
        )
        .unwrap_or_else(|error| {
            panic!("compile-time and synthesized units should remain inspectable: {error:?}")
        });

        let (json, diagnostics) = output.into_parts();

        let value: Value = serde_json::from_str(&json)
            .unwrap_or_else(|error| panic!("lowered inspection JSON must parse: {error}"));

        let units = value["units"]
            .as_array()
            .unwrap_or_else(|| panic!("lowered inspection must contain units"));

        assert!(
            units
                .iter()
                .any(|unit| unit["representation"] == "compile_time")
        );

        assert!(units.iter().any(|unit| unit["representation"] == "mir"));
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn generated_executable_host_mir_has_a_complete_structural_projection() {
        let compilation = compilation(SOURCE);

        let (_, units) =
            crate::inspection::unit::select_units(&compilation, InspectionTarget::source(0))
                .unwrap_or_else(|error| panic!("source units must be available: {error:?}"))
                .into_parts();

        let root = units
            .into_iter()
            .find(|unit| unit.value().key().kind() == BoundUnitKind::CallableBody)
            .map(|unit| unit.value().key().clone())
            .unwrap_or_else(|| panic!("test source must contain a callable body"));

        let input = ExecutableHostLoweringInput::new(
            MirUnitId::new(91),
            [root],
            test_executable_host_contract(),
            test_mir_target(),
        );

        let mir = lower_executable_host(input)
            .unwrap_or_else(|error| panic!("generated host MIR must lower: {error:?}"));

        let sources = InspectionSources::new(compilation.sources())
            .unwrap_or_else(|error| panic!("inspection sources must be available: {error:?}"));

        let model = InspectionMirUnit::from_mir(
            &mir,
            compilation
                .symbol_graph()
                .unwrap_or_else(|error| panic!("symbols must be available: {error:?}")),
            compilation
                .semantic_value_store()
                .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}")),
            &sources,
        )
        .unwrap_or_else(|error| panic!("generated host MIR must project: {error:?}"));

        let value = serde_json::to_value(model)
            .unwrap_or_else(|error| panic!("generated host JSON must serialize: {error:?}"));

        assert_eq!(value["unit_kind"], "executable_host");
        assert_eq!(value["key"]["kind"], "executable_host");
        assert!(value.to_string().contains("root_execution"));
        assert!(value.to_string().contains("\"runtime_abi\""));
    }

    #[test]
    fn protected_frames_expose_states_and_runtime_references() {
        let source = concat!(
            "module app;\n",
            "\n",
            "async func main() -> i32\n",
            "{\n",
            "    let pending = child();\n",
            "    let value: i32 = await pending;\n",
            "\n",
            "    return value;\n",
            "}\n",
            "\n",
            "async func child() -> i32\n",
            "{\n",
            "    return 1;\n",
            "}\n",
        );

        let compilation = compilation(source);

        let output = render_lowered_inspection(
            &compilation,
            InspectionTarget::source(0),
            OutputFormat::Json,
        )
        .unwrap_or_else(|error| panic!("protected-frame inspection should render: {error:?}"));

        let (json, diagnostics) = output.into_parts();

        let value: Value = serde_json::from_str(&json)
            .unwrap_or_else(|error| panic!("protected-frame JSON must parse: {error}"));

        let serialized = value.to_string();

        assert!(serialized.contains("protected_async_frame"));
        assert!(serialized.contains("\"states\""));
        assert!(serialized.contains("suspension_registration"));
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn position_selection_lowers_only_the_selected_unit() {
        let source = concat!(
            "module app;\n",
            "\n",
            "func first() -> i32\n",
            "{\n",
            "    return 1;\n",
            "}\n",
            "\n",
            "func second() -> i32\n",
            "{\n",
            "    return 2;\n",
            "}\n",
        );

        let offset = source
            .find("return 2")
            .unwrap_or_else(|| panic!("test source must contain the selected unit"));

        let position = bray_source::TextSize::try_from(offset)
            .unwrap_or_else(|error| panic!("test offset must fit: {error:?}"));

        let compilation = compilation(source);

        let output = render_lowered_inspection(
            &compilation,
            InspectionTarget::at(0, position),
            OutputFormat::Json,
        )
        .unwrap_or_else(|error| panic!("narrow lowered inspection should render: {error:?}"));

        let (json, diagnostics) = output.into_parts();

        let value: Value = serde_json::from_str(&json)
            .unwrap_or_else(|error| panic!("narrow lowered JSON must parse: {error}"));

        assert_eq!(value["units"].as_array().map_or(0, std::vec::Vec::len), 1);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    fn compilation(source_text: &str) -> Compilation {
        let source = SourceInput::virtual_text(
            SourceIdentity::new(0),
            "test.bray",
            SourceVersion::new(0),
            source_text,
        );

        let package = PackageIdentity::try_new("test.package")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        Compilation::load_sources(package, vec![source])
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"))
    }
}
