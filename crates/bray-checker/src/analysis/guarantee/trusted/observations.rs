use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::BoundExpressionId;

use super::super::flow::ExecutionFlowDomain;
use super::super::state::ExecutionState;
use crate::{CheckerRequestContext, ExecutionCondition};

/// Ordinary borrowed storage bounds the value, while an address bounds only access clauses.
pub(super) enum GuaranteeSource {
    Borrowed(BTreeSet<crate::ExecutionPlace>),
    Addresses(crate::execution_guarantees::AddressDependencies),
}

impl GuaranteeSource {
    fn borrowed_owners(&mut self) -> &mut BTreeSet<crate::ExecutionPlace> {
        let Self::Borrowed(owners) = self else {
            unreachable!("one selected input has one reference kind");
        };

        owners
    }
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    /// Select the evaluated input surface for direct calls and parameter defaults.
    pub(super) fn observation_call(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) -> Option<(BoundExpressionId, &bray_bound_tree::SelectedCall)> {
        let expression = invocation.expression()?;
        let selection = self.semantics.selections().expression(expression)?;

        let bray_bound_tree::SemanticSelection::Call(call) = selection else {
            return None;
        };

        let visible = match invocation {
            bray_bound_tree::BoundExecutionSite::Node(_) => true,
            bray_bound_tree::BoundExecutionSite::CallInput { ordinal, .. } => selection.input(ordinal)
                .is_some_and(|input| matches!(input.default_provider(), Some(bray_symbols::DefaultValueProvider::CallableParameter(_)))),
            _ => false,
        };

        visible.then_some((expression, call))
    }

    pub(super) fn prepare_call_observations(
        &self,
        state: &mut ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
        expression: BoundExpressionId,
        call: &bray_bound_tree::SelectedCall,
        arguments: &mut BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
        address_sources: &BTreeMap<
            crate::ExecutionPlace,
            crate::execution_guarantees::AddressDependencies,
        >,
        origins: &BTreeMap<
            bray_bound_tree::BoundExecutionSite,
            BTreeSet<bray_bound_tree::StoragePath>,
        >,
        retain_carriers: bool,
    ) -> (
        Vec<ExecutionCondition>,
        BTreeMap<bray_bound_tree::BoundExecutionSite, GuaranteeSource>,
    ) {
        let mut carriers = Vec::new();
        let mut owners = BTreeMap::new();

        // Whole-owner guarantees follow the returned value's dependency contract.
        // Address and field guarantees also depend on the borrowed storage that
        // established them, even when the raw value has no ordinary borrow.
        let returned = |operand| {
            retain_carriers
                && self
                    .dependencies
                    .calls()
                    .get(&invocation)
                    .and_then(|dependencies| dependencies.retained_inputs())
                    .is_none_or(|inputs| inputs.contains(&operand))
        };

        for argument in call.arguments() {
            let (parameter, ordinal, ty) = match argument {
                bray_bound_tree::SelectedArgument::Explicit {
                    parameter,
                    ordinal,
                    conversion,
                    ..
                } => (*parameter, *ordinal, conversion.target_type()),
                bray_bound_tree::SelectedArgument::Default {
                    parameter,
                    ordinal,
                    ty,
                    ..
                } => (Some(*parameter), *ordinal, *ty),
            };

            let operand = call
                .input_site(
                    expression,
                    bray_symbols::DependencySubjectRoot::Parameter(
                        bray_symbols::SymbolOrdinal::new(ordinal),
                    ),
                )
                .expect("selected argument has its evaluated input");

            let ordinal = crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(
                ordinal
                    .checked_add(u32::from(call.receiver().is_some()))
                    .expect("selected argument order fits"),
            ));

            // A parameter default sees only the earlier evaluated inputs.
            if !arguments.contains_key(&ordinal) {
                continue;
            }

            let reference = crate::representation::reference_kind(self.request, ty);

            if reference != Some(bray_symbols::DependencyReferenceKind::Borrow)
                && let Some(addresses) = address_sources.get(&ordinal)
            {
                owners.insert(operand, GuaranteeSource::Addresses(addresses.clone()));
            }

            if reference.is_none() {
                continue;
            }

            if reference == Some(bray_symbols::DependencyReferenceKind::Address) {
                if let Some(current) = self
                    .input_place(operand)
                    .and_then(|place| place.value_in(&state.current))
                    && current.owner_identity() != &ExecutionCondition::Unknown
                    && arguments
                        .get(&ordinal)
                        .is_some_and(|entry| entry.owner_identity() == current.owner_identity())
                {
                    // Completion clauses observe current contents of the evaluated address.
                    // A reassigned or exposed pointer slot cannot establish this identity.
                    if let Some(parameter) = parameter {
                        arguments.insert(
                            bray_bound_tree::BoundReferenceTarget::Surface(parameter.into()).into(),
                            current.clone(),
                        );
                    }

                    arguments.insert(ordinal.clone(), current);
                }

                continue;
            }

            let Some(place) = self.input_place(operand) else {
                // Indexed storage has no scalar field path. Its enclosing owner still bounds
                // the lifetime of a returned address or capability.
                for access in self
                    .storage
                    .occurrence_plans(operand)
                    .filter(|_| returned(operand))
                {
                    if let Some(owner) =
                        crate::ExecutionPlace::storage_owner(self.storage, access.access())
                    {
                        owners
                            .entry(operand)
                            .or_insert_with(|| GuaranteeSource::Borrowed(BTreeSet::new()))
                            .borrowed_owners()
                            .insert(owner);
                    }
                }

                continue;
            };

            let input = self.current_input(state, operand);

            // A local borrow alias retains the referent, rather than the slot holding
            // the alias. Its returned view can outlive that slot while the input lives.
            if returned(operand) {
                self.retain_trusted_borrow_owners(
                    owners
                        .entry(operand)
                        .or_insert_with(|| GuaranteeSource::Borrowed(BTreeSet::new()))
                        .borrowed_owners(),
                    operand,
                    &place,
                    origins,
                );
            }

            let value = input;

            if retain_carriers {
                carriers.push(value.clone());
            }

            if let Some(parameter) = parameter {
                arguments.insert(
                    bray_bound_tree::BoundReferenceTarget::Surface(parameter.into()).into(),
                    value.clone(),
                );
            }

            arguments.insert(ordinal, value);
        }

        if let Some(receiver) = call.receiver()
            && matches!(
                receiver.mode(),
                bray_symbols::ReceiverMode::Consuming
                    | bray_symbols::ReceiverMode::ConsumingMutable
            )
            && let Some(addresses) = address_sources.get(&crate::ExecutionPlace::argument(
                bray_symbols::SymbolOrdinal::new(0),
            ))
            && let Some(input) =
                call.input_site(expression, bray_symbols::DependencySubjectRoot::Receiver)
        {
            owners.insert(input, GuaranteeSource::Addresses(addresses.clone()));
        }

        if let Some(receiver) = call.receiver()
            && matches!(
                receiver.mode(),
                bray_symbols::ReceiverMode::Shared | bray_symbols::ReceiverMode::Mutable
            )
            && let Some(input) =
                call.input_site(expression, bray_symbols::DependencySubjectRoot::Receiver)
            && let Some(place) = self.input_place(input)
        {
            let reference =
                bray_bound_tree::BoundReferenceTarget::Surface(receiver.parameter().into());

            let value = self.current_input(state, input);

            if returned(input) {
                self.retain_trusted_borrow_owners(
                    owners
                        .entry(input)
                        .or_insert_with(|| GuaranteeSource::Borrowed(BTreeSet::new()))
                        .borrowed_owners(),
                    input,
                    &place,
                    origins,
                );
            }

            if retain_carriers {
                carriers.push(value.clone());
            }

            arguments.insert(reference.into(), value.clone());

            arguments.insert(
                crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0)),
                value,
            );
        }

        (carriers, owners)
    }

    fn retain_trusted_borrow_owners(
        &self,
        owners: &mut BTreeSet<crate::ExecutionPlace>,
        expression: bray_bound_tree::BoundExecutionSite,
        place: &crate::ExecutionPlace,
        origins: &BTreeMap<
            bray_bound_tree::BoundExecutionSite,
            BTreeSet<bray_bound_tree::StoragePath>,
        >,
    ) {
        if let Some(paths) = origins.get(&expression) {
            owners.extend(
                paths
                    .iter()
                    .filter_map(|path| crate::ExecutionPlace::storage_path(self.storage, path)),
            );
        } else {
            // Provisional execution analysis has no completed storage transition yet.
            owners.insert(place.clone());
        }
    }

    /// Classifies actual value lifetimes separately from raw-address access.
    pub(super) fn result_guarantee_owners(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
        guarantee: &ExecutionCondition,
        owners: &BTreeMap<bray_bound_tree::BoundExecutionSite, GuaranteeSource>,
        address_subjects: &[(ExecutionCondition, ExecutionCondition)],
        instantiate: &impl Fn(&ExecutionCondition) -> ExecutionCondition,
    ) -> (
        BTreeSet<crate::ExecutionPlace>,
        crate::execution_guarantees::AddressDependencies,
    ) {
        let mut retained = BTreeSet::new();
        let mut addresses = BTreeMap::new();

        let call = self.observation_call(invocation);

        let dependencies = self.dependencies.calls().get(&invocation);

        for (input, source) in owners {
            let (Some((expression, call)), Some(dependencies)) = (call, dependencies) else {
                match source {
                    GuaranteeSource::Borrowed(owners) => retained.extend(owners.iter().cloned()),
                    GuaranteeSource::Addresses(addresses) => {
                        retained.extend(addresses.values().flatten().cloned())
                    }
                }

                continue;
            };

            if let GuaranteeSource::Addresses(sources) = source {
                let subjects = address_subjects
                    .iter()
                    .filter(|(condition, _)| condition == guarantee)
                    .map(|(_, subject)| subject.clone())
                    .chain(
                        dependencies
                            .references()
                            .iter()
                            .filter(|reference| {
                                reference.relation().kind()
                                    == bray_symbols::DependencyReferenceKind::Address
                                    && reference.relation().exit().subject_root()
                                        == bray_symbols::DependencySubjectRoot::Result
                            })
                            .map(|reference| {
                                super::carriers::result_projection(
                                    self.dependencies,
                                    reference.relation().exit().projections(),
                                )
                            })
                            .filter(|subject| guarantee.observes(subject)),
                    )
                    .collect::<BTreeSet<_>>();

                for subject in subjects {
                    let address = instantiate(&subject);

                    let retained_input = dependencies
                        .retained_inputs()
                        .is_none_or(|inputs| inputs.contains(input));

                    let independent = dependencies.references().iter().any(|reference| {
                        reference.relation().entry().is_none()
                            && reference.relation().is_exact()
                            && reference.relation().kind()
                                == bray_symbols::DependencyReferenceKind::Address
                            && subject
                                == super::carriers::result_projection(
                                    self.dependencies,
                                    reference.relation().exit().projections(),
                                )
                    });

                    for (source, owners) in sources {
                        if address.owner_identity() == source.owner_identity()
                            || (subject.observes_completion_result()
                                && retained_input
                                && !independent)
                        {
                            addresses
                                .entry(address.clone())
                                .or_insert_with(BTreeSet::new)
                                .extend(owners.iter().cloned());
                        }
                    }
                }

                continue;
            }

            let GuaranteeSource::Borrowed(owners) = source else {
                unreachable!("address sources were handled above");
            };

            if owners.is_empty() {
                continue;
            }

            let mut references = dependencies
                .references()
                .iter()
                .filter(|reference| {
                    let relation = reference.relation();

                    relation.exit().subject_root() == bray_symbols::DependencySubjectRoot::Result
                        && relation
                            .entry()
                            .and_then(|entry| call.input_site(expression, entry.subject_root()))
                            == Some(*input)
                })
                .peekable();

            if dependencies.retained_inputs().is_none()
                || references.peek().is_none()
                || references.clone().any(|reference| {
                    reference.relation().kind() != bray_symbols::DependencyReferenceKind::Address
                })
            {
                retained.extend(owners.iter().cloned());
                continue;
            }

            for reference in references {
                let address = super::carriers::result_projection(
                    self.dependencies,
                    reference.relation().exit().projections(),
                );

                if guarantee.observes(&address) {
                    addresses
                        .entry(instantiate(&address))
                        .or_insert_with(BTreeSet::new)
                        .extend(owners.iter().cloned());
                }
            }
        }

        (retained, addresses)
    }
}
