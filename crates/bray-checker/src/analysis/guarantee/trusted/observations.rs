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

    pub(super) fn prepare_call_observations(
        &self,
        state: &mut ExecutionState,
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
                    .filter(|_| retain_carriers)
                {
                    if let Some(owner) =
                        crate::ExecutionPlace::storage_owner(self.storage, access.access())
                    {
                        owners.entry(operand).or_default().borrowed.insert(owner);
                    }
                }

                continue;
            };

            let input = self.current_input(state, operand);

            // A local borrow alias retains the referent, rather than the slot holding
            // the alias. Its returned view can outlive that slot while the input lives.
            if retain_carriers {
                self.retain_trusted_borrow_owners(
                    &mut owners.entry(operand).or_default().borrowed,
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
            && let Some(place) = self.input_place(input)
        {
            let reference =
                bray_bound_tree::BoundReferenceTarget::Surface(receiver.parameter().into());

            let value = self.current_input(state, input);

            if retain_carriers {
                self.retain_trusted_borrow_owners(
                    &mut owners.entry(input).or_default().borrowed,
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
}
