use std::collections::{BTreeMap, BTreeSet};

use super::super::flow::ExecutionFlowDomain;
use super::observations::GuaranteeSource;
use crate::{CheckerRequestContext, ExecutionCondition};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    /// Classifies actual value lifetimes separately from raw-address access.
    pub(super) fn result_guarantee_owners(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
        entry: &BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
        guarantee: &ExecutionCondition,
        owning: bool,
        owners: &BTreeMap<bray_bound_tree::BoundExecutionSite, GuaranteeSource>,
        address_subjects: &[(ExecutionCondition, ExecutionCondition, bool)],
        instantiate: &impl Fn(&ExecutionCondition) -> ExecutionCondition,
    ) -> (
        BTreeSet<crate::ExecutionPlace>,
        crate::execution_guarantees::AddressDependencies,
    ) {
        // An owning carrier can also carry a raw access promise. That promise still
        // expires with every possible physical input region of the address.
        let owning = owning
            && !address_subjects
                .iter()
                .any(|(condition, _, owning)| condition == guarantee && !owning);

        let arguments = entry;
        let mut retained = BTreeSet::new();

        let mut addresses = self.input_guarantee_addresses(
            invocation,
            entry,
            guarantee,
            owners,
            address_subjects,
            instantiate,
        );

        let call = self.observation_call(invocation);

        let dependencies = self.dependencies.calls().get(&invocation);

        for (input, source) in owners {
            let (Some((expression, call)), Some(dependencies)) = (call, dependencies) else {
                retained.extend(source.borrowed.iter().cloned());
                retained.extend(source.addresses.values().flatten().cloned());

                continue;
            };

            if source.borrowed.is_empty() {
                let sources = &source.addresses;

                let subjects = address_subjects
                    .iter()
                    .filter(|(condition, _, _)| condition == guarantee)
                    .map(|(_, subject, _)| subject.clone())
                    .chain(
                        dependencies
                            .references()
                            .iter()
                            .filter(|reference| {
                                !(owning && reference.relation().provenance() == bray_symbols::DependencyReferenceProvenance::InputRegions)
                                    && reference.relation().kind()
                                    == bray_symbols::DependencyReferenceKind::Address
                                    && reference.relation().exit().subject_root()
                                        == bray_symbols::DependencySubjectRoot::Result
                            })
                            .map(|reference| {
                                super::carriers::result_projection(
                                    self.request.semantic_values(), self.dependencies,
                                    reference.relation().exit().projections(),
                                )
                            })
                            .filter(|subject| guarantee.observes_trusted(subject)),
                    )
                    .collect::<BTreeSet<_>>();

                for subject in subjects {
                    let address = address_value(instantiate(&subject));

                    let retained_input = dependencies
                        .retained_inputs()
                        .is_none_or(|inputs| inputs.contains(input))
                        || !owning
                            && dependencies.references().iter().any(|reference| {
                                reference.relation().exit().subject_root()
                                    == bray_symbols::DependencySubjectRoot::Result
                                    && reference.relation().entry().and_then(|entry| {
                                        call.input_site(expression, entry.subject_root())
                                    }) == Some(*input)
                            });

                    let mut origins = dependencies
                        .references()
                        .iter()
                        .filter(|reference| {
                            reference.relation().kind()
                                == bray_symbols::DependencyReferenceKind::Address
                                && reference.relation().exit().subject_root()
                                    == bray_symbols::DependencySubjectRoot::Result
                                && subject
                                    == super::carriers::result_projection(
                                        self.request.semantic_values(),
                                        self.dependencies,
                                        reference.relation().exit().projections(),
                                    )
                        })
                        .map(|reference| reference.relation())
                        .peekable();

                    let independent = origins.peek().is_some()
                        && origins.all(|relation| relation.entry().is_none());

                    if let Some(owners) = sources.get(&address) {
                        addresses
                            .entry(address)
                            .or_insert_with(BTreeSet::new)
                            .extend(owners.iter().cloned());
                    } else if subject.observes_completion_result() && retained_input && !independent
                    {
                        self.retain_result_address_sources(
                            &mut addresses,
                            &address,
                            &subject,
                            *input,
                            sources,
                            expression,
                            call,
                            dependencies,
                            entry,
                        );
                    }
                }

                continue;
            }

            let owners = &source.borrowed;

            if owners.is_empty() {
                continue;
            }

            let mut references = dependencies
                .references()
                .iter()
                .filter(|reference| {
                    let relation = reference.relation();

                    !(owning
                        && relation.provenance()
                            == bray_symbols::DependencyReferenceProvenance::InputRegions)
                        && relation.exit().subject_root()
                            == bray_symbols::DependencySubjectRoot::Result
                        && relation
                            .entry()
                            .and_then(|entry| call.input_site(expression, entry.subject_root()))
                            == Some(*input)
                })
                .peekable();

            if dependencies
                .retained_inputs()
                .is_some_and(|inputs| !inputs.contains(input))
                && (owning || references.peek().is_none())
            {
                continue;
            }

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
                    self.request.semantic_values(),
                    self.dependencies,
                    reference.relation().exit().projections(),
                );

                if guarantee.observes_trusted(&address) {
                    let entry = reference
                        .relation()
                        .entry()
                        .expect("selected returned borrow retains its input");

                    let input = entry_subject(
                        arguments,
                        &reference_subject(
                            self.request.semantic_values(),
                            self.dependencies,
                            call,
                            entry,
                        ),
                    );

                    let owners = source.addresses.get(&address_value(input)).unwrap_or(owners);

                    addresses
                        .entry(address_value(instantiate(&address)))
                        .or_insert_with(BTreeSet::new)
                        .extend(owners.iter().cloned());
                }
            }
        }

        (retained, addresses)
    }
    fn retain_result_address_sources(
        &self,
        addresses: &mut crate::execution_guarantees::AddressDependencies,
        address: &ExecutionCondition,
        subject: &ExecutionCondition,
        input: bray_bound_tree::BoundExecutionSite,
        sources: &crate::execution_guarantees::AddressDependencies,
        expression: bray_bound_tree::BoundExpressionId,
        call: &bray_bound_tree::SelectedCall,
        dependencies: &bray_bound_tree::BoundCallDependencyContracts,
        arguments: &BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    ) {
        let mut known = false;
        let mut unknown = false;

        for reference in dependencies.references().iter().filter(|reference| {
            reference.relation().kind() == bray_symbols::DependencyReferenceKind::Address
                && reference.relation().exit().subject_root()
                    == bray_symbols::DependencySubjectRoot::Result
                && subject
                    == &super::carriers::result_projection(
                        self.request.semantic_values(),
                        self.dependencies,
                        reference.relation().exit().projections(),
                    )
        }) {
            let Some(entry) = reference.relation().entry() else {
                continue;
            };

            known = true;

            if call.input_site(expression, entry.subject_root()) != Some(input) {
                continue;
            }

            let source = reference_subject(
                self.request.semantic_values(),
                self.dependencies,
                call,
                entry,
            );

            let source = address_value(entry_subject(arguments, &source));

            if let Some(owners) = sources.get(&source) {
                addresses
                    .entry(address.clone())
                    .or_default()
                    .extend(owners.iter().cloned());
            } else if source == ExecutionCondition::unknown() || entry.projections().is_empty() {
                unknown = true;
            }
        }

        if !known || unknown {
            addresses
                .entry(address.clone())
                .or_default()
                .extend(sources.values().flatten().cloned());
        }
    }
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    /// Input access promises are bounded by the supplied storage, without tying unrelated clauses to it.
    pub(super) fn input_guarantee_addresses(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
        entry: &BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
        guarantee: &ExecutionCondition,
        sources: &BTreeMap<bray_bound_tree::BoundExecutionSite, GuaranteeSource>,
        subjects: &[(ExecutionCondition, ExecutionCondition, bool)],
        instantiate: &impl Fn(&ExecutionCondition) -> ExecutionCondition,
    ) -> crate::execution_guarantees::AddressDependencies {
        let mut addresses = BTreeMap::new();

        for (_, subject, _) in subjects.iter().filter(|(condition, subject, _)| {
            condition == guarantee
                && !subject.observes_completion_result()
                && subject.inputs().iter().any(|input| {
                    matches!(
                        input.root,
                        crate::execution_guarantees::ExecutionInput::Argument(_)
                    )
                })
        }) {
            let address = address_value(instantiate(subject));

            let owners = addresses
                .entry(address.clone())
                .or_insert_with(BTreeSet::new);

            // An existing address identifies its storage across raw-value inputs. Other
            // raw addresses may describe values written there, rather than that storage.
            let known_address = sources
                .values()
                .any(|source| source.addresses.contains_key(&address));

            let origins = self.input_address_origins(invocation, subject);

            for (input, source) in sources {
                if let Some(origins) = &origins {
                    let Some(paths) = origins.get(input) else {
                        continue;
                    };

                    for path in paths {
                        let value = address_value(entry_subject(entry, path));

                        if let Some(storage) = source.addresses.get(&value) {
                            owners.extend(storage.iter().cloned());
                        } else {
                            owners.extend(source.borrowed.iter().cloned());

                            if value == ExecutionCondition::unknown()
                                || path
                                    .inputs()
                                    .iter()
                                    .any(|input| input.projections.is_empty())
                            {
                                owners.extend(source.addresses.values().flatten().cloned());
                            }
                        }
                    }
                } else {
                    owners.extend(source.borrowed.iter().cloned());

                    if let Some(storage) = source.addresses.get(&address) {
                        owners.extend(storage.iter().cloned());
                    } else if !known_address {
                        owners.extend(source.addresses.values().flatten().cloned());
                    }
                }
            }
        }

        addresses
    }

    /// Copyable input access facts can describe an address without observing its borrowed carrier.
    pub(super) fn carry_input_access_guarantees(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
        state: &mut super::super::state::ExecutionState,
        subjects: &[(ExecutionCondition, ExecutionCondition, bool)],
        sources: &BTreeMap<bray_bound_tree::BoundExecutionSite, GuaranteeSource>,
        instantiate: &impl Fn(&ExecutionCondition) -> ExecutionCondition,
    ) {
        for (template, subject, _) in subjects.iter().filter(|(_, subject, owning)| {
            !*owning
                && !subject.observes_completion_result()
                && subject.inputs().iter().any(|input| {
                    matches!(
                        input.root,
                        crate::execution_guarantees::ExecutionInput::Argument(_)
                    )
                })
        }) {
            let address = address_value(instantiate(subject));

            if address == ExecutionCondition::unknown() {
                continue;
            }

            let guarantee = instantiate(template);

            let addresses = self.input_guarantee_addresses(
                invocation,
                &state
                    .entries
                    .get(&invocation)
                    .expect("completed invocation retains its captured arguments")
                    .arguments,
                template,
                sources,
                subjects,
                instantiate,
            );

            let carrier = state
                .witnesses
                .entry(address.owner_identity().clone())
                .or_default();

            for clause in guarantee
                .conjuncts()
                .filter(|clause| clause.observes_trusted(&address))
            {
                carrier.carry(clause, false, &addresses);
            }
        }
    }

    /// Resolve a mutated input address through the checked entry/exit relationship.
    fn input_address_origins(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
        subject: &ExecutionCondition,
    ) -> Option<BTreeMap<bray_bound_tree::BoundExecutionSite, BTreeSet<ExecutionCondition>>> {
        let (expression, call) = self.observation_call(invocation)?;

        let dependencies = self.dependencies.calls().get(&invocation)?;
        let mut origins = BTreeMap::<_, BTreeSet<_>>::new();

        for reference in dependencies.references().iter().filter(|reference| {
            let relation = reference.relation();

            relation.kind() == bray_symbols::DependencyReferenceKind::Address
                && reference_subject(
                    self.request.semantic_values(),
                    self.dependencies,
                    call,
                    relation.exit(),
                ) == *subject
        }) {
            let entry = reference.relation().entry()?;

            origins
                .entry(call.input_site(expression, entry.subject_root())?)
                .or_default()
                .insert(reference_subject(
                    self.request.semantic_values(),
                    self.dependencies,
                    call,
                    entry,
                ));
        }

        (!origins.is_empty()).then_some(origins)
    }
}

/// Express a checked formal reference path in receiver-first argument order.
fn reference_subject(
    values: &bray_symbols::SemanticValueStore,
    dependencies: &bray_bound_tree::CheckedDependencyContracts,
    call: &bray_bound_tree::SelectedCall,
    subject: &bray_symbols::DependencySubject,
) -> ExecutionCondition {
    let root = if subject.subject_root() == bray_symbols::DependencySubjectRoot::Result {
        ExecutionCondition::result()
    } else {
        ExecutionCondition::new(crate::ExecutionConditionKind::Input(crate::ExecutionPlace::argument(
            call.input_ordinal(subject.subject_root())
                .expect("checked reference has its formal input"),
        )))
    };

    super::carriers::result_projection(values, dependencies, subject.projections()).substitute(
        &|_| ExecutionCondition::unknown(),
        &root,
        &mut { ExecutionCondition::WORK_LIMIT },
    )
}

/// Checked entry paths observe the original input, independently of final mutable contents.
fn entry_subject(
    arguments: &BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    subject: &ExecutionCondition,
) -> ExecutionCondition {
    subject.substitute(
        &|input| {
            input
                .value_in(arguments)
                .unwrap_or_else(ExecutionCondition::unknown)
        },
        &ExecutionCondition::unknown(),
        &mut { ExecutionCondition::WORK_LIMIT },
    )
}

/// Classified raw-address subjects can be reached through an input loan. The
/// predicate observes the address stored there, independently of that loan.
fn address_value(value: ExecutionCondition) -> ExecutionCondition {
    value.into_referent().descriptor_identity().clone()
}
