use std::collections::{BTreeMap, BTreeSet};

use crate::{
    CodegenCallableMapping, CodegenCallableTarget, CodegenConstantMapping,
    CodegenConstantTermMapping, CodegenHelperMapping, CodegenInstanceKey, CodegenOperationMapping,
    CodegenSymbolKey, CodegenSymbolMapping, CodegenTerminatorMapping, CodegenTypeBehavior,
    CodegenTypeKind, CodegenTypeMapping, CodegenUnit, TargetAddressSpaceKind,
};

use super::super::demand::{child_constants, demanded_constant_terms, demanded_constants};
use super::callable_demand::demanded_callable_references;
use super::core::CodegenMappingsBuildError;

pub(super) fn compare_constant_terms(
    left: &CodegenConstantTermMapping,
    right: &CodegenConstantTermMapping,
) -> std::cmp::Ordering {
    left.owner()
        .cmp(right.owner())
        .then_with(|| left.term().cmp(&right.term()))
}

pub(super) fn compare_callables(
    left: &CodegenCallableMapping,
    right: &CodegenCallableMapping,
) -> std::cmp::Ordering {
    left.owner()
        .cmp(right.owner())
        .then_with(|| left.site().cmp(&right.site()))
}

pub(super) fn compare_operations(
    left: &CodegenOperationMapping,
    right: &CodegenOperationMapping,
) -> std::cmp::Ordering {
    left.owner()
        .cmp(right.owner())
        .then_with(|| left.operation().cmp(&right.operation()))
}

pub(super) fn compare_terminators(
    left: &CodegenTerminatorMapping,
    right: &CodegenTerminatorMapping,
) -> std::cmp::Ordering {
    left.owner()
        .cmp(right.owner())
        .then_with(|| left.block().cmp(&right.block()))
}

pub(super) fn validate_callable_mappings(
    unit: &CodegenUnit,
    instances: &BTreeSet<&CodegenInstanceKey>,
    symbols: &[CodegenSymbolMapping],
    mappings: &[CodegenCallableMapping],
) -> Result<(), CodegenMappingsBuildError> {
    let expected = demanded_callable_references(unit);

    let actual: BTreeSet<_> = mappings
        .iter()
        .map(|mapping| (mapping.owner().clone(), mapping.site(), mapping.reference()))
        .collect();

    if actual != expected
        || mappings.iter().any(|mapping| {
            if !instances.contains(mapping.owner()) {
                return true;
            }

            match mapping.target() {
                CodegenCallableTarget::Instance(instance) => {
                    if !instances.contains(instance) {
                        return true;
                    }

                    let key = CodegenSymbolKey::Instance(instance.clone());

                    symbols
                        .binary_search_by(|symbol| symbol.key().cmp(&key))
                        .ok()
                        .is_none_or(|index| {
                            symbols[index].signature().abi() != mapping.reference().abi()
                        })
                }
                CodegenCallableTarget::Intrinsic(_) => {
                    mapping.reference().abi() != bray_symbols::CallableAbi::Bray
                }
            }
        })
    {
        return Err(CodegenMappingsBuildError::CallableCoverageMismatch);
    }

    Ok(())
}

pub(super) fn validate_operation_mappings(
    unit: &CodegenUnit,
    symbols: &[CodegenSymbolMapping],
    mappings: &[CodegenOperationMapping],
) -> Result<(), CodegenMappingsBuildError> {
    let expected: BTreeMap<_, _> = unit
        .instances()
        .iter()
        .flat_map(|instance| {
            instance
                .mir()
                .operations_with_ids()
                .map(move |(id, operation)| {
                    (
                        (instance.key().clone(), id),
                        (
                            operation.kind().helper_references(),
                            operation.kind().requires_outgoing_capacity(),
                        ),
                    )
                })
        })
        .filter(|(_, (helpers, outgoing))| !helpers.is_empty() || *outgoing)
        .collect();

    if mappings.len() != expected.len()
        || mappings.iter().any(|mapping| {
            let key = (mapping.owner().clone(), mapping.operation());

            expected.get(&key).is_none_or(|(references, outgoing)| {
                *outgoing != mapping.outgoing_capacity().is_some()
                    || references.len() != mapping.helpers().len()
                    || references
                        .iter()
                        .zip(mapping.helpers())
                        .any(|(reference, helper)| reference != helper.reference())
            }) || mapping
                .helpers()
                .iter()
                .any(|helper| !valid_helper(symbols, helper))
        })
    {
        return Err(CodegenMappingsBuildError::OperationCoverageMismatch);
    }

    Ok(())
}

pub(super) fn validate_terminator_mappings(
    unit: &CodegenUnit,
    mappings: &[CodegenTerminatorMapping],
) -> Result<(), CodegenMappingsBuildError> {
    let expected: BTreeMap<_, _> = unit
        .instances()
        .iter()
        .flat_map(|instance| {
            instance
                .mir()
                .blocks_with_ids()
                .filter_map(move |(id, block)| {
                    block
                        .terminator()
                        .kind()
                        .requires_pattern_value_mapping()
                        .then_some(((instance.key().clone(), id), 1_usize))
                })
        })
        .collect();

    if mappings.len() != expected.len()
        || mappings.iter().any(|mapping| {
            let key = (mapping.owner().clone(), mapping.block());

            expected
                .get(&key)
                .is_none_or(|constants| *constants != mapping.constants().len())
        })
    {
        return Err(CodegenMappingsBuildError::TerminatorCoverageMismatch);
    }

    Ok(())
}

pub(super) fn validate_constant_mappings(
    unit: &CodegenUnit,
    types: &[CodegenTypeMapping],
    mappings: &[CodegenConstantMapping],
    terms: &[CodegenConstantTermMapping],
    terminators: &[CodegenTerminatorMapping],
    static_storages: &[crate::CodegenStaticStorageMapping],
) -> Result<(), CodegenMappingsBuildError> {
    if mappings
        .iter()
        .any(|mapping| !valid_constant_representation(mapping, types))
    {
        return Err(CodegenMappingsBuildError::InvalidConstantRepresentation);
    }

    let demands = demanded_constants(unit);
    let mut expected_values = demands.values().clone();

    if demands.types().iter().any(|(value, types)| {
        types.iter().any(|ty| {
            mappings
                .binary_search_by_key(&(*value, *ty), |mapping| {
                    (mapping.value(), mapping.representation())
                })
                .is_err()
        })
    }) {
        return Err(CodegenMappingsBuildError::ConstantCoverageMismatch);
    }

    expected_values.extend(
        terminators
            .iter()
            .flat_map(CodegenTerminatorMapping::constants),
    );

    let expected_terms: BTreeSet<_> = unit
        .instances()
        .iter()
        .flat_map(|instance| {
            demanded_constant_terms(instance.mir())
                .into_iter()
                .map(move |term| (instance.key().clone(), term))
        })
        .collect();

    let actual_terms: BTreeSet<_> = terms
        .iter()
        .map(|mapping| (mapping.owner().clone(), mapping.term()))
        .collect();

    if expected_terms != actual_terms {
        return Err(CodegenMappingsBuildError::ConstantCoverageMismatch);
    }

    expected_values.extend(terms.iter().map(CodegenConstantTermMapping::value));

    expected_values.extend(
        static_storages
            .iter()
            .map(crate::CodegenStaticStorageMapping::initial_value),
    );

    let actual_values: BTreeSet<_> = mappings.iter().map(CodegenConstantMapping::value).collect();

    for mapping in mappings {
        expected_values.extend(child_constants(mapping.data().kind()));
    }

    if expected_values != actual_values {
        return Err(CodegenMappingsBuildError::ConstantCoverageMismatch);
    }

    Ok(())
}

pub(super) fn valid_constant_representation(
    constant: &CodegenConstantMapping,
    types: &[CodegenTypeMapping],
) -> bool {
    if constant.semantic_type() == constant.representation() {
        return true;
    }

    let semantic = types
        .binary_search_by_key(&constant.semantic_type(), CodegenTypeMapping::ty)
        .ok()
        .and_then(|index| types.get(index));

    let representation = types
        .binary_search_by_key(&constant.representation(), CodegenTypeMapping::ty)
        .ok()
        .and_then(|index| types.get(index));

    semantic.is_some_and(|mapping| {
        mapping.behavior() == Some(CodegenTypeBehavior::String)
            && matches!(mapping.kind(), CodegenTypeKind::Aggregate(_))
    }) && representation.is_some_and(|mapping| {
        matches!(
            mapping.kind(),
            CodegenTypeKind::Pointer {
                target,
                address_space: TargetAddressSpaceKind::Default,
            } if *target == constant.semantic_type()
        )
    })
}

fn valid_helper(symbols: &[CodegenSymbolMapping], helper: &CodegenHelperMapping) -> bool {
    let Some(key) = helper.symbol() else {
        return true;
    };

    symbols
        .binary_search_by(|symbol| symbol.key().cmp(key))
        .ok()
        .is_some_and(|index| symbols[index].signature().abi() == helper.reference().abi())
}
