use std::collections::{BTreeMap, BTreeSet};

use bray_symbols::{
    ConstantTermData, ConstantTermId, PredicateInstanceData, SemanticValueStore,
    SemanticValueStoreError, TypeData,
};

use crate::contract::{MAX_CONDITION_STEPS, instantiate_condition_with_budget};

/// Available meaning for an exactly selected predicate application.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PredicateConditionResolution {
    /// Selection or definition checking failed, so the application supplies no evidence.
    Unavailable,
    /// A checked opaque relation retains the selected application as its logical identity.
    Opaque(PredicateInstanceData),
    /// A checked transparent definition supplies symbolic meaning before substitution.
    Defined {
        /// The selected declaration and its generic arguments.
        predicate: PredicateInstanceData,
        /// The definition's receiver-first symbolic condition.
        condition: ConstantTermId,
    },
}

enum ExpansionStep {
    Visit(ConstantTermId),
    Finish(ConstantTermId),
    Alias {
        source: ConstantTermId,
        replacement: ConstantTermId,
    },
}

/// Expands transparent checked predicate definitions for bounded condition reasoning.
///
/// The resolver selects an application and supplies its checked definition meaning or opaque
/// identity. An unavailable definition supplies no evidence. Opaque applications remain distinct atoms. Cycles
/// and exhausted work return no evidence. This never evaluates an ordinary callable body.
/// Borrow annotations describe observation access, not address-valued constants. Numeric types
/// and conversions remain part of the resulting terms. Callers must retain observation validity.
pub fn expand_predicate_condition<E>(
    values: &SemanticValueStore,
    condition: ConstantTermId,
    mut resolve: impl FnMut(PredicateInstanceData) -> Result<PredicateConditionResolution, E>,
) -> Result<Option<ConstantTermId>, E>
where
    E: From<SemanticValueStoreError>,
{
    let mut pending = vec![ExpansionStep::Visit(condition)];
    let mut active = BTreeSet::new();
    let mut mapped = BTreeMap::new();
    let mut remaining = MAX_CONDITION_STEPS;

    while let Some(step) = pending.pop() {
        let Some(next) = remaining.checked_sub(1) else {
            return Ok(None);
        };

        remaining = next;

        match step {
            ExpansionStep::Visit(term) => {
                if mapped.contains_key(&term) {
                    continue;
                }

                if !active.insert(term) {
                    return Ok(None);
                }

                let data = values.constant_term_data(term)?;

                pending.push(ExpansionStep::Finish(term));

                if data
                    .try_map_terms(|child| {
                        if pending.len() >= remaining {
                            return Err(());
                        }

                        pending.push(ExpansionStep::Visit(child));

                        Ok(child)
                    })
                    .is_err()
                {
                    return Ok(None);
                }
            }
            ExpansionStep::Finish(term) => {
                let data = values.constant_term_data(term)?;

                let Ok(mut rewritten) =
                    data.try_map_terms(|child| mapped.get(&child).copied().ok_or(()))
                else {
                    return Ok(None);
                };

                if let ConstantTermData::PredicateCall {
                    predicate,
                    arguments,
                } = &mut rewritten
                {
                    let definition = match resolve(*predicate)? {
                        PredicateConditionResolution::Unavailable => return Ok(None),
                        PredicateConditionResolution::Opaque(selected) => {
                            *predicate = selected;

                            None
                        }
                        PredicateConditionResolution::Defined {
                            predicate: selected,
                            condition,
                        } => {
                            *predicate = selected;

                            Some(condition)
                        }
                    };

                    if let Some(definition) = definition {
                        let definition = values
                            .substitute_constant_term(definition, predicate.substitution())?;

                        let Some(replacement) = instantiate_condition_with_budget(
                            values,
                            definition,
                            arguments,
                            &mut remaining,
                        )?
                        else {
                            return Ok(None);
                        };

                        pending.push(ExpansionStep::Alias {
                            source: term,
                            replacement,
                        });

                        pending.push(ExpansionStep::Visit(replacement));

                        continue;
                    }
                }

                let replacement = match rewritten {
                    ConstantTermData::Typed { term, ty }
                        if matches!(&*values.type_data(ty)?, TypeData::Borrow { .. }) =>
                    {
                        term
                    }
                    rewritten => values.intern_constant_term(rewritten)?,
                };

                active.remove(&term);
                mapped.insert(term, replacement);
            }
            ExpansionStep::Alias {
                source,
                replacement,
            } => {
                let Some(replacement) = mapped.get(&replacement).copied() else {
                    return Ok(None);
                };

                active.remove(&source);
                mapped.insert(source, replacement);
            }
        }
    }

    Ok(mapped.get(&condition).copied())
}

#[cfg(test)]
mod tests {
    use bray_symbols::{
        ConstantTermData, GenericOwnerId, GenericSubstitutionData, PredicateDefinitionSymbolId,
        PredicateInstanceData, PredicateSymbolId, SemanticValueStore, SemanticValueStoreError,
        SymbolId, SymbolOrdinal,
    };

    use super::{PredicateConditionResolution, expand_predicate_condition};

    fn predicate(values: &SemanticValueStore) -> PredicateInstanceData {
        let symbol = PredicateSymbolId::from_symbol_id(SymbolId::new(0));
        let owner = GenericOwnerId::try_new(symbol.into()).unwrap();

        let substitution = values
            .intern_generic_substitution(GenericSubstitutionData::try_new(owner, [], []).unwrap())
            .unwrap();

        PredicateInstanceData::new(PredicateDefinitionSymbolId::Predicate(symbol), substitution)
    }

    #[test]
    fn predicate_expansion_preserves_opaque_atoms_and_instantiates_transparent_arguments() {
        let values = SemanticValueStore::try_new().unwrap();

        let formal = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let actual = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(1)))
            .unwrap();

        let predicate = predicate(&values);

        let call = values
            .intern_constant_term(ConstantTermData::predicate_call(predicate, [actual]))
            .unwrap();

        assert_eq!(
            expand_predicate_condition::<SemanticValueStoreError>(&values, call, |predicate| Ok(
                PredicateConditionResolution::Opaque(predicate)
            )),
            Ok(Some(call))
        );

        assert_eq!(
            expand_predicate_condition::<SemanticValueStoreError>(&values, call, |predicate| Ok(
                PredicateConditionResolution::Defined {
                    predicate,
                    condition: formal
                }
            )),
            Ok(Some(actual))
        );
    }

    #[test]
    fn unavailable_predicates_supply_no_evidence_and_opaque_selection_retains_identity() {
        let values = SemanticValueStore::try_new().unwrap();
        let original = predicate(&values);
        let symbol = PredicateSymbolId::from_symbol_id(SymbolId::new(1));
        let owner = GenericOwnerId::try_new(symbol.into()).unwrap();

        let substitution = values
            .intern_generic_substitution(GenericSubstitutionData::try_new(owner, [], []).unwrap())
            .unwrap();

        let selected = PredicateInstanceData::new(
            PredicateDefinitionSymbolId::Predicate(symbol),
            substitution,
        );

        let call = values
            .intern_constant_term(ConstantTermData::predicate_call(original, []))
            .unwrap();

        let expected = values
            .intern_constant_term(ConstantTermData::predicate_call(selected, []))
            .unwrap();

        assert_eq!(
            expand_predicate_condition::<SemanticValueStoreError>(&values, call, |_| Ok(
                PredicateConditionResolution::Unavailable
            )),
            Ok(None)
        );

        assert_eq!(
            expand_predicate_condition::<SemanticValueStoreError>(&values, call, |_| Ok(
                PredicateConditionResolution::Opaque(selected)
            )),
            Ok(Some(expected))
        );
    }

    #[test]
    fn recursive_predicate_meaning_does_not_manufacture_evidence() {
        let values = SemanticValueStore::try_new().unwrap();

        let formal = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let call = values
            .intern_constant_term(ConstantTermData::predicate_call(
                predicate(&values),
                [formal],
            ))
            .unwrap();

        assert_eq!(
            expand_predicate_condition::<SemanticValueStoreError>(&values, call, |predicate| Ok(
                PredicateConditionResolution::Defined {
                    predicate,
                    condition: call
                }
            )),
            Ok(None)
        );
    }
}
