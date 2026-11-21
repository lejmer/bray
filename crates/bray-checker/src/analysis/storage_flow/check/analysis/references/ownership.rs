use super::super::core::StorageFlowCollector;
use super::outputs::InvocationOutputs;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Source, Target};
use bray_symbols::TypeData;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(super) fn stage_returned_values(
        &self,
        state: &mut crate::analysis::storage_flow::model::StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        invocation: &InvocationOutputs<'_>,
        destination: &bray_bound_tree::StoragePath,
        transfers: impl IntoIterator<Item = bray_symbols::DependencyOwnedTransfer>,
    ) {
        if matches!(occurrence, bray_bound_tree::BoundExecutionSite::CallInput { .. }) {
            return;
        }

        let entry = match invocation {
            InvocationOutputs::Call { entry: Some(entry), .. } | InvocationOutputs::Frame { entry, .. } => *entry,
            _ => return,
        };

        for transfer in transfers {
            let route = invocation.storage().returned_value(&transfer);
            let root = route.input_root();

            let path = route.input_path();
            let input = path.projections();
            let mut output = crate::storage::owned_storage_projections(self.dependencies, transfer.output());
            let wrapping = if matches!(invocation, InvocationOutputs::Call { .. }) {
                self.invocation_wrapping(occurrence, root)
            } else { Vec::new() };

            let adaptation = if input.starts_with(wrapping.as_slice()) {
                &[][..]
            } else {
                // Returning the complete adapted owner also returns its wrapper.
                let Some(adaptation) = wrapping.strip_prefix(input) else { continue; };

                output.extend_from_slice(adaptation);

                adaptation
            };

            let sources = if !route.is_borrowed()
                && let Some(extracted) = entry.extracted_values.get(&transfer.input())
                && let Some(captured) = extracted.inline_captured_storage()
            {
                vec![Source::Captured(captured.clone())]
            } else {
                let captured = match invocation {
                    InvocationOutputs::Call { .. } => self.execution.invocation_capture(entry, occurrence, root),
                    InvocationOutputs::Frame { has_receiver, .. } => root.input_ordinal(*has_receiver)
                        .and_then(|ordinal| entry.argument_storage(ordinal)),
                    InvocationOutputs::Lifecycle { .. } => unreachable!("cleanup has no returned owner"),
                };

                let Some(captured) = captured else { continue; };

                if route.is_borrowed() {
                    let input = input.strip_prefix(wrapping.as_slice()).unwrap_or_default();
                    let captured = input.iter().fold(captured.clone(), |captured, projection| captured.project(*projection));

                    vec![Source::Captured(captured)]
                } else if adaptation.is_empty() {
                    let path = if wrapping.is_empty() { std::borrow::Cow::Borrowed(path) }
                        else {
                            let Some(path) = self.invocation_reference_path(occurrence, root, path) else { continue; };

                            path
                        };

                    let Some(sources) = state.memory.captured_reference_sources(self.request.semantic_values(), captured, &path) else { continue; };

                    sources
                } else {
                    vec![Source::Captured(captured.clone())]
                }
            };

            let output = Target::storage(destination.clone()).project(&output);

            self.stage_preserved_value(state, occurrence, &transfer, sources, &output, adaptation);
        }
    }

    fn stage_preserved_value(
        &self,
        state: &mut crate::analysis::storage_flow::model::StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        transfer: &bray_symbols::DependencyOwnedTransfer,
        sources: Vec<Source>,
        output: &Target,
        adaptation: &[bray_bound_tree::StorageProjection],
    ) {
        let preservation = crate::storage::owned_storage_preservation(self.dependencies, transfer.preservation());
        let preservation = preservation.subtree(adaptation);

        for source in sources {
            state.memory.stage_preserved_value(occurrence, &source, output.clone(), &preservation);
        }
    }

    pub(super) fn owned_result(
        &mut self,
        candidates: bray_symbols::DependencyOwnedResult<
            bray_bound_tree::StorageProjection,
            (
                bray_symbols::ConstantTermId,
                crate::analysis::guarantee::OwnedReturnCondition,
            ),
        >,
    ) -> crate::CheckerQueryResult<bray_symbols::DependencyOwnedResult, C::UpstreamError> {
        use bray_symbols::ConstantTermData;

        let mut copyable = std::collections::BTreeMap::new();

        let result = candidates.try_map(
            &mut |candidate| {
                let (input, shape) = candidate.input();

                let transfer = bray_symbols::DependencyOwnedTransfer::new(
                    input,
                    candidate.output().iter().copied(),
                ).with_preservation(candidate.preservation().clone());

                use crate::analysis::guarantee::OwnedReturnCondition;

                let conditional = shape != OwnedReturnCondition::Unconditional;
                let mut term = transfer.input();
                let mut projections = Vec::new();

                let ordinal = loop {
                    match self
                        .request
                        .semantic_values()
                        .constant_term_data(term)
                        .as_ref()
                    {
                        ConstantTermData::CallableArgument(ordinal) => break ordinal.raw(),
                        ConstantTermData::Projection(projection) => {
                            projections.push(projection.kind());
                            term = projection.subject();
                        }
                        _ => unreachable!(
                            "inferred ownership source contains only input projections"
                        ),
                    }
                };

                let inputs = self.request.trusted_contracts()
                    .expect("owned transfer inference retains formal inputs");

                let input = inputs.inputs
                    [usize::try_from(ordinal).expect("formal input ordinal fits the target")];

                let target = bray_bound_tree::StorageBindingTarget::from_reference_target(input)
                    .expect("formal ownership input has a storage binding role");

                let mut ty = match self
                    .storage
                    .binding(target)
                    .expect("formal ownership input retains its checked storage")
                {
                    bray_bound_tree::StorageBinding::Identity(identity) => self
                        .storage
                        .storage_type(identity)
                        .expect("formal ownership input retains its checked type"),
                    bray_bound_tree::StorageBinding::Access(access) => self
                        .storage
                        .access(access)
                        .expect("formal ownership input retains its checked access")
                        .reached_type(),
                };

                // A checked extraction owns the referent, not the loan descriptor.
                ty = self.request.semantic_values().unborrowed_type(ty);

                for projection in projections.into_iter().rev() {
                    ty = self.owned_projection_type(ty, projection)?;
                }

                let copies = if let Some(copies) = copyable.get(&ty) {
                    *copies
                } else {
                    let resolved = match crate::storage::type_is_copyable(self.request, ty) {
                        crate::CheckerOutcome::Complete(result) => result,
                        crate::CheckerOutcome::Cancelled => {
                            return Err(crate::CheckerQueryError::Cancelled);
                        }
                        crate::CheckerOutcome::UpstreamFailure(error) => {
                            return Err(crate::CheckerQueryError::Upstream(error));
                        }
                    };

                    let (copies, diagnostics) = resolved.into_parts();

                    self.diagnostics.add_range(diagnostics);
                    copyable.insert(ty, copies);

                    copies
                };

                if !copies {
                    let condition = if conditional {
                        self.result_path_condition(transfer.output())?
                    } else {
                        None
                    };

                    // A conditional whole result needs an observable inactive shape.
                    if conditional && condition.is_none() {
                        return Ok(bray_symbols::DependencyOwnedResult::default());
                    }

                    let transfer = bray_symbols::DependencyOwnedResult::Transfer(
                        bray_symbols::DependencyOwnedTransfer::new(
                            transfer.input(),
                            transfer.output().iter().map(|projection| {
                                crate::ExecutionPlace::projection_kind(*projection)
                                    .expect("owned result retains a portable output projection")
                            }),
                        )
                        .with_preservation(transfer.preservation().try_map_projections(&mut |projection| {
                            Ok::<_, std::convert::Infallible>(crate::ExecutionPlace::projection_kind(*projection)
                                .expect("owned retained data has a portable projection"))
                        }).expect("checked owned data projection mapping is infallible")),
                    );

                    let Some(condition) = condition else { return Ok(transfer); };

                    let inactive = self.request.semantic_values().intern_constant_term(ConstantTermData::Unary {
                        operation: bray_symbols::ConstantUnaryOperation::LogicalNot,
                        operand: condition,
                    }).expect("checked payload guard belongs to its semantic store");

                    return Ok(bray_symbols::DependencyOwnedResult::choice([
                        bray_symbols::DependencyOwnedResult::case(condition, transfer),
                        bray_symbols::DependencyOwnedResult::case(inactive, Default::default()),
                    ]));
                }

                Ok(bray_symbols::DependencyOwnedResult::default())
            },
            &mut |(condition, _)| Ok(Some(*condition)),
        )?;

        Ok(result)
    }

    fn owned_projection_type(
        &mut self,
        ty: bray_symbols::TypeId,
        projection: bray_symbols::ConstantProjectionKind,
    ) -> crate::CheckerQueryResult<bray_symbols::TypeId, C::UpstreamError> {
        let projected = crate::storage::projected_owned_type(self.request, ty, projection)?;

        let (projected, diagnostics) = projected.into_parts();

        self.diagnostics.add_range(diagnostics);

        Ok(projected.unwrap_or_else(|| panic!(
            "checked owned path retains its projected type: unit {:?}, owner {ty:?} ({:?}), projection {projection:?}",
            self.request.unit().key(), self.request.semantic_values().type_data(ty),
        )))
    }

    pub(super) fn result_path_condition(
        &mut self,
        path: &[bray_bound_tree::StorageProjection],
    ) -> crate::CheckerQueryResult<Option<bray_symbols::ConstantTermId>, C::UpstreamError> {
        use crate::ExecutionCondition;
        use bray_bound_tree::BoundOperator;
        use bray_bound_tree::StorageProjection;
        use bray_symbols::{ConstantValueData, ConstantValueKind};

        let inputs = self
            .request
            .trusted_contracts()
            .expect("owned result retains formal inputs");

        let result = match self
            .storage
            .binding(bray_bound_tree::StorageBindingTarget::Result)
            .expect("checked body has result storage")
        {
            bray_bound_tree::StorageBinding::Identity(result) => result,
            _ => unreachable!("checked result has an independent storage identity"),
        };

        let mut ty = self
            .storage
            .storage_type(result)
            .expect("checked normal result retains its type");

        let mut subject = ExecutionCondition::result();
        let mut guard = None;

        for &projection in path {
            let condition = match projection {
                StorageProjection::NullableValue => {
                    assert!(
                        matches!(
                            self.request.semantic_values().type_data(ty).as_ref(),
                            TypeData::Nullable(_)
                        ),
                        "inactive nullable output retains its nullable type"
                    );

                    Some(ExecutionCondition::operation(
                        BoundOperator::NotEqual,
                        vec![
                            subject.clone(),
                            ExecutionCondition::new(crate::ExecutionConditionKind::Literal(std::sync::Arc::new(
                                ConstantValueData::new(ty, ConstantValueKind::NullableAbsent),
                            ))),
                        ],
                    ))
                }
                StorageProjection::ActiveUnionPayloadField { variant, .. } => {
                    Some(subject.clone().matches_variant(variant))
                }
                _ => None,
            };

            if let Some(condition) = condition {
                guard = Some(match guard {
                    Some(previous) => ExecutionCondition::operation(
                        BoundOperator::LogicalAnd,
                        vec![previous, condition],
                    ),
                    None => condition,
                });
            }

            ty = self.owned_projection_type(
                ty, crate::ExecutionPlace::projection_kind(projection)
                    .expect("owned result retains an owning output projection"),
            )?;

            subject = subject.project(projection);
        }

        let Some(guard) = guard else {
            return Ok(None);
        };

        Ok(crate::execution_condition_term(
            self.request.semantic_values(),
            &guard,
            &inputs.inputs,
            crate::representation::representation_type(
                self.request,
                bray_compiler_known::RepresentationRole::ScalarBool,
            ),
        )
        .expect("checked owned result guard interns in the same semantic store"))
    }
}
