//! Typed semantic-selection inspection records.

use bray_bound_tree::{
    BoundCallableTarget, BoundReferenceTarget, CheckedSemanticSelections, ConstructionTarget,
    ConversionTarget, IndexTarget, OperatorTarget, SelectedConversion, SelectedIterationSource,
    SelectedOperation, SelectedPropagation, SelectedPropagationBoundary, SemanticSelection,
};
use bray_symbols::{
    AnyLocalSymbolId, CallableInstanceData, ImplementationInstanceId, ImplementationRequirementKey,
    LocalSymbolSnapshot, SemanticValueStore, SymbolGraph, TypeId,
};
use serde::Serialize;

use crate::inspection::{InspectionSymbolIdentity, InspectionType, TypeInspectionError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SelectionInspectionError {
    Local,
    SemanticValue,
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
    BuiltInOperator {
        operator: &'static str,
    },
    TraitOperator {
        operator: &'static str,
        member: InspectionSymbolIdentity,
        fulfillment: InspectionSymbolIdentity,
        evidence: Box<InspectionImplementationEvidence>,
    },
    TraitConstraintOperator {
        operator: &'static str,
        member: InspectionSymbolIdentity,
        constraint_owner: InspectionSymbolIdentity,
        constraint_ordinal: u32,
    },
    BuiltInIndex {
        operation: &'static str,
    },
    CustomIndex {
        capability: &'static str,
        member: InspectionSymbolIdentity,
        fulfillment: InspectionSymbolIdentity,
        evidence: Box<InspectionImplementationEvidence>,
    },
    TraitConstraintIndex {
        capability: &'static str,
        member: InspectionSymbolIdentity,
        constraint_owner: InspectionSymbolIdentity,
        constraint_ordinal: u32,
    },
    Construction {
        callable: InspectionSymbolIdentity,
        evidence: Box<InspectionImplementationEvidence>,
    },
    Conversion {
        conversion: Box<InspectionConversion>,
    },
    Implementation {
        evidence: Box<InspectionImplementationEvidence>,
    },
    Iteration {
        #[serde(flatten)]
        iteration: Box<InspectionIteration>,
    },
    Propagation {
        boundary: &'static str,
        result_type: Option<InspectionType>,
        error_conversion: Option<Box<InspectionConversion>>,
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
            Self::BuiltInOperator { operator } => format!("built-in {operator}"),
            Self::TraitOperator {
                operator,
                fulfillment,
                ..
            } => format!("trait {operator} via {}", fulfillment.text()),
            Self::TraitConstraintOperator {
                operator, member, ..
            } => format!("trait {operator} via constraint for {}", member.text()),
            Self::BuiltInIndex { operation } => format!("built-in {operation}"),
            Self::CustomIndex {
                capability,
                fulfillment,
                ..
            } => {
                format!("custom {capability} index via {}", fulfillment.text())
            }
            Self::TraitConstraintIndex {
                capability, member, ..
            } => {
                format!(
                    "custom {capability} index via constraint for {}",
                    member.text()
                )
            }
            Self::Construction { callable, .. } => {
                format!("type-form construction via {}", callable.text())
            }
            Self::Conversion { conversion } => conversion.text(),
            Self::Implementation { evidence } => evidence.text(),
            Self::Iteration { iteration } => format!(
                "{} iteration via {} then {}",
                iteration.mode,
                iteration.iterate.fulfillment.text(),
                iteration.next.fulfillment.text()
            ),
            Self::Propagation {
                boundary,
                result_type,
                ..
            } => result_type.as_ref().map_or_else(
                || format!("propagate within {boundary}"),
                |result_type| format!("propagate to {boundary} as {}", result_type.text()),
            ),
        }
    }
}

#[derive(Serialize)]
struct InspectionImplementationEvidence {
    subject: InspectionType,
    required_trait: InspectionSymbolIdentity,
    implementation: InspectionSymbolIdentity,
}

impl InspectionImplementationEvidence {
    fn text(&self) -> String {
        format!(
            "{} for {} via {}",
            self.required_trait.display_name(),
            self.subject.text(),
            self.implementation.display_name()
        )
    }
}

#[derive(Serialize)]
struct InspectionProtocolOperation {
    member: InspectionSymbolIdentity,
    fulfillment: InspectionSymbolIdentity,
    evidence: Box<InspectionImplementationEvidence>,
}

#[derive(Serialize)]
struct InspectionIteration {
    mode: &'static str,
    source_type: InspectionType,
    cursor_type: InspectionType,
    element_type: InspectionType,
    iterate: InspectionProtocolOperation,
    next: InspectionProtocolOperation,
    has_exact_count: bool,
}

#[derive(Serialize)]
struct InspectionConversion {
    source_type: InspectionType,
    target_type: InspectionType,
    rule: InspectionConversionRule,
}

impl InspectionConversion {
    fn text(&self) -> String {
        format!(
            "{} {} -> {}",
            self.rule.kind_name(),
            self.source_type.text(),
            self.target_type.text()
        )
    }
}

#[derive(Serialize)]
#[serde(tag = "rule_kind", rename_all = "snake_case")]
enum InspectionConversionRule {
    Identity,
    BuiltInScalar,
    Composite {
        components: Vec<InspectionConversion>,
    },
    Trait {
        member: InspectionSymbolIdentity,
        fulfillment: InspectionSymbolIdentity,
        evidence: Box<InspectionImplementationEvidence>,
    },
    TraitConstraint {
        member: InspectionSymbolIdentity,
        constraint_owner: InspectionSymbolIdentity,
        constraint_ordinal: u32,
    },
}

impl InspectionConversionRule {
    const fn kind_name(&self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::BuiltInScalar => "built-in scalar",
            Self::Composite { .. } => "composite",
            Self::Trait { .. } => "trait",
            Self::TraitConstraint { .. } => "trait constraint",
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
                target: selection_target(entry.selection(), locals, symbols, semantic_values)?,
            })
        })
        .collect()
}

pub(super) const fn selection_kind(selection: &SemanticSelection) -> &'static str {
    match selection {
        SemanticSelection::Reference(_) => "reference",
        SemanticSelection::Call(_) => "call",
        SemanticSelection::Predicate(_) => "predicate",
        SemanticSelection::Operation(operation) => operation.kind().as_str(),
        SemanticSelection::Iteration(_) => "iteration",
        SemanticSelection::Propagation(_) => "propagation",
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
        SemanticSelection::Call(call) => {
            callable_target(call.target(), locals, symbols, semantic_values).map(Some)
        }
        SemanticSelection::Predicate(predicate) => Ok(Some(InspectionSelectionTarget::Surface {
            symbol: InspectionSymbolIdentity::from_symbol(
                symbols,
                predicate.predicate().into_any(),
            ),
        })),
        SemanticSelection::Operation(operation) => {
            operation_target(operation, symbols, semantic_values).map(Some)
        }
        SemanticSelection::Iteration(iteration) => {
            iteration_target(iteration, symbols, semantic_values).map(Some)
        }
        SemanticSelection::Propagation(propagation) => {
            propagation_target(propagation, symbols, semantic_values).map(Some)
        }
    }
}

fn propagation_target(
    propagation: &SelectedPropagation,
    symbols: &SymbolGraph,
    semantic_values: &SemanticValueStore,
) -> Result<InspectionSelectionTarget, SelectionInspectionError> {
    let boundary = match propagation.boundary() {
        Some(SelectedPropagationBoundary::Callable) => "callable",
        Some(SelectedPropagationBoundary::YieldRegion(_)) => "yield_region",
        None => "current_run",
    };

    let result_type = propagation
        .result_type()
        .map(|ty| InspectionType::from_type(semantic_values, symbols, ty))
        .transpose()?;

    let error_conversion = propagation
        .error_conversion()
        .map(|conversion| inspection_conversion(conversion, symbols, semantic_values))
        .transpose()?
        .map(Box::new);

    Ok(InspectionSelectionTarget::Propagation {
        boundary,
        result_type,
        error_conversion,
    })
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
            symbol: InspectionSymbolIdentity::from_symbol(symbols, instance.definition().symbol()),
        }),
        BoundCallableTarget::Predicate(instance) => Ok(InspectionSelectionTarget::Surface {
            symbol: InspectionSymbolIdentity::from_symbol(
                symbols,
                instance.definition().into_any(),
            ),
        }),
        BoundCallableTarget::Anonymous(callable) => local_target(callable.into(), locals),
        BoundCallableTarget::Indirect(ty) => indirect_target(semantic_values, symbols, ty),
    }
}

fn operation_target(
    operation: &SelectedOperation,
    symbols: &SymbolGraph,
    semantic_values: &SemanticValueStore,
) -> Result<InspectionSelectionTarget, SelectionInspectionError> {
    match operation {
        SelectedOperation::Member(target) => Ok(InspectionSelectionTarget::Surface {
            symbol: InspectionSymbolIdentity::from_symbol(symbols, target.member()),
        }),
        SelectedOperation::Construction(construction) => match construction.target() {
            ConstructionTarget::Struct(symbol) => Ok(InspectionSelectionTarget::Surface {
                symbol: InspectionSymbolIdentity::from_symbol(symbols, symbol.into()),
            }),
            ConstructionTarget::UnionVariant(symbol) => Ok(InspectionSelectionTarget::Surface {
                symbol: InspectionSymbolIdentity::from_symbol(symbols, symbol.into()),
            }),
            ConstructionTarget::TypeForm {
                callable,
                requirement,
                witness,
            } => Ok(InspectionSelectionTarget::Construction {
                callable: callable_identity(symbols, callable),
                evidence: Box::new(implementation_evidence(
                    requirement,
                    witness,
                    symbols,
                    semantic_values,
                )?),
            }),
        },
        SelectedOperation::Operator { target, .. } => {
            operator_target(*target, symbols, semantic_values)
        }
        SelectedOperation::CompoundAssignment(selection) => {
            operator_target(selection.target(), symbols, semantic_values)
        }
        SelectedOperation::Index { target, .. } => index_target(*target, symbols, semantic_values),
        SelectedOperation::Conversion(conversion) => Ok(InspectionSelectionTarget::Conversion {
            conversion: Box::new(inspection_conversion(conversion, symbols, semantic_values)?),
        }),
        SelectedOperation::Implementation(witness) => {
            Ok(InspectionSelectionTarget::Implementation {
                evidence: Box::new(implementation_evidence(
                    witness.requirement(),
                    witness.witness(),
                    symbols,
                    semantic_values,
                )?),
            })
        }
    }
}

fn operator_target(
    target: OperatorTarget,
    symbols: &SymbolGraph,
    semantic_values: &SemanticValueStore,
) -> Result<InspectionSelectionTarget, SelectionInspectionError> {
    match target {
        OperatorTarget::BuiltIn(operator) => Ok(InspectionSelectionTarget::BuiltInOperator {
            operator: operator.as_str(),
        }),
        OperatorTarget::Trait {
            operator,
            member,
            fulfillment,
            requirement,
            witness,
        } => Ok(InspectionSelectionTarget::TraitOperator {
            operator: operator.as_str(),
            member: callable_identity(symbols, member),
            fulfillment: callable_identity(symbols, fulfillment),
            evidence: Box::new(implementation_evidence(
                requirement,
                witness,
                symbols,
                semantic_values,
            )?),
        }),
        OperatorTarget::TraitConstraint {
            operator,
            member,
            dispatch,
            ..
        } => Ok(InspectionSelectionTarget::TraitConstraintOperator {
            operator: operator.as_str(),
            member: callable_identity(symbols, member),
            constraint_owner: InspectionSymbolIdentity::from_symbol(
                symbols,
                dispatch.owner().symbol(),
            ),
            constraint_ordinal: dispatch.ordinal().raw(),
        }),
    }
}

fn index_target(
    target: IndexTarget,
    symbols: &SymbolGraph,
    semantic_values: &SemanticValueStore,
) -> Result<InspectionSelectionTarget, SelectionInspectionError> {
    let operation = match target {
        IndexTarget::ArrayElement => "array_element",
        IndexTarget::SliceElement => "slice_element",
        IndexTarget::ArraySlice => "array_slice",
        IndexTarget::Slice => "slice",
        IndexTarget::Custom {
            borrow_kind,
            member,
            fulfillment,
            requirement,
            witness,
        } => {
            return Ok(InspectionSelectionTarget::CustomIndex {
                capability: borrow_kind.as_str(),
                member: callable_identity(symbols, member),
                fulfillment: callable_identity(symbols, fulfillment),
                evidence: Box::new(implementation_evidence(
                    requirement,
                    witness,
                    symbols,
                    semantic_values,
                )?),
            });
        }
        IndexTarget::TraitConstraint {
            borrow_kind,
            member,
            dispatch,
            ..
        } => {
            return Ok(InspectionSelectionTarget::TraitConstraintIndex {
                capability: borrow_kind.as_str(),
                member: callable_identity(symbols, member),
                constraint_owner: InspectionSymbolIdentity::from_symbol(
                    symbols,
                    dispatch.owner().symbol(),
                ),
                constraint_ordinal: dispatch.ordinal().raw(),
            });
        }
    };

    Ok(InspectionSelectionTarget::BuiltInIndex { operation })
}

fn inspection_conversion(
    conversion: &SelectedConversion,
    symbols: &SymbolGraph,
    semantic_values: &SemanticValueStore,
) -> Result<InspectionConversion, SelectionInspectionError> {
    let rule = match conversion.target() {
        ConversionTarget::Identity => InspectionConversionRule::Identity,
        ConversionTarget::BuiltInScalar => InspectionConversionRule::BuiltInScalar,
        ConversionTarget::Composite(components) => InspectionConversionRule::Composite {
            components: components
                .iter()
                .map(|component| inspection_conversion(component, symbols, semantic_values))
                .collect::<Result<_, _>>()?,
        },
        ConversionTarget::Trait {
            member,
            fulfillment,
            requirement,
            witness,
        } => InspectionConversionRule::Trait {
            member: callable_identity(symbols, *member),
            fulfillment: callable_identity(symbols, *fulfillment),
            evidence: Box::new(implementation_evidence(
                *requirement,
                *witness,
                symbols,
                semantic_values,
            )?),
        },
        ConversionTarget::TraitConstraint {
            member, dispatch, ..
        } => InspectionConversionRule::TraitConstraint {
            member: callable_identity(symbols, *member),
            constraint_owner: InspectionSymbolIdentity::from_symbol(
                symbols,
                dispatch.owner().symbol(),
            ),
            constraint_ordinal: dispatch.ordinal().raw(),
        },
    };

    Ok(InspectionConversion {
        source_type: InspectionType::from_type(semantic_values, symbols, conversion.source_type())?,
        target_type: InspectionType::from_type(semantic_values, symbols, conversion.target_type())?,
        rule,
    })
}

fn iteration_target(
    iteration: &SelectedIterationSource,
    symbols: &SymbolGraph,
    semantic_values: &SemanticValueStore,
) -> Result<InspectionSelectionTarget, SelectionInspectionError> {
    Ok(InspectionSelectionTarget::Iteration {
        iteration: Box::new(InspectionIteration {
            mode: iteration.mode().as_str(),
            source_type: InspectionType::from_type(
                semantic_values,
                symbols,
                iteration.source_type(),
            )?,
            cursor_type: InspectionType::from_type(
                semantic_values,
                symbols,
                iteration.cursor_type(),
            )?,
            element_type: InspectionType::from_type(
                semantic_values,
                symbols,
                iteration.element_type(),
            )?,
            iterate: protocol_operation(
                iteration.iterate_member(),
                iteration.iterate(),
                iteration.iterable_requirement(),
                iteration.iterable_witness(),
                symbols,
                semantic_values,
            )?,
            next: protocol_operation(
                iteration.next_member(),
                iteration.next(),
                iteration.iterator_requirement(),
                iteration.iterator_witness(),
                symbols,
                semantic_values,
            )?,
            has_exact_count: iteration.exact_count().is_some(),
        }),
    })
}

fn protocol_operation(
    member: CallableInstanceData,
    fulfillment: CallableInstanceData,
    requirement: ImplementationRequirementKey,
    witness: ImplementationInstanceId,
    symbols: &SymbolGraph,
    semantic_values: &SemanticValueStore,
) -> Result<InspectionProtocolOperation, SelectionInspectionError> {
    Ok(InspectionProtocolOperation {
        member: callable_identity(symbols, member),
        fulfillment: callable_identity(symbols, fulfillment),
        evidence: Box::new(implementation_evidence(
            requirement,
            witness,
            symbols,
            semantic_values,
        )?),
    })
}

fn implementation_evidence(
    requirement: ImplementationRequirementKey,
    witness: ImplementationInstanceId,
    symbols: &SymbolGraph,
    semantic_values: &SemanticValueStore,
) -> Result<InspectionImplementationEvidence, SelectionInspectionError> {
    let trait_application = semantic_values
        .trait_application_data(requirement.trait_application())
        .map_err(|_| SelectionInspectionError::SemanticValue)?;

    let implementation = semantic_values
        .implementation_instance_data(witness)
        .map_err(|_| SelectionInspectionError::SemanticValue)?;

    Ok(InspectionImplementationEvidence {
        subject: InspectionType::from_type(semantic_values, symbols, requirement.subject())?,
        required_trait: InspectionSymbolIdentity::from_symbol(
            symbols,
            trait_application.definition().into(),
        ),
        implementation: InspectionSymbolIdentity::from_symbol(
            symbols,
            implementation.definition().into_any(),
        ),
    })
}

fn callable_identity(
    symbols: &SymbolGraph,
    callable: CallableInstanceData,
) -> InspectionSymbolIdentity {
    InspectionSymbolIdentity::from_symbol(symbols, callable.definition().symbol())
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
    let (id, name) = local_identity(target, locals).ok_or(SelectionInspectionError::Local)?;

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
        AnyLocalSymbolId::Binding(target) => locals
            .binding(target)
            .map(|symbol| (target.ordinal(), Some(symbol.name().as_str().to_owned()))),
        AnyLocalSymbolId::Constant(target) => locals
            .constant(target)
            .map(|symbol| (target.ordinal(), Some(symbol.name().as_str().to_owned()))),
        AnyLocalSymbolId::AnonymousCallable(target) => locals
            .anonymous_callable(target)
            .map(|_| (target.ordinal(), None)),
        AnyLocalSymbolId::AnonymousCallableParameter(target) => locals
            .anonymous_parameter(target)
            .map(|symbol| (target.ordinal(), Some(symbol.name().as_str().to_owned()))),
        AnyLocalSymbolId::PostconditionResult(target) => locals
            .postcondition_result(target)
            .map(|_| (target.ordinal(), None)),
    }
}
