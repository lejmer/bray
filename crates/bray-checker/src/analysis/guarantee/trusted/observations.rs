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

    pub(super) fn refresh_borrowed_arguments(
        &self,
        state: &mut ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
        arguments: &mut BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    ) {
        for value in arguments.values_mut() {
            *value = self.complete_captured_references(state, invocation, value.clone());
        }
    }

    pub(super) fn prepare_call_observations(
        &self,
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
