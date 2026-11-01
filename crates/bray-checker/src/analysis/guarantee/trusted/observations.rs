use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::BoundExpressionId;

use super::super::flow::ExecutionFlowDomain;
use super::super::state::ExecutionState;
use crate::{CheckerRequestContext, ExecutionCondition};

/// Ordinary borrowed storage bounds the value, while an address bounds only access clauses.
pub(super) enum GuaranteeSource {
    Borrowed(BTreeSet<crate::ExecutionPlace>),
    Address {
        value: ExecutionCondition,
        owners: BTreeSet<crate::ExecutionPlace>,
    },
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
    pub(super) fn prepare_call_observations(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        call: &bray_bound_tree::SelectedCall,
        arguments: &mut BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
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
                    .get(&expression.into())
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

            let reference = crate::representation::reference_kind(self.request, ty);

            if reference.is_none() {
                continue;
            }

            if reference == Some(bray_symbols::DependencyReferenceKind::Address) {
                let ordinal = ordinal
                    .checked_add(u32::from(call.receiver().is_some()))
                    .expect("selected argument order must fit");

                let ordinal =
                    crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(ordinal));

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

                if let Some(value) = arguments.get(&ordinal) {
                    owners.insert(
                        operand,
                        GuaranteeSource::Address {
                            value: value.clone(),
                            owners: state.address_owners(value),
                        },
                    );
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

            let ordinal = ordinal
                .checked_add(u32::from(call.receiver().is_some()))
                .expect("selected argument order must fit");

            arguments.insert(
                crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(ordinal)),
                value,
            );
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

        let call = invocation.expression().and_then(|expression| {
            match self.semantics.selections().expression(expression) {
                Some(bray_bound_tree::SemanticSelection::Call(call)) => Some((expression, call)),
                _ => None,
            }
        });

        let dependencies = self.dependencies.calls().get(&invocation);

        for (input, source) in owners {
            let owners = match source {
                GuaranteeSource::Borrowed(owners) | GuaranteeSource::Address { owners, .. } => {
                    owners
                }
            };

            if owners.is_empty() {
                continue;
            }

            let (Some((expression, call)), Some(dependencies)) = (call, dependencies) else {
                retained.extend(owners.iter().cloned());
                continue;
            };

            if let GuaranteeSource::Address { value, .. } = source {
                for (_, subject) in address_subjects
                    .iter()
                    .filter(|(condition, _)| condition == guarantee)
                {
                    let address = instantiate(subject);
                    let same_address = address.owner_identity() == value.owner_identity();

                    let retained_input = dependencies
                        .retained_inputs()
                        .is_none_or(|inputs| inputs.contains(input));

                    let independent = dependencies.references().iter().any(|reference| {
                        reference.relation().entry().is_none()
                            && reference.relation().kind()
                                == bray_symbols::DependencyReferenceKind::Address
                            && subject
                                == &super::carriers::result_projection(
                                    self.dependencies,
                                    reference.relation().exit().projections(),
                                )
                    });

                    if same_address
                        || (subject.observes_completion_result() && retained_input && !independent)
                    {
                        addresses
                            .entry(address)
                            .or_insert_with(BTreeSet::new)
                            .extend(owners.iter().cloned());
                    }
                }

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
