//! Typed semantic-selection inspection records.

use bray_bound_tree::{
    BoundCallableTarget, BoundReferenceTarget, CheckedSemanticSelections, ConstructionTarget,
    SemanticSelection, SelectedOperation,
};
use bray_symbols::{
    AnyLocalSymbolId, LocalSymbolSnapshot, SemanticValueStore, SymbolGraph, TypeId,
};
use serde::Serialize;

use crate::inspection::InspectionSymbolIdentity;
use crate::type_inspection::{InspectionType, TypeInspectionError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SelectionInspectionError {
    Local,
    Type,
}

impl From<TypeInspectionError> for SelectionInspectionError {
    fn from(_: TypeInspectionError) -> Self {
        Self::Type
    }
}

#[derive(Serialize)]
pub(super) struct InspectionSelectionEntry {
    expression: u32,
    selection_kind: &'static str,
    target: Option<InspectionSelectionTarget>,
}

impl InspectionSelectionEntry {
    pub(super) const fn expression(&self) -> u32 {
        self.expression
    }

    pub(super) const fn selection_kind(&self) -> &'static str {
        self.selection_kind
    }

    pub(super) fn target_text(&self) -> Option<String> {
        self.target.as_ref().map(InspectionSelectionTarget::text)
    }
}

#[derive(Serialize)]
#[serde(tag = "target_kind", rename_all = "snake_case")]
enum InspectionSelectionTarget {
    Surface {
        symbol: InspectionSymbolIdentity,
    },
    Local {
        symbol_kind: &'static str,
        id: u32,
        name: Option<String>,
    },
    Indirect {
        r#type: InspectionType,
    },
}

impl InspectionSelectionTarget {
    fn text(&self) -> String {
        match self {
            Self::Surface { symbol } => symbol.text(),
            Self::Local {
                symbol_kind,
                id,
                name,
            } => {
                let name = name
                    .as_ref()
                    .map(|name| format!(" {name}"))
                    .unwrap_or_default();

                format!("{symbol_kind}{name} [local:{id}]")
            }
            Self::Indirect { r#type } => format!("indirect {}", r#type.text()),
        }
    }
}

pub(super) fn selection_entries(
    selections: &CheckedSemanticSelections,
    locals: &LocalSymbolSnapshot,
    symbols: &SymbolGraph,
    semantic_values: &SemanticValueStore,
) -> Result<Vec<InspectionSelectionEntry>, SelectionInspectionError> {
    selections
        .entries()
        .iter()
        .map(|entry| {
            Ok(InspectionSelectionEntry {
                expression: entry.expression().ordinal(),
                selection_kind: selection_kind(entry.selection()),
                target: selection_target(
                    entry.selection(),
                    locals,
                    symbols,
                    semantic_values,
                )?,
            })
        })
        .collect()
}

pub(super) const fn selection_kind(selection: &SemanticSelection) -> &'static str {
    match selection {
        SemanticSelection::Reference(_) => "reference",
        SemanticSelection::Call(_) => "call",
        SemanticSelection::Operation(operation) => operation.kind().as_str(),
        SemanticSelection::Iteration(_) => "iteration",
    }
}

fn selection_target(
    selection: &SemanticSelection,
    locals: &LocalSymbolSnapshot,
    symbols: &SymbolGraph,
    semantic_values: &SemanticValueStore,
) -> Result<Option<InspectionSelectionTarget>, SelectionInspectionError> {
    match selection {
        SemanticSelection::Reference(target) => {
            reference_target(*target, locals, symbols).map(Some)
        }
        SemanticSelection::Call(call) => callable_target(
            call.target(),
            locals,
            symbols,
            semantic_values,
        )
        .map(Some),
        SemanticSelection::Operation(operation) => {
            Ok(operation_target(operation, symbols))
        }
        SemanticSelection::Iteration(_) => Ok(None),
    }
}

fn reference_target(
    target: BoundReferenceTarget,
    locals: &LocalSymbolSnapshot,
    symbols: &SymbolGraph,
) -> Result<InspectionSelectionTarget, SelectionInspectionError> {
    match target {
        BoundReferenceTarget::Local(local) => local_target(local, locals),
        BoundReferenceTarget::Surface(symbol) => Ok(InspectionSelectionTarget::Surface {
            symbol: InspectionSymbolIdentity::from_symbol(symbols, symbol),
        }),
    }
}

fn callable_target(
    target: BoundCallableTarget,
    locals: &LocalSymbolSnapshot,
    symbols: &SymbolGraph,
    semantic_values: &SemanticValueStore,
) -> Result<InspectionSelectionTarget, SelectionInspectionError> {
    match target {
        BoundCallableTarget::Declaration(instance) => Ok(InspectionSelectionTarget::Surface {
            symbol: InspectionSymbolIdentity::from_symbol(
                symbols,
                instance.definition().symbol(),
            ),
        }),
        BoundCallableTarget::Anonymous(callable) => {
            local_target(callable.into(), locals)
        }
        BoundCallableTarget::Indirect(ty) => indirect_target(semantic_values, symbols, ty),
    }
}

fn operation_target(
    operation: &SelectedOperation,
    symbols: &SymbolGraph,
) -> Option<InspectionSelectionTarget> {
    let symbol = match operation {
        SelectedOperation::Member(target) => Some(target.member()),
        SelectedOperation::Construction(construction) => match construction.target() {
            ConstructionTarget::Struct(symbol) => Some(symbol.into()),
            ConstructionTarget::UnionVariant(symbol) => Some(symbol.into()),
            ConstructionTarget::TypeForm { callable, .. } => {
                Some(callable.definition().symbol())
            }
        },
        SelectedOperation::Operator { .. }
        | SelectedOperation::Index { .. }
        | SelectedOperation::Conversion(_)
        | SelectedOperation::Implementation(_) => None,
    };

    symbol.map(|symbol| InspectionSelectionTarget::Surface {
        symbol: InspectionSymbolIdentity::from_symbol(symbols, symbol),
    })
}

fn indirect_target(
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    ty: TypeId,
) -> Result<InspectionSelectionTarget, SelectionInspectionError> {
    Ok(InspectionSelectionTarget::Indirect {
        r#type: InspectionType::from_type(semantic_values, symbols, ty)?,
    })
}

fn local_target(
    target: AnyLocalSymbolId,
    locals: &LocalSymbolSnapshot,
) -> Result<InspectionSelectionTarget, SelectionInspectionError> {
    let (id, name) =
        local_identity(target, locals).ok_or(SelectionInspectionError::Local)?;

    Ok(InspectionSelectionTarget::Local {
        symbol_kind: target.kind().as_str(),
        id,
        name,
    })
}

fn local_identity(
    target: AnyLocalSymbolId,
    locals: &LocalSymbolSnapshot,
) -> Option<(u32, Option<String>)> {
    match target {
        AnyLocalSymbolId::Binding(target) => locals.binding(target).map(|symbol| {
            (
                target.ordinal(),
                Some(symbol.name().as_str().to_owned()),
            )
        }),
        AnyLocalSymbolId::Constant(target) => locals.constant(target).map(|symbol| {
            (
                target.ordinal(),
                Some(symbol.name().as_str().to_owned()),
            )
        }),
        AnyLocalSymbolId::AnonymousCallable(target) => locals
            .anonymous_callable(target)
            .map(|_| {
                (
                    target.ordinal(),
                    None,
                )
            }),
        AnyLocalSymbolId::AnonymousCallableParameter(target) => {
            locals.anonymous_parameter(target).map(|symbol| {
                (
                    target.ordinal(),
                    Some(symbol.name().as_str().to_owned()),
                )
            })
        }
        AnyLocalSymbolId::PostconditionResult(target) => locals
            .postcondition_result(target)
            .map(|_| {
                (
                    target.ordinal(),
                    None,
                )
            }),
    }
}
