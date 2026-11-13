use bray_bound_tree::{BoundExpressionId, BoundReferenceTarget};
use bray_diagnostics::{DiagnosticCondition as Condition, DiagnosticConditionProjection as Projection, DiagnosticTypeArgument};
use bray_symbols::{AnyLocalSymbolId, AnySymbolId, ConstantProjectionKind, ConstantValueKind, GenericArgument};

use crate::analysis::guarantee::flow::ExecutionFlowDomain;
use crate::execution_guarantees::ExecutionInput;
use crate::{CheckerQueryResult, CheckerRequestContext, ExecutionCondition, ExecutionPlace};

type Outcome<C> = CheckerQueryResult<Condition, <C as CheckerRequestContext>::UpstreamError>;

pub(super) fn witness_transfer<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>, kind: bray_diagnostics::DiagnosticWitnessTransferKind,
    carrier: &ExecutionCondition, guarantee: &ExecutionCondition,
) -> CheckerQueryResult<bray_diagnostics::DiagnosticWitnessTransfer, C::UpstreamError> {
    Ok(bray_diagnostics::DiagnosticWitnessTransfer {
        kind,
        carrier: condition(domain, carrier, &mut 128)?,
        guarantee: condition(domain, guarantee, &mut 128)?,
    })
}

pub(super) fn panic_requirement<C: CheckerRequestContext + ?Sized>(
    flow: &super::flow::ExecutionFlow<'_, '_, C>, node: bray_bound_tree::AnyBoundNodeId,
) -> CheckerQueryResult<Option<bray_diagnostics::DiagnosticContractObligation>, C::UpstreamError> {
    for edge in flow.domain.graph.edges() {
        let Some(crate::analysis::model::AnalysisRefinement::PreconditionFailure(invocation)) = edge.refinement() else {
            continue;
        };
        if invocation.node() != node || flow.edge_output(edge.id()).is_none() {
            continue;
        }
        let Some(contract) = flow.domain.request.trusted_contracts().and_then(|contracts| contracts.calls.get(&invocation)) else {
            continue;
        };
        let state = flow.output(edge.source()).expect("reachable entry failure has a source state");
        let entry = state.entries.get(&invocation).expect("entry failure retains its captured input values before retirement");

        if let Err(failed) = entry.prove_requirements(contract.preconditions.iter().map(|(condition, _)| condition)) {
            return obligation(&flow.domain, bray_diagnostics::DiagnosticContractObligationKind::CallRequirement, &failed).map(Some);
        }
    }

    Ok(None)
}

pub(super) fn obligation<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>, kind: bray_diagnostics::DiagnosticContractObligationKind,
    failed: &crate::UnprovenExecutionCondition,
) -> CheckerQueryResult<bray_diagnostics::DiagnosticContractObligation, C::UpstreamError> {
    Ok(bray_diagnostics::DiagnosticContractObligation {
        kind,
        required: condition(domain, failed.required(), &mut 128)?,
        actual: condition(domain, failed.actual(), &mut 128)?,
    })
}

fn condition<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>,
    value: &ExecutionCondition,
    budget: &mut usize,
) -> Outcome<C> {
    let Some(remaining) = budget.checked_sub(1) else { return Ok(Condition::Elided); };
    *budget = remaining;

    Ok(match value {
        ExecutionCondition::Boolean(value) => Condition::Boolean(*value),
        ExecutionCondition::Literal(value) => literal(domain, value.kind(), budget)?,
        ExecutionCondition::Input(value)
            | ExecutionCondition::Joined(_, _, value) => place(domain, value, budget)?,
        ExecutionCondition::Result => Condition::Result,
        ExecutionCondition::Expression(value) | ExecutionCondition::ScopedCapability(value) => expression(domain, *value, budget)?,
        ExecutionCondition::CapturedInput { owner, invocation, ordinal } =>
            captured_input(domain, owner, *invocation, *ordinal, budget)?,
        ExecutionCondition::Present(value) => Condition::Present(Box::new(condition(domain, value, budget)?)),
        ExecutionCondition::Constructed(base, _) | ExecutionCondition::ScalarValue(base) | ExecutionCondition::PostState(_, base)
            | ExecutionCondition::Trusted(base) | ExecutionCondition::Borrowed(base)
            | ExecutionCondition::Entry { condition: base, .. }
            | ExecutionCondition::InitializedSnapshot(_, base, _) => condition(domain, base, budget)?,
        ExecutionCondition::Observation { owner, .. } => condition(domain, owner, budget)?,
        ExecutionCondition::Predicate(predicate, substitution, arguments) => {
            application(domain, predicate.into_any(), *substitution, arguments, budget)?
        }
        ExecutionCondition::Call(callable, _, _, arguments, _) => {
            application(domain, callable.definition().symbol(), callable.substitution(), arguments, budget)?
        }
        ExecutionCondition::Operation(operation, operands) => Condition::Operation {
            operation: crate::constant::diagnostic_operation(*operation),
            operands: operands.iter().map(|value| condition(domain, value, budget)).collect::<Result<_, _>>()?,
        },
        ExecutionCondition::Projection(projection, value) => Condition::Projection {
            value: Box::new(condition(domain, value, budget)?),
            projection: projection_value(domain, *projection, budget)?,
        },
        ExecutionCondition::Variant(variant) => Condition::Source(symbol_path(domain, (*variant).into())?.join(".")),
        ExecutionCondition::Tagged(variant, value) => Condition::Application {
            path: symbol_path(domain, (*variant).into())?, types: Vec::new(),
            arguments: vec![condition(domain, value, budget)?],
        },
        ExecutionCondition::VariantTest(variant, value) => Condition::VariantTest {
            value: Box::new(condition(domain, value, budget)?),
            variant: symbol_path(domain, (*variant).into())?.join("."),
        },
        ExecutionCondition::Unknown => Condition::Unknown,
        ExecutionCondition::Discriminant(value) | ExecutionCondition::Descriptor(_, value) => condition(domain, value, budget)?,
        ExecutionCondition::ValueAlias(_, _) => panic!("internal data alias reached source diagnostic rendering: {value:?}"),
    })
}

fn captured_input<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>, owner: &ExecutionCondition,
    invocation: bray_bound_tree::BoundExecutionSite, ordinal: bray_symbols::SymbolOrdinal, budget: &mut usize,
) -> Outcome<C> {
    let source = invocation.expression()
        .and_then(|expression| domain.semantics.selections().expression(expression))
        .and_then(|selection| selection.input(ordinal))
        .and_then(|input| input.expression());

    match source {
        Some(source) => expression(domain, source, budget),
        None => Ok(Condition::CapturedInput { owner: Box::new(condition(domain, owner, budget)?), ordinal: ordinal.raw() }),
    }
}

fn application<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>, symbol: AnySymbolId,
    substitution: bray_symbols::GenericSubstitutionId, arguments: &[ExecutionCondition], budget: &mut usize,
) -> Outcome<C> {
    let selected = domain.request.semantic_values().generic_substitution_data(substitution);
    let types = selected.bindings().iter().map(|binding| match binding.argument() {
        GenericArgument::Type(ty) => crate::diagnostic::diagnostic_type(domain.request.context(), ty).map(DiagnosticTypeArgument::Type),
        GenericArgument::Constant(_) => Ok(DiagnosticTypeArgument::Constant),
    }).collect::<Result<_, _>>()?;

    Ok(Condition::Application {
        path: symbol_path(domain, symbol)?, types,
        arguments: arguments.iter().map(|value| condition(domain, value, budget)).collect::<Result<_, _>>()?,
    })
}

fn symbol_path<C: CheckerRequestContext + ?Sized>(domain: &ExecutionFlowDomain<'_, '_, C>, symbol: AnySymbolId) -> CheckerQueryResult<Vec<String>, C::UpstreamError> {
    let context = domain.request.context();
    let name = context.member_name(symbol)?.expect("checked named condition symbol retains its name");
    let key = context.symbol_key(symbol)?.expect("checked named condition symbol retains its key");
    Ok(crate::diagnostic::diagnostic_symbol_path(key, name.as_str()))
}

fn expression<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>, value: BoundExpressionId, budget: &mut usize,
) -> Outcome<C> {
    if let Some(bray_bound_tree::BoundExpression::Name(name)) = domain.request.view().expression(value) {
        return place(domain, &ExecutionPlace::from(name.target()), budget);
    }

    if let Some(selection @ bray_bound_tree::SemanticSelection::Call(call)) = domain.semantics.selections().expression(value)
        && let bray_bound_tree::BoundCallableTarget::Declaration(callable) = call.target()
    {
        let arguments = selection.inputs().filter_map(|input| input.expression())
            .map(ExecutionCondition::Expression).collect::<Vec<_>>();

        return application(domain, callable.definition().symbol(), callable.substitution(), &arguments, budget);
    }

    Ok(Condition::Source(crate::diagnostic::expression_text(domain.request, value)))
}

fn place<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>, value: &ExecutionPlace, budget: &mut usize,
) -> Outcome<C> {
    let mut result = match &value.root {
        ExecutionInput::Reference(BoundReferenceTarget::Surface(AnySymbolId::ReceiverParameter(_))) => Condition::Source("self".to_owned()),
        ExecutionInput::Reference(BoundReferenceTarget::Surface(symbol)) => {
            let context = domain.request.context();
            let name = context.member_name(*symbol)?;

            match (symbol, name) {
                (AnySymbolId::CallableParameter(parameter), None) => {
                    let parameter = context.callable_parameter(*parameter)?
                        .expect("checked unnamed input retains its source/imported parameter record");
                    Condition::Argument(parameter.ordinal())
                }
                (_, None) => panic!("checked condition input {value:?} has no source/imported name"),
                (symbol, Some(name)) => match symbol {
                AnySymbolId::CallableParameter(_) | AnySymbolId::GenericConstParameter(_) => Condition::Source(name.as_str().to_owned()),
                AnySymbolId::StructField(_) | AnySymbolId::UnionPayloadField(_) => Condition::Source(format!("self.{}", name.as_str())),
                _ => Condition::Source(symbol_path(domain, *symbol)?.join(".")),
                },
            }
        }
        ExecutionInput::Reference(BoundReferenceTarget::Local(local)) => {
            let locals = domain.request.unit().local_symbols();
            match local {
                AnyLocalSymbolId::Binding(id) => Condition::Source(locals.binding(*id).expect("condition binding is installed").name().as_str().to_owned()),
                AnyLocalSymbolId::AnonymousCallableParameter(id) => Condition::Source(locals.anonymous_parameter(*id).expect("condition parameter is installed").name().as_str().to_owned()),
                AnyLocalSymbolId::Constant(id) => Condition::Source(locals.constant(*id).expect("condition constant is installed").name().as_str().to_owned()),
                AnyLocalSymbolId::PostconditionResult(_) => Condition::Result,
                AnyLocalSymbolId::AnonymousCallable(_) => {
                    let anchor = locals.syntax_anchor(*local).expect("condition callable has its source anchor");
                    Condition::Source(domain.request.source_syntax(anchor).text().trim().to_owned())
                }
            }
        }
        ExecutionInput::Argument(ordinal) => Condition::Argument(ordinal.raw()),
        ExecutionInput::Constant(term) => term_value(domain, *term, budget)?,
        ExecutionInput::Static(selection) => Condition::Source(symbol_path(domain, selection.template().declaration().into())?.join(".")),
        ExecutionInput::Storage(storage) => {
            let identity = domain.storage.identity(*storage).expect("condition storage is planned");
            if let Some(rebound) = ExecutionPlace::storage_identity(domain.storage, *storage).filter(|other| other.root != value.root) {
                place(domain, &rebound, budget)?
            } else if let Some(producer) = identity.definition_occurrence() {
                occurrence(domain, producer, budget)?
            } else {
                Condition::Unknown
            }
        }
    };

    for projection in value.projections.iter().copied() {
        result = Condition::Projection { value: Box::new(result), projection: projection_value(domain, projection, budget)? };
    }

    Ok(result)
}

fn occurrence<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>, site: bray_bound_tree::BoundExecutionSite, budget: &mut usize,
) -> Outcome<C> {
    let source = match site {
        bray_bound_tree::BoundExecutionSite::CallInput { expression, ordinal } => {
            let input = domain.semantics.selections().expression(expression)
                .expect("diagnostic invocation retains its checked selection")
                .input(ordinal).expect("diagnostic input retains its selected ordinal");

            match input.expression() {
                Some(source) => source,
                None => {
                    let bray_symbols::DependencySubjectRoot::Parameter(ordinal) = input.root() else {
                        panic!("a declaration-owned default belongs to a formal parameter");
                    };
                    return Ok(Condition::Argument(ordinal.raw()));
                }
            }
        }
        _ => match site.expression() {
            Some(source) => source,
            None => return Ok(Condition::Unknown),
        },
    };

    expression(domain, source, budget)
}

fn projection_value<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>, value: ConstantProjectionKind, budget: &mut usize,
) -> CheckerQueryResult<Projection, C::UpstreamError> {
    Ok(match value {
        ConstantProjectionKind::ProductField(field) => Projection::Field(domain.request.context().member_name(field.into())?.expect("checked field has a name").as_str().to_owned()),
        ConstantProjectionKind::UnionPayloadField(field) => Projection::Field(domain.request.context().member_name(field.into())?.expect("checked union field has a name").as_str().to_owned()),
        ConstantProjectionKind::TupleElement(index) => Projection::TupleElement(index.raw()),
        ConstantProjectionKind::ArrayElementOrdinal(index) => Projection::ArrayElement(index.raw()),
        ConstantProjectionKind::ArrayElement(index) => Projection::Index(Box::new(term_value(domain, index, budget)?)),
        ConstantProjectionKind::ArraySlice { lower, upper } => Projection::Slice {
            lower: lower.map(|term| term_value(domain, term, budget).map(Box::new)).transpose()?,
            upper: upper.map(|term| term_value(domain, term, budget).map(Box::new)).transpose()?,
        },
        ConstantProjectionKind::NullableValue => Projection::NullableValue,
    })
}

fn term_value<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>, term: bray_symbols::ConstantTermId, budget: &mut usize,
) -> Outcome<C> {
    let inputs = domain.request.trusted_contracts().map_or(&[][..], |contracts| &contracts.inputs);
    let value = crate::execution_condition_from_term(domain.request.semantic_values(), term, inputs)
        .expect("diagnostic constant term belongs to the checked semantic store");
    condition(domain, &value, budget)
}

fn literal<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>, value: &ConstantValueKind, budget: &mut usize,
) -> Outcome<C> {
    let Some(remaining) = budget.checked_sub(1) else { return Ok(Condition::Elided); };
    *budget = remaining;

    Ok(match value {
        ConstantValueKind::Unit => Condition::Literal("()".to_owned()),
        ConstantValueKind::StaticAddress(selection) => Condition::StaticAddress(symbol_path(domain, selection.template().declaration().into())?),
        ConstantValueKind::Borrowed(value) => {
            let value = domain.request.semantic_values().constant_value_data(*value);
            literal(domain, value.kind(), budget)?
        }
        ConstantValueKind::Boolean(value) => Condition::Boolean(*value),
        ConstantValueKind::Integer(value) => Condition::Literal(crate::constant::to_big_integer(value).to_string()),
        ConstantValueKind::Character(value) => Condition::Character(*value),
        ConstantValueKind::String(value) => Condition::String(value.to_string()),
        ConstantValueKind::NullableAbsent => Condition::Absent,
        ConstantValueKind::NullablePresent(value) => {
            let value = domain.request.semantic_values().constant_value_data(*value);
            Condition::Present(Box::new(literal(domain, value.kind(), budget)?))
        }
        ConstantValueKind::Error => Condition::Unknown,
        ConstantValueKind::Real(_) | ConstantValueKind::Complex { .. } | ConstantValueKind::Tuple(_)
            | ConstantValueKind::Array(_) | ConstantValueKind::Product(_) | ConstantValueKind::Union { .. } => Condition::Elided,
    })
}
