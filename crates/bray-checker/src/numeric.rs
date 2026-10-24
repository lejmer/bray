use bray_compiler_known::ImplementationHook;
use bray_symbols::{
    CallablePhaseBehaviors, CallableSignature, ConstantBinaryOperation, ConstantTermData,
    ConstantTermId, ConstantValueData, ConstantValueKind, NumericTruncation, PredicateDomain,
    TypeData,
};

use crate::{CheckerQueryError, CheckerRequestContext};

/// Instantiates the fixed numerical meaning of ordinary truncation requirements.
/// This publishes value conditions before callers perform their ordinary contract analysis.
pub fn normalize_numeric_requirements<C: CheckerRequestContext + ?Sized>(
    request: &C,
    signature: CallableSignature,
) -> Result<CallableSignature, CheckerQueryError<C::UpstreamError>> {
    let values = request.semantic_values();
    let data = values.type_data(signature.callable_type());

    let TypeData::Callable(callable) = data.as_ref() else {
        panic!("checked numeric contract retains its callable type");
    };

    let Some(phases) = normalize_numeric_phase_requirements(request, callable.phase_behaviors())?
    else {
        return Ok(signature);
    };

    let ty = values
        .intern_type(TypeData::Callable(
            callable.clone().with_phase_behaviors(phases),
        ))
        .expect("checked numeric contract type belongs to its semantic store");

    let callable_type = signature.callable_type();

    signature.try_map_types(|original| {
        Ok(if original == callable_type {
            ty
        } else {
            original
        })
    })
}

/// Normalizes ordinary numeric requirements when constructing a callable contract.
/// Returns no replacement when the established phase behaviors are unchanged.
pub fn normalize_numeric_phase_requirements<C: CheckerRequestContext + ?Sized>(
    request: &C,
    phases: &CallablePhaseBehaviors,
) -> Result<Option<CallablePhaseBehaviors>, CheckerQueryError<C::UpstreamError>> {
    let values = request.semantic_values();
    let original = phases.invocation().predicate_requirements();

    let mut rewritten = None;

    for (index, requirement) in original.iter().enumerate() {
        let condition = match (requirement.domain(), requirement.condition()) {
            (PredicateDomain::Value, Some(term)) => {
                Some(normalize_domain_condition(request, term)?)
            }
            _ => requirement.condition(),
        };

        let satisfied = condition.is_some_and(|term| matches!(values.constant_term_data(term).as_ref(),
            ConstantTermData::Value(value) if matches!(values.constant_value_data(*value).kind(), ConstantValueKind::Boolean(true))))
            && requirement.domain() == PredicateDomain::Value;

        if satisfied || condition != requirement.condition() {
            rewritten.get_or_insert_with(|| original[..index].to_vec());
        }

        if let Some(requirements) = &mut rewritten
            && !satisfied
        {
            requirements.push(requirement.with_condition(condition, requirement.domain()));
        }
    }

    let Some(requirements) = rewritten else {
        return Ok(None);
    };

    let guarantees = phases
        .deferred_execution()
        .unwrap_or_else(|| phases.invocation())
        .predicate_guarantees()
        .iter()
        .copied();

    // Callable types and phases are immutable, Arc-backed values. Retain unchanged contracts.
    Ok(Some(
        phases.clone().with_predicates(requirements, guarantees),
    ))
}

fn normalize_domain_condition<C: CheckerRequestContext + ?Sized>(
    request: &C,
    original: ConstantTermId,
) -> Result<ConstantTermId, CheckerQueryError<C::UpstreamError>> {
    let values = request.semantic_values();
    let mut term = original;

    loop {
        let data = values.constant_term_data(term);

        match data.as_ref() {
            ConstantTermData::Typed { term: inner, .. } => term = *inner,
            ConstantTermData::Call {
                callable,
                arguments,
                ..
            } => {
                let callable = values.callable_instance_data(*callable);
                let hook = request.implementation_hook(callable.definition().symbol())?;

                if !hook.is_some_and(|hook| {
                    hook.is_available()
                        && hook.hook() == ImplementationHook::NumericTruncationDomain
                }) {
                    return Ok(original);
                }

                let Some(plan) = NumericTruncation::for_instance(
                    values,
                    request.available_compiler_known_symbols(),
                    *callable,
                    request
                        .selected_target()
                        .machine()
                        .pointer_width_bits()
                        .get(),
                ) else {
                    // Open generic contracts retain their original query until instantiation.
                    return Ok(original);
                };

                let [argument] = arguments.as_ref() else {
                    panic!("domain query retains one source argument");
                };

                let mut argument = *argument;

                while let ConstantTermData::Typed { term, .. } =
                    values.constant_term_data(argument).as_ref()
                {
                    argument = *term;
                }

                let boolean = crate::representation::representation_type_for_context(
                    request,
                    bray_compiler_known::RepresentationRole::ScalarBool,
                );

                let intern = |data| {
                    values
                        .intern_constant_term(data)
                        .expect("derived numerical condition belongs to its semantic store")
                };

                let NumericTruncation::RealToInteger {
                    minimum,
                    minimum_inclusive,
                    maximum,
                    ..
                } = plan
                else {
                    return Ok(intern(ConstantTermData::Value(
                        values
                            .intern_constant_value(ConstantValueData::new(
                                boolean,
                                ConstantValueKind::Boolean(true),
                            ))
                            .expect("derived Boolean belongs to its semantic store"),
                    )));
                };

                let substitution = values.generic_substitution_data(callable.substitution());

                let bray_symbols::GenericArgument::Type(source) =
                    substitution.bindings()[1].argument()
                else {
                    unreachable!("recognized domain query source argument is a type");
                };

                let bound = |bits| {
                    intern(ConstantTermData::Value(
                        values
                            .intern_constant_value(ConstantValueData::new(
                                source,
                                ConstantValueKind::Real(bits),
                            ))
                            .expect("derived real bound belongs to its semantic store"),
                    ))
                };

                let lower = intern(ConstantTermData::Binary {
                    operation: if minimum_inclusive {
                        ConstantBinaryOperation::GreaterOrEqual
                    } else {
                        ConstantBinaryOperation::Greater
                    },
                    left: argument,
                    right: bound(minimum),
                });

                let upper = intern(ConstantTermData::Binary {
                    operation: ConstantBinaryOperation::Less,
                    left: argument,
                    right: bound(maximum),
                });

                return Ok(intern(ConstantTermData::Binary {
                    operation: ConstantBinaryOperation::LogicalAnd,
                    left: lower,
                    right: upper,
                }));
            }
            _ => return Ok(original),
        }
    }
}
