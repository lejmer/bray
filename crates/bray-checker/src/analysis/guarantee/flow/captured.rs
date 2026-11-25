use bray_bound_tree::{
    BoundExecutionSite, StorageCapturedValue, StorageMutation, StorageMutationTarget,
    StorageReferencePath, StorageValueDestination, StorageValueInput,
};
use bray_symbols::CapturedValueTemplate as Value;

use super::super::state::ExecutionState;
use super::model::ExecutionFlowDomain;
use crate::{CheckerRequestContext, ExecutionCondition, ExecutionPlace};

pub(in crate::analysis) type ResolveWrite<'a> =
    dyn Fn(&StorageCapturedValue, &StorageReferencePath) -> Option<Vec<StorageMutationTarget>> + 'a;
pub(in crate::analysis) type CaptureFrameInputs<'a> =
    dyn Fn(&crate::ExecutionCallEvidence) -> Vec<bray_bound_tree::StorageValueCapture> + 'a;

struct ResolvedFailureValues {
    values: std::sync::Arc<[bray_bound_tree::StorageValueTransition]>,
    references: std::sync::Arc<[bray_symbols::DependencyBorrowRelation]>,
    stored_values: std::sync::Arc<[bray_symbols::DependencyStoredValue]>,
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis) fn invocation_value_transitions(
        &self,
        state: &ExecutionState,
        invocation: BoundExecutionSite,
        bindings: &[bray_symbols::CapturedValueOutcome],
        resolve: &ResolveWrite<'_>,
    ) -> Vec<bray_bound_tree::StorageValueTransition> {
        let selection = invocation
            .expression()
            .and_then(|expression| self.semantics.selections().expression(expression));

        let storage = self
            .dependencies
            .calls()
            .get(&invocation)
            .map(|call| call.storage());

        self.entry_value_transitions(
            state.admitted_call(invocation),
            bindings,
            storage,
            |root| selection?.invocation_input_ordinal(invocation, root),
            resolve,
        )
    }

    pub(super) fn entry_value_transitions(
        &self,
        entry: Option<&crate::ExecutionCallEvidence>,
        bindings: &[bray_symbols::CapturedValueOutcome],
        storage: Option<&bray_bound_tree::BoundCallStorage>,
        ordinal: impl Fn(bray_symbols::DependencySubjectRoot) -> Option<bray_symbols::SymbolOrdinal>,
        resolve: &ResolveWrite<'_>,
    ) -> Vec<bray_bound_tree::StorageValueTransition> {
        bindings
            .iter()
            .enumerate()
            .map(|(index, outcome)| {
                let mut transition = self.entry_value_transition(
                    entry,
                    outcome.guard,
                    &outcome.bindings,
                    storage,
                    &ordinal,
                    resolve,
                );

                transition.source_row = Some(index);

                transition.outcome = transition
                    .outcome
                    .with_ownership(outcome.ownership.clone())
                    .with_carriers(self.bound_carriers(&outcome.carriers).iter().cloned());

                transition
            })
            .collect()
    }

    fn entry_value_transition(
        &self,
        entry: Option<&crate::ExecutionCallEvidence>,
        guard: Option<bray_symbols::ConstantTermId>,
        bindings: &[(bray_symbols::DependencySubject, Value)],
        storage: Option<&bray_bound_tree::BoundCallStorage>,
        ordinal: &impl Fn(bray_symbols::DependencySubjectRoot) -> Option<bray_symbols::SymbolOrdinal>,
        resolve: &ResolveWrite<'_>,
    ) -> bray_bound_tree::StorageValueTransition {
        let mut writes = Vec::new();

        for (subject, template) in bindings {
            let value = self.storage_value_expression(template);

            if subject.subject_root() == bray_symbols::DependencySubjectRoot::Result {
                writes.push((subject, StorageValueDestination::Result, value, None));

                continue;
            }

            let Some(input) = ordinal(subject.subject_root()) else {
                continue;
            };

            let Some(descriptor) = entry.and_then(|entry| entry.argument_storage(input)) else {
                continue;
            };

            let Some((ty, path)) = storage.and_then(|storage| storage.mutable_write(subject))
            else {
                continue;
            };

            let Some(targets) = resolve(descriptor, path) else {
                continue;
            };

            let first = path
                .steps()
                .first()
                .expect("a checked write follows its mutable descriptor");

            let projections = &path.projections()[..first.index()];

            let observed = projections
                .iter()
                .fold(descriptor.clone(), |value, field| value.project(*field));

            if let [target] = targets.as_slice()
                && let Some(address) = target.address.as_ref().filter(|target| target.is_exact())
                && let Some(expected) = observed.known_address()
            {
                let destination = StorageValueDestination::Current {
                    target: address.clone(),
                    ty,
                    installed: false,
                };

                writes.push((
                    subject,
                    destination,
                    value,
                    Some((input, projections.into(), expected.clone())),
                ));

                continue;
            }

            writes.extend(targets.into_iter().map(|target| {
                (
                    subject,
                    StorageValueDestination::Region {
                        target: target.footprint,
                        via: subject.clone(),
                    },
                    value.clone(),
                    None,
                )
            }));
        }

        // The exit describes writes in callee coordinates. Instantiation can make
        // independent formal destinations overlap, losing their exact final value.
        let conflicted = writes
            .iter()
            .enumerate()
            .filter_map(|(index, (_, destination, value, _))| {
                let StorageValueDestination::Current { target, .. } = destination else {
                    return None;
                };

                writes
                    .iter()
                    .any(|(_, other, incoming, _)| {
                        let reached = match other {
                            StorageValueDestination::Result => return false,
                            StorageValueDestination::Current { target, .. }
                                if other != destination || incoming != value =>
                            {
                                target
                            }
                            StorageValueDestination::Current { .. } => return false,
                            StorageValueDestination::Region { target, .. } => target,
                        };

                        target.relationship_in_slots(reached, self.storage.identities())
                            != bray_bound_tree::StorageRelationship::Disjoint
                    })
                    .then_some(index)
            })
            .collect::<std::collections::BTreeSet<_>>();

        let mut write_inputs = Vec::new();

        let writes =
            writes
                .into_iter()
                .enumerate()
                .map(|(index, (subject, destination, value, input))| {
                    let destination = match destination {
                        StorageValueDestination::Current { target, .. }
                            if conflicted.contains(&index) =>
                        {
                            StorageValueDestination::Region {
                                target,
                                via: subject.clone(),
                            }
                        }
                        destination => {
                            // The bound destination and input retain the same immutable address paths.
                            write_inputs.extend(input);

                            destination
                        }
                    };

                    (destination, value)
                });

        let transition = bray_bound_tree::StorageValueTransition::new(guard, writes);

        transition.with_write_inputs(write_inputs)
    }

    pub(in crate::analysis::guarantee) fn bound_carriers(
        &self,
        carriers: &[bray_symbols::DependencyWitnessCarrier],
    ) -> std::sync::Arc<[bray_symbols::DependencyWitnessCarrier<bray_bound_tree::StorageProjection>]>
    {
        carriers
            .iter()
            .map(|carrier| {
                let path = crate::storage::storage_projections(
                    self.request.semantic_values(),
                    self.dependencies,
                    carrier.path(),
                )
                .expect("checked result carrier retains its selected projections");

                carrier.remap(carrier.guarantee(), path)
            })
            .collect()
    }

    pub(in crate::analysis) fn storage_value_expression(
        &self,
        value: &Value,
    ) -> Value<StorageValueInput, bray_bound_tree::StorageProjection> {
        value
            .try_map(
                &mut |term| Ok::<_, ()>(StorageValueInput::Term(*term)),
                &mut |projection| {
                    crate::storage::storage_projections(
                        self.request.semantic_values(),
                        self.dependencies,
                        &[*projection],
                    )
                    .and_then(|values| values.into_iter().next())
                    .ok_or(())
                },
                &mut |root| Ok(root.clone()),
                &mut |callee| Ok(callee.clone()),
                &mut |variant| Ok(*variant),
            )
            .unwrap_or(Value::Unknown)
    }

    /// The completed current value, independently of its prebound private prediction.
    pub(in crate::analysis) fn captured_output(
        &self,
        state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
        destination: &StorageValueDestination,
    ) -> Option<ExecutionCondition> {
        match destination {
            StorageValueDestination::Region { .. } => None,
            StorageValueDestination::Result => Some(self.invocation_value(state, invocation)),
            StorageValueDestination::Current { target, ty, .. } => {
                self.current_target_value(state, target, Some(*ty))
            }
        }
    }

    /// Publishes one paired value to current storage and its admitted outcome.
    pub(in crate::analysis) fn publish_captured_output(
        &self,
        state: &mut ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
        destination: &StorageValueDestination,
        value: ExecutionCondition,
    ) {
        match destination {
            StorageValueDestination::Result => {
                if let BoundExecutionSite::Node(bray_bound_tree::AnyBoundNodeId::Expression(
                    expression,
                )) = invocation
                {
                    if let Some(private) = self.private_operator_result(expression) {
                        state.assign(private, value.clone(), None);
                    } else {
                        self.publish_expression(state, expression, value.clone());
                    }
                } else if let Some(place) = self.invocation_result_place(invocation) {
                    state.assign(place, value.clone(), None);
                }
            }
            StorageValueDestination::Current { target, ty, .. } => {
                self.publish_current_value(state, target, *ty, value.clone())
            }
            StorageValueDestination::Region { .. } => {}
        }

        if let Some(entry) = state.admitted_call_mut(invocation) {
            // The destination and entry retain handles to the same immutable evaluated value.
            entry.outputs.publish(destination.clone(), value);
        }
    }

    pub(in crate::analysis) fn publish_current_value(
        &self,
        state: &mut ExecutionState,
        target: &bray_bound_tree::StorageTarget,
        ty: bray_symbols::TypeId,
        value: ExecutionCondition,
    ) {
        let (place, borrowed) = self.current_target_place(target, ty);

        let value = match place.value_in(&state.current) {
            Some(previous) if borrowed && previous.is_reference() => previous.with_referent(value),
            _ => value,
        };

        state.assign(place, value, Some(target.projections()));
    }

    pub(in crate::analysis) fn bind_cleanup_receiver_outputs(
        &self,
        state: &ExecutionState,
        entry: &crate::ExecutionCallEvidence,
        mutation: StorageMutation,
        input: &bray_bound_tree::StorageCapturedValue,
        transition: &bray_bound_tree::StorageValueTransition,
    ) -> std::collections::BTreeMap<StorageValueDestination, ExecutionCondition> {
        let ordinal = bray_symbols::SymbolOrdinal::new(0);
        let place = ExecutionPlace::argument(ordinal);

        let Some(receiver) = place.value_in(&entry.arguments) else {
            return Default::default();
        };

        let reached = receiver
            .clone()
            .into_referent()
            .captured_value(input.clone());

        let receiver = if receiver.is_reference() {
            receiver.with_referent(reached)
        } else {
            reached.borrowed()
        };

        let arguments = std::collections::BTreeMap::from([(place, receiver.clone())]);
        let result = ExecutionCondition::unknown();

        let argument = |input: &StorageValueInput| match input {
            StorageValueInput::Argument(input) => {
                assert_eq!(*input, ordinal, "the lifecycle has one evaluated receiver");

                receiver.clone()
            }
            StorageValueInput::Term(term) => entry
                .formal_condition(self.request.semantic_values(), *term)
                .substitute(
                    &|place| {
                        place
                            .value_in(&arguments)
                            .unwrap_or_else(ExecutionCondition::unknown)
                    },
                    &result,
                    &mut { ExecutionCondition::WORK_LIMIT },
                ),
        };

        self.bind_transition_values(
            state,
            entry,
            mutation,
            &result,
            &transition.outcome.bindings,
            &argument,
            &mut 0,
        )
    }

    pub(in crate::analysis::guarantee) fn replay_captured_publications(
        &self,
        state: &mut ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
        completion: &bray_bound_tree::StorageCompletion,
    ) {
        for (destination, snapshot) in completion.publications() {
            let Some(value) = self.captured_output(state, invocation, destination) else {
                continue;
            };

            self.publish_captured_output(
                state,
                invocation,
                destination,
                value.captured_value(snapshot.clone()),
            );
        }
    }

    pub(in crate::analysis) fn resolve_failure_writes<'state>(
        &self,
        state: &'state ExecutionState,
        mutation: StorageMutation,
        resolve: &ResolveWrite<'_>,
    ) -> Option<(
        &'state crate::ExecutionCallEvidence,
        bray_bound_tree::StorageFailureWrites,
    )> {
        use bray_bound_tree::StorageFailureWrites;

        let StorageMutation::InvocationFailure {
            occurrence,
            kind,
            row,
            ..
        } = mutation
        else {
            unreachable!("failure writes belong to an invocation failure edge");
        };

        if kind == bray_symbols::DependencyFailureKind::Panic
            && let Some((_, source, frame)) = self.direct_await_body(state, occurrence)
        {
            let ordinal = |root: bray_symbols::DependencySubjectRoot| {
                root.input_ordinal(frame.has_receiver())
            };

            let resolved = if let Some(site) = source.local() {
                let call = self.dependencies.calls().get(&site)?;
                let environment = (&*frame.entry, call.storage(), ordinal);

                let rows = call
                    .deferred_reports()
                    .and_then(|reports| reports.failures());

                self.resolve_abnormal_values(state, (kind, row), environment, rows, resolve)
            } else {
                let body = self
                    .dependencies
                    .frame_callees()
                    .get(frame.callee.as_deref()?)?;

                let environment = (&*frame.entry, &body.storage, ordinal);

                self.resolve_abnormal_values(
                    state,
                    (kind, row),
                    environment,
                    body.summary.failure_outcomes(),
                    resolve,
                )
            }?;

            return Some((
                frame.entry.as_ref(),
                StorageFailureWrites::Bound {
                    producer: Some(source.clone()),
                    input_count: frame.entry.positional_arguments().count(),
                    values: resolved.values,
                    references: resolved.references,
                    stored_values: resolved.stored_values,
                },
            ));
        }

        let entry = state.admitted_call(occurrence)?;

        if matches!(occurrence, BoundExecutionSite::Node(_))
            && occurrence
                .expression()
                .and_then(|expression| self.memory.operation(expression))
                .is_some_and(|operation| {
                    crate::memory::execution::commits_buffer_cleanup(operation.kind())
                })
        {
            return Some((entry, StorageFailureWrites::AdmittedOutputs));
        }

        let call = self.dependencies.calls().get(&occurrence)?;

        let selection = occurrence
            .expression()
            .and_then(|expression| self.semantics.selections().expression(expression));

        let environment = (entry, call.storage(), |root| {
            selection?.invocation_input_ordinal(occurrence, root)
        });

        let resolved = self.resolve_abnormal_values(
            state,
            (kind, row),
            environment,
            call.reports().failures(),
            resolve,
        )?;

        Some((
            entry,
            StorageFailureWrites::Bound {
                producer: None,
                input_count: call.input_count(),
                values: resolved.values,
                references: resolved.references,
                stored_values: resolved.stored_values,
            },
        ))
    }

    fn resolve_abnormal_values<Report>(
        &self,
        state: &ExecutionState,
        (kind, selected): (bray_symbols::DependencyFailureKind, Option<usize>),
        (entry, storage, ordinal): (
            &crate::ExecutionCallEvidence,
            &bray_bound_tree::BoundCallStorage,
            impl Fn(bray_symbols::DependencySubjectRoot) -> Option<bray_symbols::SymbolOrdinal>,
        ),
        rows: Option<&[bray_symbols::CapturedFailureOutcome<bray_symbols::ConstantTermId, Report>]>,
        resolve: &ResolveWrite<'_>,
    ) -> Option<ResolvedFailureValues> {
        let proof = state.ordinary_proof();

        let rows = rows?
            .iter()
            .enumerate()
            .filter(|(index, outcome)| {
                outcome.covers(kind)
                    && selected.map_or_else(
                        || {
                            entry.admits_failure(*index)
                                && entry
                                    .failure_guard(*index)
                                    .is_none_or(|guard| proof.can_assume(guard, true))
                        },
                        |selected| *index == selected,
                    )
            })
            .collect::<Vec<_>>();

        if rows.is_empty() {
            return None;
        }

        let values = rows
            .iter()
            .map(|(index, row)| {
                let mut transition = self.entry_value_transition(
                    Some(entry),
                    None,
                    &row.writes,
                    Some(storage),
                    &ordinal,
                    resolve,
                );

                transition.source_row = Some(*index);

                transition
            })
            .collect::<Vec<_>>();

        let selected = rows.iter().map(|(_, row)| *row).collect::<Vec<_>>();

        let references = bray_symbols::CapturedFailureOutcome::reference_origins(
            selected.iter().copied(),
            bray_symbols::DependencySubject::contains,
        );

        let stored_values = bray_symbols::CapturedFailureOutcome::stored_dependencies(
            selected.iter().copied(),
            bray_symbols::DependencySubject::contains,
        );

        (values.iter().any(|row| !row.outcome.bindings.is_empty())
            || !references.is_empty()
            || !stored_values.is_empty())
        .then(|| ResolvedFailureValues {
            values: values.into(),
            references,
            stored_values,
        })
    }

    pub(in crate::analysis) fn bind_failure_writes(
        &self,
        state: &ExecutionState,
        entry: &crate::ExecutionCallEvidence,
        mutation: StorageMutation,
        writes: &bray_bound_tree::StorageFailureWrites,
        narrow: bool,
    ) -> crate::execution_guarantees::ExecutionOutputs {
        let bray_bound_tree::StorageFailureWrites::Bound {
            input_count,
            values,
            ..
        } = writes
        else {
            return entry.outputs.clone();
        };

        let proof = (narrow
            && matches!(
                mutation,
                StorageMutation::InvocationFailure { row: None, .. }
            ))
        .then(|| state.ordinary_proof());

        let values = values.iter().filter(|value| {
            proof.as_ref().is_none_or(|proof| {
                value.source_row.is_none_or(|row| {
                    entry.admits_failure(row)
                        && entry
                            .failure_guard(row)
                            .is_none_or(|guard| proof.can_assume(guard, true))
                })
            })
        });

        self.bind_entry_outputs(
            state,
            entry,
            mutation,
            *input_count,
            ExecutionCondition::unknown(),
            values,
        )
    }

    pub(super) fn replay_failure_writes(
        &self,
        state: &ExecutionState,
        mutation: StorageMutation,
        writes: &[bray_bound_tree::StorageFailureWrites],
    ) -> Option<crate::execution_guarantees::ExecutionOutputs> {
        use crate::execution_guarantees::ExecutionOutputs;
        use bray_bound_tree::StorageFailureWrites;

        let StorageMutation::InvocationFailure {
            occurrence, kind, ..
        } = mutation
        else {
            unreachable!("failure writes belong to an invocation failure edge");
        };

        let frame = (kind == bray_symbols::DependencyFailureKind::Panic)
            .then(|| self.direct_await_body(state, occurrence))
            .flatten();

        let (entry, producer) = if let Some((_, producer, frame)) = frame {
            (frame.entry.as_ref(), Some(producer))
        } else {
            (state.admitted_call(occurrence)?, None)
        };

        let mut reached = writes
            .iter()
            .filter(|writes| match writes {
                StorageFailureWrites::Bound {
                    producer: source, ..
                } => source.as_ref() == producer,
                StorageFailureWrites::AdmittedOutputs => producer.is_none(),
            })
            .map(|writes| self.bind_failure_writes(state, entry, mutation, writes, true));

        let first = reached.next()?;

        let Some(second) = reached.next() else {
            return Some(first);
        };

        let mut rows = first.0.iter().cloned().collect::<Vec<_>>();

        for output in std::iter::once(second).chain(reached) {
            rows.extend(output.0.iter().cloned());
        }

        Some(ExecutionOutputs(rows.into()))
    }

    pub(in crate::analysis) fn complete_failed_writes(
        &self,
        state: &mut ExecutionState,
        mutation: StorageMutation,
        writes: &crate::execution_guarantees::ExecutionOutputs,
        completion: &bray_bound_tree::StorageCompletion,
    ) {
        for (destination, snapshot) in completion.publications() {
            let StorageValueDestination::Current { target, ty, .. } = destination else {
                unreachable!("a failed cleanup publishes only its committed current writes");
            };

            let Some(value) = writes.common_value(destination).cloned() else {
                continue;
            };

            let value = self
                .complete_captured_references(state, mutation, value)
                .captured_value(snapshot.clone());

            self.publish_current_value(state, target, *ty, value);
        }
    }

    pub(in crate::analysis::guarantee) fn bind_entry_values(
        &self,
        state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
        entry: &mut crate::ExecutionCallEvidence,
        captures: &[bray_bound_tree::StorageValueCapture],
    ) {
        let selection = invocation
            .expression()
            .and_then(|expression| self.semantics.selections().expression(expression));

        for capture in captures {
            let input = ExecutionPlace::argument(capture.input);

            let Some(mut value) = input.value_in(&entry.arguments) else {
                continue;
            };

            if let Some(kind) = capture.borrow_kind {
                entry.retain_argument_borrow_kind(capture.input, kind);
            }

            value = self.bind_captured_input(state, value, capture);

            if let Some((pointer, bounds)) = &capture.element_bounds {
                let attach = |value: ExecutionCondition| {
                    let prefix = ExecutionCondition::storage_value(
                        bray_symbols::CapturedValueOperation::Select(
                            bray_bound_tree::StorageProjection::OwnedTarget,
                        ),
                        vec![value.clone()].into(),
                    );

                    let prefix = match prefix.computed_value().kind() {
                        crate::ExecutionConditionKind::StorageValue(
                            bray_symbols::CapturedValueOperation::OwningStorage(field),
                            contents,
                        ) => {
                            assert_eq!(
                                field, pointer,
                                "owning prefix retains its checked backing field"
                            );

                            contents[0].clone()
                        }
                        _ => prefix,
                    }
                    .captured_value(bounds.clone());

                    value.with_storage_component(
                        &[bray_bound_tree::StorageProjection::OwnedTarget],
                        ExecutionCondition::storage_value(
                            bray_symbols::CapturedValueOperation::OwningStorage(*pointer),
                            vec![prefix].into(),
                        ),
                    )
                };

                value = if value.is_reference() {
                    let contents = value.clone().into_referent();

                    value.with_referent(attach(contents))
                } else {
                    attach(value)
                };
            }

            let named = match invocation {
                BoundExecutionSite::Iterate(_)
                | BoundExecutionSite::Next(_)
                | BoundExecutionSite::ScopedEnter(_)
                | BoundExecutionSite::ScopedExit(_) => self
                    .request
                    .trusted_contracts()
                    .and_then(|contracts| contracts.calls.get(&invocation))
                    .and_then(|contract| contract.arguments.keys().next().copied()),
                _ => selection
                    .and_then(|selection| selection.input(capture.input))
                    .and_then(bray_bound_tree::SelectedInput::reference),
            };

            if let Some(named) = named
                && let Some(alias) = entry.arguments.get_mut(&ExecutionPlace::from(named))
            {
                *alias = value.clone();
            }

            entry.arguments.insert(input, value);
        }

        self.admit_buffer_relocation(invocation, captures, entry);
    }

    pub(super) fn bind_captured_input(
        &self,
        state: &ExecutionState,
        value: ExecutionCondition,
        capture: &bray_bound_tree::StorageValueCapture,
    ) -> ExecutionCondition {
        let value = if let Some((ty, snapshot)) = &capture.referent {
            let reached = capture
                .value
                .known_address()
                .and_then(|target| {
                    self.current_captured_target_value(
                        state,
                        target,
                        *ty,
                        capture.value.enclosing_view(*ty),
                    )
                })
                .or_else(|| value.current_referent().cloned())
                .unwrap_or_else(|| value.clone().into_referent());

            let reached = reached.captured_value(std::sync::Arc::clone(snapshot));

            if value.is_reference() {
                value.with_referent(reached)
            } else {
                ExecutionCondition::reference_value(value.borrowed(), reached)
            }
        } else {
            value
        };

        // The outer input capture identifies the passed descriptor even when
        // the reached referent contains another exact descriptor of its own.
        value.captured_value(capture.value.clone())
    }

    fn admit_buffer_relocation(
        &self,
        invocation: BoundExecutionSite,
        captures: &[bray_bound_tree::StorageValueCapture],
        entry: &mut crate::ExecutionCallEvidence,
    ) {
        use crate::ExecutionConditionKind as K;
        use bray_bound_tree::CheckedMemoryOperationKind;

        if !invocation
            .expression()
            .and_then(|expression| self.memory.operation(expression))
            .is_some_and(|operation| {
                matches!(
                    operation.kind(),
                    CheckedMemoryOperationKind::RawBufferRelocate { .. }
                )
            })
        {
            return;
        }

        let [source, destination] = captures else {
            return;
        };

        let pointer = |capture: &bray_bound_tree::StorageValueCapture| {
            let (pointer, _) = capture.element_bounds.as_ref()?;

            let (_, owner) = capture.referent.as_ref()?;

            Some(owner.project(*pointer))
        };

        let (Some(source), Some(destination)) = (pointer(source), pointer(destination)) else {
            return;
        };

        let (Some(source), Some(destination)) =
            (source.known_address(), destination.known_address())
        else {
            return;
        };

        if !crate::storage::buffer_backings_disjoint(
            self.request.semantic_values(),
            self.storage,
            source,
            destination,
        ) {
            return;
        }

        let Some(contract) = self
            .request
            .trusted_contracts()
            .and_then(|contracts| contracts.calls.get(&invocation))
        else {
            return;
        };

        let key = bray_compiler_known::CompilerKnownDeclarationKey::try_new("NonOverlapping")
            .expect("closed memory predicate key must be valid");

        let predicate = self
            .request
            .context()
            .available_compiler_known_symbols()
            .provider()
            .declaration_symbol::<bray_symbols::PredicateSymbolId>(&key)
            .map(bray_symbols::PredicateDefinitionSymbolId::from);

        for requirement in &contract.requirements {
            if matches!(requirement.kind(), K::Trusted(condition)
                if matches!(condition.kind(), K::Predicate(identity, _, _) if Some(*identity) == predicate))
            {
                entry
                    .bind_condition(requirement, &ExecutionCondition::unknown())
                    .pointer_observations(&self.pointer_predicates)
                    .assume(true, &mut entry.trusted_assumptions);
            }
        }
    }

    pub(in crate::analysis) fn prepare_failure_rows(
        &self,
        state: &mut ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) {
        let Some(rows) = self
            .dependencies
            .calls()
            .get(&invocation)
            .and_then(|call| call.reports().failures())
        else {
            return;
        };

        let Some(entry) = state.admitted_call(invocation) else {
            return;
        };

        let selected =
            entry.select_failure_rows(self.request.semantic_values(), &self.proofs, Some(rows));

        if entry.failure_rows != selected {
            state
                .admitted_call_mut(invocation)
                .expect("the selected failure rows belong to the admitted call")
                .failure_rows = selected;
        }
    }

    pub(in crate::analysis) fn invocation_capture<'entry>(
        &self,
        entry: &'entry crate::ExecutionCallEvidence,
        invocation: BoundExecutionSite,
        root: bray_symbols::DependencySubjectRoot,
    ) -> Option<&'entry bray_bound_tree::StorageCapturedValue> {
        let selection = self
            .semantics
            .selections()
            .expression(invocation.expression()?)?;

        entry.argument_storage(selection.invocation_input_ordinal(invocation, root)?)
    }

    pub(in crate::analysis) fn feasible_call_outputs(
        &self,
        state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) -> Vec<usize> {
        let Some(entry) = state.admitted_call(invocation) else {
            return Vec::new();
        };

        self.feasible_entry_outputs(state, entry)
    }

    pub(super) fn feasible_entry_outputs(
        &self,
        state: &ExecutionState,
        entry: &crate::ExecutionCallEvidence,
    ) -> Vec<usize> {
        let unknown = ExecutionCondition::unknown();

        entry
            .outputs
            .0
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                let result = row
                    .writes
                    .get(&StorageValueDestination::Result)
                    .unwrap_or(&unknown);

                let guard = row.guard.bind(entry, result);

                self.admits_output(&state.assumptions, entry, result, &guard, &row.ownership)
                    .then_some(index)
            })
            .collect()
    }

    pub(in crate::analysis) fn prepare_captured_values(
        &self,
        state: &mut ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
        transitions: &[bray_bound_tree::StorageValueTransition],
    ) {
        if transitions.is_empty() {
            return;
        }

        let producer = invocation
            .expression()
            .map(|expression| {
                self.observed_call_result(state, expression)
                    .unwrap_or_else(|| {
                        ExecutionCondition::new(crate::ExecutionConditionKind::Expression(
                            expression,
                        ))
                    })
            })
            .unwrap_or_else(ExecutionCondition::unknown);

        let Some(mut entry) = state.entries.remove(&invocation) else {
            return;
        };

        let input_count = self
            .dependencies
            .calls()
            .get(&invocation)
            .map(|call| call.input_count())
            .or_else(|| {
                invocation
                    .expression()
                    .and_then(|expression| self.semantics.selections().expression(expression))
                    .map(|selection| selection.inputs().count())
            })
            .expect("captured transition has a checked receiver-first input environment");

        let outputs = self.bind_entry_outputs(
            state,
            &entry,
            StorageMutation::Operation(invocation),
            input_count,
            producer,
            transitions,
        );

        if entry.outputs != outputs {
            std::sync::Arc::make_mut(&mut entry).outputs = outputs;
        }

        state.entries.insert(invocation, entry);
    }

    pub(super) fn bind_entry_outputs<'values>(
        &self,
        state: &ExecutionState,
        entry: &crate::ExecutionCallEvidence,
        mutation: StorageMutation,
        input_count: usize,
        producer: ExecutionCondition,
        transitions: impl IntoIterator<Item = &'values bray_bound_tree::StorageValueTransition>,
    ) -> crate::execution_guarantees::ExecutionOutputs {
        let decode = |term| {
            crate::execution_condition_from_type_term(
                self.request.semantic_values(),
                term,
                input_count,
            )
            .expect("checked captured term belongs to its call environment")
        };

        let term = |term| entry.bind_condition(&decode(term), &producer);

        let input = |input: &StorageValueInput| match input {
            StorageValueInput::Argument(ordinal) => ExecutionPlace::argument(*ordinal)
                .value_in(&entry.arguments)
                .unwrap_or_else(ExecutionCondition::unknown),
            StorageValueInput::Term(source) => term(*source),
        };

        let mut outcomes = Vec::new();

        let mut alternative = 0;

        for transition in transitions {
            use crate::execution_guarantees::OutputGuard;

            let sources = transition.source_row.map(|index| [index].into());
            let mut exact = true;

            let reached = transition
                .write_inputs()
                .iter()
                .all(|(input, projections, expected)| {
                    let actual = entry.argument_storage(*input).map(|value| {
                        projections
                            .iter()
                            .fold(value.clone(), |value, field| value.project(*field))
                    });

                    match actual
                        .as_ref()
                        .and_then(StorageCapturedValue::known_address)
                    {
                        Some(actual) => actual == expected,
                        None => {
                            exact = false;

                            true
                        }
                    }
                });

            let transition = &transition.outcome;

            let guard = match transition.guard.map(&decode) {
                Some(formal) if formal.observes_completion_result() => {
                    OutputGuard::Completion(formal)
                }
                Some(formal) => OutputGuard::Bound(entry.bind_condition(&formal, &producer)),
                None => OutputGuard::Bound(ExecutionCondition::boolean(true)),
            };

            // A compound exit guard may already fail its input conditions even
            // while its result condition awaits private production.
            let admission = guard.bind(entry, &ExecutionCondition::unknown());

            if !reached
                || !state
                    .proofs
                    .ordinary_proof(&entry.assumptions)
                    .can_assume(&admission, true)
            {
                alternative += transition
                    .bindings
                    .iter()
                    .flat_map(|(_, value)| value.nodes())
                    .map(|node| match node {
                        Value::Deferred { alternatives, .. } => alternatives.len(),
                        _ => 0,
                    })
                    .sum::<usize>();

                outcomes.push(crate::execution_guarantees::ExecutionOutput {
                    sources,
                    guard: OutputGuard::Bound(ExecutionCondition::boolean(false)),
                    carriers: Default::default(),
                    ownership: Default::default(),
                    writes: Default::default(),
                });

                continue;
            }

            let mut writes = self.bind_transition_values(
                state,
                entry,
                mutation,
                &producer,
                &transition.bindings,
                &input,
                &mut alternative,
            );

            if !exact {
                writes.retain(|destination, _| {
                    !matches!(destination, StorageValueDestination::Current { .. })
                });
            }

            outcomes.push(crate::execution_guarantees::ExecutionOutput {
                sources,
                guard,
                carriers: std::sync::Arc::clone(&transition.carriers),
                ownership: transition.ownership.clone(),
                writes,
            });
        }

        crate::execution_guarantees::ExecutionOutputs(outcomes.into())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "binding retains the admitted entry, selected outcome, and stable value identities"
    )]
    fn bind_transition_values(
        &self,
        state: &ExecutionState,
        entry: &crate::ExecutionCallEvidence,
        mutation: StorageMutation,
        producer: &ExecutionCondition,
        bindings: &[(
            StorageValueDestination,
            Value<StorageValueInput, bray_bound_tree::StorageProjection>,
        )],
        input: &impl Fn(&StorageValueInput) -> ExecutionCondition,
        alternative: &mut usize,
    ) -> std::collections::BTreeMap<StorageValueDestination, ExecutionCondition> {
        bindings
            .iter()
            .map(|(destination, recipe)| {
                let value =
                    self.bind_captured_value(state, entry, recipe, input, mutation, alternative);

                (
                    destination.clone(),
                    self.prepare_output_value(mutation, destination, value, producer),
                )
            })
            .collect()
    }

    fn prepare_output_value(
        &self,
        mutation: StorageMutation,
        destination: &StorageValueDestination,
        value: ExecutionCondition,
        producer: &ExecutionCondition,
    ) -> ExecutionCondition {
        match destination {
            StorageValueDestination::Region { .. } => value,
            StorageValueDestination::Result => value.with_producer(producer.clone()),
            StorageValueDestination::Current { target, ty, .. } => {
                let (place, _) = self.current_target_place(target, *ty);

                let contents = ExecutionCondition::post_state(
                    mutation,
                    ExecutionCondition::new(crate::ExecutionConditionKind::Input(place)),
                );

                value.copied_value().clone().with_producer(contents)
            }
        }
    }

    pub(in crate::analysis::guarantee) fn complete_entry_outputs(
        &self,
        state: &mut ExecutionState,
        (source, publication): (Option<BoundExecutionSite>, BoundExecutionSite),
        result: &ExecutionCondition,
        entry: &crate::ExecutionCallEvidence,
        mut finish: impl FnMut(
            &mut ExecutionState,
            ExecutionCondition,
            Option<&crate::execution_guarantees::ExecutionOutput>,
        ) -> ExecutionCondition,
    ) -> Option<ExecutionCondition> {
        let outputs = &entry.outputs;

        if outputs.0.is_empty() {
            return Some(finish(state, result.clone(), None));
        }

        let mut feasible = outputs.0.iter().filter_map(|row| {
            let guard = row.guard.bind(entry, result);

            self.admits_output(&state.assumptions, entry, result, &guard, &row.ownership)
                .then_some((row, guard))
        });

        let first = feasible.next()?;
        let computation = result.owner_identity();

        // Completed rows replace a returned reference with its actual referent.
        // The invocation's own term still describes that referent's data.
        let referent = publication.expression()
            .and_then(|expression| self.observed_call_result(state, expression))
            .and_then(|observed| observed.current_referent().cloned())
            .filter(|referent| matches!(referent.kind(), crate::ExecutionConditionKind::Call(_, _, _, _, Some(_))));

        let mut complete = |branch: &mut ExecutionState,
                            (row, guard): (
            &crate::execution_guarantees::ExecutionOutput,
            ExecutionCondition,
        )| {
            let value = self.complete_output_row(
                branch,
                source.map(|site| (site, entry)),
                publication,
                result.clone(),
                row,
                guard,
            );

            // A const computation describes the data of its result, or of the referent
            // of its returned reference. The completed row supplies that same data.
            let computed = if matches!(computation.kind(), crate::ExecutionConditionKind::Call(_, _, _, _, None))
                && (result == computation
                    || result.observation_kind() == Some(crate::execution_guarantees::ExecutionObservationKind::Scalar))
            {
                Some((computation, value.computed_value()))
            } else {
                referent.as_ref()
                    .zip(value.current_referent())
                    .map(|(computation, referent)| (computation, referent.computed_value()))
            };

            if let Some((computation, observed)) = computed
                && observed != computation
                && !observed.is_unknown()
                && !observed.is_reference()
                && !computation.observes_live(None)
                && !observed.observes_live(None)
            {
                ExecutionCondition::value_alias(computation.clone(), observed.clone())
                    .assume(true, &mut branch.assumptions);
            }

            // On this row the const call's returned reference is the reached reference,
            // so trusted facts about the call's referent hold for that same referent.
            if let Some((call, reached)) = referent.as_ref().zip(value.current_referent())
                && call != reached
                && !reached.is_unknown()
            {
                let snapshots = std::collections::BTreeMap::from([(call.clone(), reached.clone())]);

                let carried = branch.trusted_assumptions.iter()
                    .filter_map(|(fact, holds)| {
                        let rewritten = fact.with_snapshots(&snapshots);

                        (&rewritten != fact).then_some((rewritten, *holds))
                    })
                    .collect::<Vec<_>>();

                for (fact, holds) in carried {
                    fact.assume(holds, &mut branch.trusted_assumptions);
                }
            }

            finish(branch, value, Some(row))
        };

        let Some(last) = feasible.next_back() else {
            return Some(complete(state, first));
        };

        let expression = publication
            .expression()
            .expect("normal call outcomes retain their publishing expression");

        let mut completed = state.clone();
        let mut value = complete(&mut completed, first);

        for row in feasible {
            let mut branch = state.clone();
            let incoming = complete(&mut branch, row);

            value = super::merge_returned(
                &mut completed,
                &branch,
                &value,
                &incoming,
                result,
                expression,
                None,
            );
        }

        let incoming = complete(state, last);

        value = super::merge_returned(
            &mut completed,
            state,
            &value,
            &incoming,
            result,
            expression,
            None,
        );

        *state = completed;

        Some(value)
    }

    fn complete_output_row(
        &self,
        state: &mut ExecutionState,
        admitted: Option<(BoundExecutionSite, &crate::ExecutionCallEvidence)>,
        publication: BoundExecutionSite,
        mut result: ExecutionCondition,
        row: &crate::execution_guarantees::ExecutionOutput,
        guard: ExecutionCondition,
    ) -> ExecutionCondition {
        if admitted.is_some_and(|(site, _)| site == publication)
            && let Some(entry) = state.admitted_call_mut(publication)
        {
            entry.outputs = crate::execution_guarantees::ExecutionOutputs(
                [crate::execution_guarantees::ExecutionOutput {
                    guard: crate::execution_guarantees::OutputGuard::Bound(guard.clone()),
                    ..row.clone()
                }]
                .into(),
            );
        }

        guard.assume(true, &mut state.assumptions);

        for (destination, value) in &row.writes {
            let value = value.clone();

            if *destination == StorageValueDestination::Result {
                result = value.with_producer(result);
            } else {
                let value = self.complete_captured_references(
                    state,
                    StorageMutation::Operation(publication),
                    value,
                );

                self.publish_captured_output(state, publication, destination, value);
            }
        }

        result
    }

    fn bind_captured_value(
        &self,
        state: &ExecutionState,
        entry: &crate::ExecutionCallEvidence,
        value: &Value<StorageValueInput, bray_bound_tree::StorageProjection>,
        input: &impl Fn(&StorageValueInput) -> ExecutionCondition,
        mutation: bray_bound_tree::StorageMutation,
        alternative: &mut usize,
    ) -> ExecutionCondition {
        match value {
            Value::Input(source) => input(source),
            Value::Static(selection) => {
                ExecutionCondition::new(crate::ExecutionConditionKind::Input(
                    ExecutionPlace::static_instance(selection.clone()),
                ))
            }
            Value::Data(source) => {
                let value = input(source);

                ExecutionCondition::scalar_observation(
                    value.owner_identity().clone(),
                    value.computed_value().clone(),
                )
            }
            Value::Deferred {
                alternatives,
                unknown,
            } => {
                let mut frame = if *unknown {
                    crate::ExecutionDeferredValue::unknown(&state.assumptions)
                } else {
                    crate::ExecutionDeferredValue::default()
                };

                for invocation in alternatives.iter() {
                    let callee = invocation.call.clone();

                    let key = bray_bound_tree::StorageDeferredProducer::Summary {
                        instantiation: mutation,
                        callee: callee.clone(),
                        alternative: *alternative,
                    };

                    *alternative += 1;

                    let arguments = invocation
                        .captures
                        .iter()
                        .enumerate()
                        .map(|(index, value)| {
                            let ordinal = bray_symbols::SymbolOrdinal::new(
                                u32::try_from(index).expect("checked capture ordinal fits"),
                            );

                            (
                                ExecutionPlace::argument(ordinal),
                                self.bind_captured_value(
                                    state,
                                    entry,
                                    value,
                                    input,
                                    mutation,
                                    alternative,
                                ),
                            )
                        })
                        .collect();

                    let mut selection = super::super::deferred::selection(&state.assumptions);

                    if let Some(guard) = &invocation.guard {
                        input(guard).assume(true, &mut selection);
                    }

                    let mut captured = crate::ExecutionCallEvidence {
                        arguments,
                        owned_inputs: std::sync::Arc::clone(&invocation.owned_inputs),
                        pending_execution: true,
                        ..Default::default()
                    };

                    state.transfer_frame_admission(entry, &mut captured);

                    let mut producer = crate::ExecutionDeferredInvocation {
                        callee,
                        transition: None,
                        selection,
                        completed: None,
                        reports: Default::default(),
                        entry: std::sync::Arc::new(captured),
                    };

                    self.bind_carried_report_support(&mut producer);
                    frame.producers.insert(key, producer);
                }

                ExecutionCondition::unknown().with_deferred_value(std::sync::Arc::new(frame))
            }
            Value::Unknown => ExecutionCondition::unknown(),
            Value::Tagged(variant, value) => {
                ExecutionCondition::new(crate::ExecutionConditionKind::Tagged(
                    *variant,
                    std::sync::Arc::new(self.bind_captured_value(
                        state,
                        entry,
                        value,
                        input,
                        mutation,
                        alternative,
                    )),
                ))
            }
            Value::Operation(bray_symbols::CapturedValueOperation::Inline, values) => {
                super::preservation::ValuePreservation::inline_snapshot(
                    &self.bind_captured_value(
                        state,
                        entry,
                        &values[0],
                        input,
                        mutation,
                        alternative,
                    ),
                    mutation,
                )
            }
            Value::Operation(operation, values) => ExecutionCondition::storage_value(
                *operation,
                values
                    .iter()
                    .map(|value| {
                        self.bind_captured_value(state, entry, value, input, mutation, alternative)
                    })
                    .collect(),
            ),
            Value::Aggregate { base, fields } => ExecutionCondition::constructed(
                self.bind_captured_value(state, entry, base, input, mutation, alternative),
                fields
                    .iter()
                    .map(|(field, value)| {
                        (
                            *field,
                            self.bind_captured_value(
                                state,
                                entry,
                                value,
                                input,
                                mutation,
                                alternative,
                            ),
                        )
                    })
                    .collect(),
            ),
        }
    }

    /// Refreshing a reached view does not change the actual enclosing owner's lineage.
    pub(in crate::analysis) fn complete_captured_references(
        &self,
        state: &mut ExecutionState,
        mutation: StorageMutation,
        value: ExecutionCondition,
    ) -> ExecutionCondition {
        let refreshed = self.refresh_captured_references(state, value.clone(), Some(mutation));

        if refreshed != value {
            let ownership = state.owned_result(&value);

            state.owned_results.insert(refreshed.clone(), ownership);
        }

        refreshed
    }

    /// A captured descriptor survives its old slot. Its reached data observes current memory.
    pub(in crate::analysis::guarantee) fn refresh_captured_references(
        &self,
        state: &ExecutionState,
        value: ExecutionCondition,
        completed: Option<StorageMutation>,
    ) -> ExecutionCondition {
        let mut refreshed = value.clone();

        let unchanged = matches!(completed, Some(StorageMutation::Operation(site))
            if state.admitted_call(site).is_some_and(|entry| entry.storage_unchanged));

        for (path, kind) in value.captured_observations() {
            let bray_bound_tree::StorageObservationKind::Reference(ty) = kind else {
                continue;
            };

            let bray_symbols::TypeData::Borrow { target: ty, .. } =
                *self.request.semantic_values().type_data(ty)
            else {
                continue;
            };

            let descriptor = path
                .iter()
                .copied()
                .fold(value.clone(), ExecutionCondition::project);

            if !descriptor.is_reference() {
                continue;
            }

            let Some(storage) = descriptor.inline_captured_storage() else {
                continue;
            };

            let reached = storage.known_address().and_then(|target| {
                self.current_captured_target_value(state, target, ty, storage.enclosing_view(ty))
            });

            let reached = match (reached, completed) {
                (Some(reached), _) => reached,
                // An unchanged transition preserves even an abstract referent.
                (None, Some(_)) if unchanged => continue,
                (None, Some(mutation)) => ExecutionCondition::post_state(
                    mutation,
                    descriptor.owner_identity().clone().into_referent(),
                ),
                (None, None) => continue,
            };

            if descriptor.current_referent() != Some(&reached) {
                refreshed =
                    refreshed.with_storage_component(&path, descriptor.with_referent(reached));
            }
        }

        refreshed
    }
}

impl ExecutionState {
    pub(in crate::analysis) fn select_call_output(
        &mut self,
        invocation: bray_bound_tree::BoundExecutionSite,
        row: usize,
    ) {
        let entry = self
            .admitted_call_mut(invocation)
            .expect("selected output belongs to an admitted call");

        entry.select_output(row);
    }
}
