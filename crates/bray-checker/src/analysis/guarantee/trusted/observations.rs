use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::BoundExpressionId;

use super::super::flow::ExecutionFlowDomain;
use super::super::state::ExecutionState;
use crate::{CheckerRequestContext, ExecutionCondition};

/// An input can provide borrowed storage and independently bounded raw-address fields.
#[derive(Default)]
pub(super) struct GuaranteeSource {
    pub(super) borrowed: BTreeSet<crate::ExecutionPlace>,
    pub(super) addresses: crate::execution_guarantees::AddressDependencies,
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
            bray_bound_tree::BoundExecutionSite::CallInput { ordinal, .. } => {
                selection.input(ordinal).is_some_and(|input| {
                    matches!(
                        input.default_provider(),
                        Some(bray_symbols::DefaultValueProvider::CallableParameter(_))
                    )
                })
            }
            _ => false,
        };

        visible.then_some((expression, call))
    }

    fn refresh_borrowed_arguments(
        &self,
        state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
        arguments: &mut BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
        completion: &bray_bound_tree::StorageCompletion,
    ) {
        let Some(selection) = invocation
            .expression()
            .and_then(|expression| self.semantics.selections().expression(expression))
        else {
            return;
        };

        let Some(contracts) = self.dependencies.calls().get(&invocation) else {
            return;
        };

        let mut refreshed = BTreeSet::<crate::ExecutionPlace>::new();

        for subject in contracts.reference_subjects() {
            if contracts
                .reference_type(subject)
                .and_then(|ty| crate::representation::reference_kind(self.request, ty))
                != Some(bray_symbols::DependencyReferenceKind::Borrow)
            {
                continue;
            }

            let prepared = contracts
                .reference_path(subject)
                .expect("prepared reference subject retains its path");

            let Some(input) = selection.invocation_input_site(invocation, subject.subject_root())
            else {
                continue;
            };

            let observe = |base: &bray_bound_tree::StoragePath| {
                let projected;

                let path = if prepared.projections().is_empty() {
                    base
                } else {
                    projected = bray_bound_tree::StoragePath {
                        root: base.root,
                        projections: base
                            .projections
                            .iter()
                            .chain(prepared.projections())
                            .copied()
                            .collect(),
                    };

                    &projected
                };

                let target = completion.references().get(path)?;

                let target = (target).value_observation_path()?;

                if let Some(bray_bound_tree::StorageIdentity::Static(declaration)) =
                    self.storage.identity(target.root)
                {
                    // Physical storage paths group a static declaration's instances.
                    // Keep the exact selected instance from the evaluated input.
                    let input = self.current_input(state, input);

                    let (ExecutionCondition::Input(mut place), _) = input.observation_path()?
                    else {
                        return None;
                    };

                    if place.selected_static() != Some(declaration) {
                        return None;
                    }

                    place.projections = std::sync::Arc::new([]);

                    let place = place.project(&target.projections)?;

                    return Some(
                        place
                            .value_in(&state.current)
                            .unwrap_or(ExecutionCondition::Unknown),
                    );
                }

                self.current_storage_value(state, &target)
            };

            let observed = match input {
                bray_bound_tree::BoundExecutionSite::Node(
                    bray_bound_tree::AnyBoundNodeId::Expression(expression),
                ) => self
                    .storage
                    .node_plans(expression.into())
                    .filter_map(|plan| self.storage.logical_path(plan.access()))
                    .find_map(observe)
                    .or_else(|| {
                        self.storage
                            .temporary(expression)
                            .and_then(|root| observe(&bray_bound_tree::StoragePath::root(root)))
                    }),
                bray_bound_tree::BoundExecutionSite::CallInput {
                    expression,
                    ordinal,
                } => self
                    .storage
                    .invocation_input(expression, ordinal)
                    .and_then(|access| self.storage.logical_path(access))
                    .and_then(observe),
                bray_bound_tree::BoundExecutionSite::Iterate(_)
                | bray_bound_tree::BoundExecutionSite::Next(_)
                | bray_bound_tree::BoundExecutionSite::ScopedEnter(_) => self
                    .storage
                    .invocation_result(input)
                    .and_then(|access| self.storage.logical_path(access))
                    .and_then(observe),
                _ => None,
            };

            let value = observed
                .or_else(|| {
                    self.input_place(input)
                        .and_then(|place| place.project(prepared.projections()))
                        .and_then(|place| place.value_in(&state.current))
                })
                .or_else(|| {
                    prepared
                        .projections()
                        .is_empty()
                        .then(|| self.current_input(state, input))
                });

            let Some(mut value) = value else {
                continue;
            };

            let ordinal = selection
                .invocation_input_ordinal(invocation, subject.subject_root())
                .expect("prepared input reference retains its ordinal");

            let Some(place) =
                crate::ExecutionPlace::argument(ordinal).project(prepared.projections())
            else {
                continue;
            };

            // Project the refreshed evaluated argument. Its source expression
            // can expire or name a different value while the call is executing.
            // Root refresh supplies current fields without rereading that source.
            let argument = crate::ExecutionPlace::argument(ordinal).value_in(arguments)
                .unwrap_or_else(|| panic!("completed borrow input retains its evaluated root: {invocation:?}, {subject:?}"));
            let context = prepared.projections().iter().copied()
                .fold(argument, ExecutionCondition::project);

            // An unavailable source handle cannot erase a child already supplied
            // by this completion's refreshed parent snapshot.
            value = if value.is_unknown() && refreshed.iter().any(|parent| parent.contains(&place)) {
                context
            } else {
                context.with_referent(value)
            };

            if !value.contains_unknown() { refreshed.insert(place.clone()); }

            // Consuming the carrier does not freeze the contents of its borrowed fields.
            super::super::state::update_parent_observations(
                arguments,
                &place,
                &value,
                Some(prepared.projections()),
            );

            arguments.insert(place, value);

            if let Some(reference) = selection
                .inputs()
                .find(|input| input.root() == subject.subject_root())
                .and_then(|input| input.reference())
                && let Some(value) = arguments.get(&crate::ExecutionPlace::argument(ordinal))
            {
                arguments.insert(reference.into(), value.clone());
            }
        }

    }

    pub(super) fn prepare_call_observations(
        &self,
        state: &mut ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
        call: &bray_bound_tree::SelectedCall,
        arguments: &mut BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
        address_sources: &BTreeMap<
            crate::ExecutionPlace,
            crate::execution_guarantees::AddressDependencies,
        >,
        completion: &bray_bound_tree::StorageCompletion,
        retain_authority: bool,
    ) -> BTreeMap<bray_bound_tree::BoundExecutionSite, GuaranteeSource> {
        self.refresh_borrowed_arguments(state, invocation, arguments, completion);

        let expression = invocation
            .expression()
            .expect("visible selected call retains its expression");

        let mut owners = BTreeMap::<_, GuaranteeSource>::new();

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

            if let Some(addresses) = address_sources.get(&ordinal) {
                owners.entry(operand).or_default().addresses = addresses.clone();
            }

            if reference != Some(bray_symbols::DependencyReferenceKind::Borrow) {
                continue;
            }

            let place = self.input_place(operand);

            if place.is_none() {
                // Indexed storage has no scalar field path. Its enclosing owner still bounds
                // the lifetime of a returned address or capability.
                for access in self
                    .storage
                    .occurrence_plans(operand)
                    .filter(|_| retain_authority)
                {
                    if let Some(owner) = self.storage.logical_path(access.access())
                        .and_then(|path| crate::ExecutionPlace::storage_owner(self.storage, path))
                    {
                        owners.entry(operand).or_default().borrowed.insert(owner);
                    }
                }
            }

            let input = arguments
                .get(&ordinal)
                .expect("prepared call retains its refreshed argument")
                .clone();

            // A local borrow alias retains the referent, rather than the slot holding
            // the alias. Its returned view can outlive that slot while the input lives.
            if retain_authority {
                self.retain_trusted_borrow_owners(
                    &mut owners.entry(operand).or_default().borrowed,
                    operand,
                    place.as_ref(),
                    completion.borrow_origins(),
                );
            }

            let value = input;

            if let Some(parameter) = parameter {
                arguments.insert(
                    bray_bound_tree::BoundReferenceTarget::Surface(parameter.into()).into(),
                    value.clone(),
                );
            }

            arguments.insert(ordinal, value);
        }

        if let Some(addresses) = address_sources.get(&crate::ExecutionPlace::argument(
            bray_symbols::SymbolOrdinal::new(0),
        )) && let Some(input) =
            call.input_site(expression, bray_symbols::DependencySubjectRoot::Receiver)
        {
            owners.entry(input).or_default().addresses = addresses.clone();
        }

        if let Some(receiver) = call.receiver()
            && matches!(
                receiver.mode(),
                bray_symbols::ReceiverMode::Shared | bray_symbols::ReceiverMode::Mutable
            )
            && let Some(input) =
                call.input_site(expression, bray_symbols::DependencySubjectRoot::Receiver)
        {
            let reference =
                bray_bound_tree::BoundReferenceTarget::Surface(receiver.parameter().into());

            let value = arguments
                .get(&crate::ExecutionPlace::argument(
                    bray_symbols::SymbolOrdinal::new(0),
                ))
                .expect("prepared receiver retains its refreshed argument")
                .clone();

            if retain_authority {
                self.retain_trusted_borrow_owners(
                    &mut owners.entry(input).or_default().borrowed,
                    input,
                    self.input_place(input).as_ref(),
                    completion.borrow_origins(),
                );
            }

            arguments.insert(reference.into(), value.clone());

            arguments.insert(
                crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0)),
                value,
            );
        }

        owners
    }

    fn retain_trusted_borrow_owners(
        &self,
        owners: &mut BTreeSet<crate::ExecutionPlace>,
        expression: bray_bound_tree::BoundExecutionSite,
        place: Option<&crate::ExecutionPlace>,
        origins: &BTreeMap<
            bray_bound_tree::BoundExecutionSite,
            BTreeSet<bray_bound_tree::StoragePath>,
        >,
    ) {
        if let Some(paths) = origins.get(&expression) {
            owners.extend(
                paths
                    .iter()
                    .filter_map(|path| crate::ExecutionPlace::storage_owner(self.storage, path)),
            );
        } else if let Some(place) = place {
            // Provisional execution analysis has no completed storage transition yet.
            owners.insert(place.clone());
        }
    }
}
