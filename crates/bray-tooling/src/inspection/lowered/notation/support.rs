use std::fmt::Write;

use super::super::model::{
    InspectionMirAttribute, InspectionMirEdge, InspectionMirNamedOperand, InspectionMirNamedPlace,
    InspectionMirNamedSymbol, InspectionMirNamedType, InspectionMirOperand, InspectionMirOperation,
    InspectionMirPlace, InspectionMirSemanticValue, InspectionMirSource, InspectionMirTerminator,
    InspectionMirUnit,
};

pub(super) trait OperandCollection {
    fn operands(&self) -> &[InspectionMirNamedOperand];
}

pub(super) trait PlaceCollection {
    fn places(&self) -> &[InspectionMirNamedPlace];
}

pub(super) trait AttributeCollection {
    fn attributes(&self) -> &[InspectionMirAttribute];
}

pub(super) trait TypeCollection {
    fn types(&self) -> &[InspectionMirNamedType];
}

pub(super) trait SymbolCollection {
    fn symbols(&self) -> &[InspectionMirNamedSymbol];
}

pub(super) trait SemanticValueCollection {
    fn semantic_values(&self) -> &[InspectionMirSemanticValue];
}

pub(super) trait EdgeCollection {
    fn edges(&self) -> &[InspectionMirEdge];
}

impl OperandCollection for InspectionMirOperation {
    fn operands(&self) -> &[InspectionMirNamedOperand] {
        &self.operands
    }
}

impl OperandCollection for InspectionMirTerminator {
    fn operands(&self) -> &[InspectionMirNamedOperand] {
        &self.operands
    }
}

impl PlaceCollection for InspectionMirOperation {
    fn places(&self) -> &[InspectionMirNamedPlace] {
        &self.places
    }
}

impl PlaceCollection for InspectionMirTerminator {
    fn places(&self) -> &[InspectionMirNamedPlace] {
        &self.places
    }
}

impl AttributeCollection for InspectionMirOperation {
    fn attributes(&self) -> &[InspectionMirAttribute] {
        &self.attributes
    }
}

impl AttributeCollection for InspectionMirTerminator {
    fn attributes(&self) -> &[InspectionMirAttribute] {
        &self.attributes
    }
}

impl TypeCollection for InspectionMirOperation {
    fn types(&self) -> &[InspectionMirNamedType] {
        &self.types
    }
}

impl TypeCollection for InspectionMirTerminator {
    fn types(&self) -> &[InspectionMirNamedType] {
        &self.types
    }
}

impl SymbolCollection for InspectionMirOperation {
    fn symbols(&self) -> &[InspectionMirNamedSymbol] {
        &self.symbols
    }
}

impl SymbolCollection for InspectionMirTerminator {
    fn symbols(&self) -> &[InspectionMirNamedSymbol] {
        &self.symbols
    }
}

impl SemanticValueCollection for InspectionMirOperation {
    fn semantic_values(&self) -> &[InspectionMirSemanticValue] {
        &self.semantic_values
    }
}

impl SemanticValueCollection for InspectionMirTerminator {
    fn semantic_values(&self) -> &[InspectionMirSemanticValue] {
        &self.semantic_values
    }
}

impl EdgeCollection for InspectionMirTerminator {
    fn edges(&self) -> &[InspectionMirEdge] {
        &self.edges
    }
}

pub(super) fn operand_for<T>(value: &T, role: &str) -> String
where
    T: OperandCollection,
{
    value
        .operands()
        .iter()
        .find(|operand| operand.role == role)
        .map(|operand| operand_text(&operand.operand))
        .unwrap_or_else(|| "<missing operand>".into())
}

pub(super) fn optional_operand<T>(value: &T, role: &str) -> String
where
    T: OperandCollection,
{
    value
        .operands()
        .iter()
        .find(|operand| operand.role == role)
        .map(|operand| format!(" {}", operand_text(&operand.operand)))
        .unwrap_or_default()
}

pub(super) fn place_for<T>(value: &T, role: &str) -> String
where
    T: PlaceCollection,
{
    value
        .places()
        .iter()
        .find(|place| place.role == role)
        .map(|place| place_text(&place.place))
        .unwrap_or_else(|| "<missing place>".into())
}

pub(super) fn attribute_text<T>(value: &T, name: &str) -> Option<String>
where
    T: AttributeCollection,
{
    value
        .attributes()
        .iter()
        .find(|attribute| attribute.name == name)
        .map(|attribute| attribute.value.text())
}

pub(super) fn type_for<'a, T>(value: &'a T, role: &str) -> Option<&'a str>
where
    T: TypeCollection,
{
    value
        .types()
        .iter()
        .find(|r#type| r#type.role == role)
        .map(|r#type| r#type.r#type.text())
}

pub(super) fn symbol_for<T>(value: &T, role: &str) -> String
where
    T: SymbolCollection,
{
    value
        .symbols()
        .iter()
        .find(|symbol| symbol.role == role)
        .map(|symbol| format!("@{}", symbol.symbol.display_name()))
        .unwrap_or_else(|| "<missing symbol>".into())
}

pub(super) fn semantic_for<'a, T>(
    value: &'a T,
    role: &str,
) -> Option<&'a InspectionMirSemanticValue>
where
    T: SemanticValueCollection,
{
    value
        .semantic_values()
        .iter()
        .find(|semantic| semantic.role == role)
}

pub(super) fn edge_for<'a, T>(value: &'a T, role: &str) -> Option<&'a InspectionMirEdge>
where
    T: EdgeCollection,
{
    value.edges().iter().find(|edge| edge.role == role)
}

pub(super) fn edge_text_for<T>(value: &T, role: &str) -> String
where
    T: EdgeCollection,
{
    edge_for(value, role)
        .map(edge_text)
        .unwrap_or_else(|| "<missing edge>".into())
}

pub(super) fn cleanup_edge_text_for<T>(value: &T, role: &str) -> String
where
    T: EdgeCollection,
{
    edge_for(value, role)
        .map(cleanup_edge_text)
        .unwrap_or_else(|| "cleanup -> <missing edge>".into())
}

pub(super) fn semantic_text(value: &InspectionMirSemanticValue) -> String {
    value
        .text
        .clone()
        .unwrap_or_else(|| format!("{}#{}", value.value_kind, value.id))
}

pub(super) fn edge_text(edge: &InspectionMirEdge) -> String {
    let arguments = edge
        .arguments
        .iter()
        .map(operand_text)
        .collect::<Vec<_>>()
        .join(", ");

    if arguments.is_empty() {
        format!("bb{}", edge.target)
    } else {
        format!("bb{}({arguments})", edge.target)
    }
}

pub(super) fn cleanup_edge_text(edge: &InspectionMirEdge) -> String {
    let phase = edge.cleanup_phase.unwrap_or("cleanup");

    format!("cleanup[{phase}] -> {}", edge_text(edge))
}

pub(super) fn operand_text(operand: &InspectionMirOperand) -> String {
    match operand {
        InspectionMirOperand::Value { value } => format!("%{value}"),
        InspectionMirOperand::Constant { value, r#type }
        | InspectionMirOperand::Immediate { value, r#type } => {
            format!("{value}: {}", r#type.text())
        }
        InspectionMirOperand::Copy { place } => format!("copy {}", place_text(place)),
        InspectionMirOperand::Move { place } => format!("move {}", place_text(place)),
    }
}

pub(super) fn place_text(place: &InspectionMirPlace) -> String {
    let mut text = format!("slot{}", place.storage);

    for projection in &place.projections {
        match projection.projection_kind {
            "dereference" => text.push_str(".*"),
            "field" | "variant" | "active_union_payload_field" => {
                if let Some(detail) = &projection.detail {
                    let _ = write!(text, ".{detail}");
                }
            }
            "tuple_field" => {
                let _ = write!(text, ".{}", projection.detail.as_deref().unwrap_or("?"));
            }
            "element_from_start" => {
                let _ = write!(text, "[{}]", projection.detail.as_deref().unwrap_or("?"));
            }
            "element_from_end" => {
                let _ = write!(
                    text,
                    "[end - {}]",
                    projection.detail.as_deref().unwrap_or("?")
                );
            }
            "index" => text.push_str("[index]"),
            "slice" => text.push_str("[..]"),
            "nullable_value" => text.push_str(".value"),
            "owned_storage" => text.push_str(".owned"),
            _ => {
                let _ = write!(text, ".{}", projection.projection_kind);
            }
        }
    }

    text
}

pub(super) fn source_annotation(
    source: &InspectionMirSource,
    block_source: &InspectionMirSource,
    include_source: bool,
) -> String {
    if !include_source {
        return String::new();
    }

    let source = source_text(source);

    if source == source_text(block_source) {
        String::new()
    } else {
        format!(" // source: {source}")
    }
}

pub(in crate::inspection::lowered) fn source_text(source: &InspectionMirSource) -> String {
    match source {
        InspectionMirSource::CompilerProvidedCallable { callable } => {
            format!("compiler-provided {}", callable.text())
        }
        InspectionMirSource::Source { syntax, synthesis } => {
            let mut text = syntax.text();

            if let Some(synthesis) = synthesis {
                let _ = write!(
                    text,
                    " synthesized {}:{}",
                    synthesis.role, synthesis.ordinal
                );
            }

            text
        }
        InspectionMirSource::ExecutableHost { package, product } => {
            format!("generated host {package}/{product}")
        }
        InspectionMirSource::GeneratedLifecycle { role } => {
            format!("generated lifecycle {role}")
        }
        InspectionMirSource::ImportedExecutable { owner, template } => {
            format!("imported {} template:{template}", owner.text())
        }
    }
}

pub(super) fn value_type(unit: &InspectionMirUnit, value: u32) -> Option<&str> {
    unit.values
        .iter()
        .find(|candidate| candidate.id == value)
        .map(|value| value.r#type.text())
}

pub(super) fn frame_execution_text(
    execution: &super::super::model::InspectionMirFrameExecution,
) -> String {
    let retained = execution
        .retained_storages
        .iter()
        .map(|storage| format!("slot{storage}"))
        .collect::<Vec<_>>()
        .join(", ");

    let lanes = execution.lane_requirements.join(", ");

    format!(
        "affinity {} lanes [{lanes}] retained [{retained}]",
        execution.affinity
    )
}
