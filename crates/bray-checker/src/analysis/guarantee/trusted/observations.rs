use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{BoundDependencySubject, BoundExecutionSite, BoundExpressionId};
use bray_symbols::SymbolOrdinal;

use super::super::{flow::ExecutionFlowDomain, state::ExecutionState};
use crate::{CheckerRequestContext, ExecutionCallEvidence, ExecutionCondition, ExecutionPlace};

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

    pub(in crate::analysis::guarantee) fn admitted_borrow_owners(
        &self,
        invocation: BoundExecutionSite,
        entry: &ExecutionCallEvidence,
    ) -> BTreeMap<SymbolOrdinal, std::sync::Arc<[ExecutionPlace]>> {
        let Some(expression) = invocation.expression() else { return BTreeMap::new(); };

        let Some(selection) = self.semantics.selections().expression(expression) else { return BTreeMap::new(); };

        entry.positional_arguments().filter_map(|(ordinal, argument)| {
            if !argument.is_reference() { return None; }

            let root = selection.invocation_input_root(invocation, ordinal)?;
            let site = selection.invocation_input_site(invocation, root)?;

            let incoming = entry.argument_storage(ordinal).and_then(|argument| argument.known_address())
                .filter(|target| matches!(target.origin,
                    bray_bound_tree::StorageOrigin::Entry { kind: bray_symbols::DependencyReferenceKind::Borrow, .. }))
                .and_then(|target| target.value_observation_path())
                .and_then(|path| ExecutionPlace::storage_owner(self.storage, &path));

            let owners = self.liveness.retained_by(site).filter_map(|subject| {
                let access = match subject {
                    BoundDependencySubject::BorrowCapability(borrow) => {
                        let capability = self.storage.borrow_capability(borrow).expect("retained lifetime has a planned capability");

                        if capability.entry_binding().is_some() { return None; }

                        capability.access()
                    }
                    BoundDependencySubject::StorageAccess(access) if self.retained_storage.contains(&access) => access,
                    _ => return None,
                };

                if self.storage.borrow_reaches_external_storage(access) { return None; }

                ExecutionPlace::storage_owner(self.storage, self.storage.logical_path(access)?)
            });

            // Caller-owned entry loans can support a returned address without
            // introducing a new invocation-local capability.
            Some((ordinal, bray_base::sorted_unique_shared_slice(owners.chain(incoming))))
        }).collect()
    }

    pub(super) fn completed_call_observations<'entry>(
        &self, state: &mut ExecutionState, invocation: BoundExecutionSite, mutation: bray_bound_tree::StorageMutation,
        entry: &'entry ExecutionCallEvidence, retain_authority: bool,
    ) -> (Cow<'entry, BTreeMap<ExecutionPlace, ExecutionCondition>>, BTreeMap<BoundExecutionSite, GuaranteeSource>) {
        let mut arguments = Cow::Borrowed(&entry.arguments);
        let admission = Some((invocation, entry));

        for (place, value) in &entry.arguments {
            let completed = if retain_authority {
                self.complete_captured_references(state, mutation, admission, value.clone())
            } else {
                self.refresh_captured_references(state, value.clone(), Some((mutation, admission)))
            };

            if completed != *value {
                arguments.to_mut().insert(place.clone(), completed);
            }
        }

        let owners = self.observation_call(invocation).map_or_else(BTreeMap::new, |(_, call)| {
            self.prepare_call_observations(invocation, call, &mut arguments, entry, retain_authority)
        });

        (arguments, owners)
    }

    fn prepare_call_observations(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
        call: &bray_bound_tree::SelectedCall,
        arguments: &mut Cow<'_, BTreeMap<crate::ExecutionPlace, ExecutionCondition>>,
        entry: &ExecutionCallEvidence,
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

            let position = SymbolOrdinal::new(
                ordinal
                    .checked_add(u32::from(call.receiver().is_some()))
                    .expect("selected argument order fits"),
            );

            let ordinal = ExecutionPlace::argument(position);

            // A parameter default sees only the earlier evaluated inputs.
            if !arguments.contains_key(&ordinal) {
                continue;
            }

            let reference = crate::representation::reference_kind(self.request, ty);

            if retain_authority && let Some(addresses) = entry.addresses.get(&ordinal) {
                owners.entry(operand).or_default().addresses = addresses.clone();
            }

            if reference != Some(bray_symbols::DependencyReferenceKind::Borrow) {
                continue;
            }

            let input = arguments
                .get(&ordinal)
                .expect("prepared call retains its refreshed argument")
                .clone();

            if retain_authority && let Some(support) = entry.borrow_owners.get(&position) {
                owners.entry(operand).or_default().borrowed.extend(support.iter().cloned());
            }

            if let Some(parameter) = parameter {
                let parameter = bray_bound_tree::BoundReferenceTarget::Surface(parameter.into()).into();

                if arguments.get(&parameter) != Some(&input) {
                    arguments.to_mut().insert(parameter, input);
                }
            }
        }

        if retain_authority && let Some(addresses) = entry.addresses.get(&crate::ExecutionPlace::argument(
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

            if retain_authority && let Some(support) = entry.borrow_owners.get(&SymbolOrdinal::new(0)) {
                owners.entry(input).or_default().borrowed.extend(support.iter().cloned());
            }

            let reference = reference.into();

            if arguments.get(&reference) != Some(&value) {
                arguments.to_mut().insert(reference, value);
            }
        }

        owners
    }
}
