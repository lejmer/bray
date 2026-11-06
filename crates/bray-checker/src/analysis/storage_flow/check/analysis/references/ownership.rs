use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use bray_symbols::TypeData;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(super) fn prepare_returned_frames(
        &self,
        state: &mut crate::analysis::storage_flow::model::StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        contracts: &bray_bound_tree::BoundCallDependencyContracts,
        destination: &bray_bound_tree::StoragePath,
    ) {
        use crate::analysis::storage_flow::memory::Target;

        if matches!(
            occurrence,
            bray_bound_tree::BoundExecutionSite::CallInput { .. }
        ) || contracts.owned_result().is_empty()
        {
            return;
        }

        // Opaque frames carry producer state without ordinary reference fields.
        // Follow the same checked owning-return paths as the execution domain.
        let selection = self
            .execution
            .semantics
            .selections()
            .expression(
                occurrence
                    .expression()
                    .expect("owned return has its invocation"),
            )
            .expect("owned return retains its selected invocation");

        let transfers = contracts.owned_result().transfers().filter_map(|transfer| {
            let crate::ExecutionCondition::Input(input) =
                crate::execution_condition_from_type_term(
                    self.request.semantic_values(),
                    transfer.input(),
                    contracts.input_count(),
                )
                .expect("checked ownership source decodes in the same store")
            else {
                unreachable!("checked ownership source contains only input projections");
            };

            let crate::execution_guarantees::ExecutionInput::Argument(ordinal) = input.root else {
                unreachable!("checked ownership source names a receiver-first input");
            };

            let root = selection
                .invocation_input_root(occurrence, ordinal)
                .expect("owned return retains its actual input");

            let source = self.invocation_place(occurrence, root)?;

            let input =
                crate::storage::owned_storage_projections(self.dependencies, &input.projections);

            let mut output =
                crate::storage::owned_storage_projections(self.dependencies, transfer.output());

            let wrapping = self.invocation_wrapping(occurrence, root);

            let input = if let Some(path) = input.strip_prefix(wrapping.as_slice()) {
                path
            } else {
                // Returning the complete adapted owner also returns its wrapper.
                output.extend_from_slice(wrapping.strip_prefix(input.as_slice())?);

                &[]
            };

            Some((
                Target::storage(source).project(input),
                Target::storage(destination.clone()).project(&output),
            ))
        });

        state.memory.stage_deferred_values(occurrence, transfers);
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

        if candidates.is_empty() {
            return Ok(bray_symbols::DependencyOwnedResult::default());
        }

        let inputs = self
            .request
            .trusted_contracts()
            .expect("owned transfer inference retains formal inputs");

        let mut copyable = std::collections::BTreeMap::new();

        let result = candidates.try_map(
            &mut |candidate| {
                let (input, shape) = candidate.input();

                let transfer = bray_symbols::DependencyOwnedTransfer::new(
                    input,
                    candidate.output().iter().copied(),
                );

                use crate::analysis::guarantee::OwnedReturnCondition;

                let conditional = shape != OwnedReturnCondition::Unconditional;
                let presence = shape == OwnedReturnCondition::PresentValue;
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
                        self.result_path_condition(transfer.output(), presence)?
                    } else {
                        None
                    };

                    // A conditional whole result needs an observable inactive shape.
                    if conditional && condition.is_none() {
                        return Ok(bray_symbols::DependencyOwnedResult::default());
                    }

                    let transfer = if presence {
                        let values = self.request.semantic_values();

                        let input = values
                            .intern_constant_term(bray_symbols::ConstantTermData::Projection(
                                bray_symbols::ConstantProjection::new(
                                    transfer.input(),
                                    bray_symbols::ConstantProjectionKind::NullableValue,
                                ),
                            ))
                            .expect(
                                "conditional nullable input belongs to the same semantic store",
                            );

                        bray_symbols::DependencyOwnedTransfer::new(
                            input,
                            transfer
                                .output()
                                .iter()
                                .copied()
                                .chain([bray_bound_tree::StorageProjection::NullableValue]),
                        )
                    } else {
                        transfer
                    };

                    return Ok(bray_symbols::DependencyOwnedResult::Transfer(
                        bray_symbols::DependencyOwnedTransfer::new(
                            transfer.input(),
                            transfer.output().iter().map(|projection| {
                                crate::ExecutionPlace::projection_kind(*projection)
                                    .expect("owned result retains a portable output projection")
                            }),
                        )
                        .with_condition(condition),
                    ));
                }

                Ok(bray_symbols::DependencyOwnedResult::default())
            },
            &mut |(condition, _)| Ok(Some(*condition)),
        )?;

        Ok(if result.is_empty() {
            bray_symbols::DependencyOwnedResult::default()
        } else {
            result
        })
    }

    fn owned_projection_type(
        &mut self,
        ty: bray_symbols::TypeId,
        projection: bray_symbols::ConstantProjectionKind,
    ) -> crate::CheckerQueryResult<bray_symbols::TypeId, C::UpstreamError> {
        use bray_symbols::{ConstantProjectionKind, DependencyProjection};

        let projection = match projection {
            ConstantProjectionKind::ProductField(field) => {
                DependencyProjection::ProductField(field)
            }
            ConstantProjectionKind::UnionPayloadField(field) => {
                DependencyProjection::UnionPayloadField(field)
            }
            ConstantProjectionKind::TupleElement(ordinal) => {
                DependencyProjection::TupleElement(ordinal)
            }
            ConstantProjectionKind::NullableValue => DependencyProjection::NullableValue,
            ConstantProjectionKind::ArrayElementOrdinal(_) => {
                let data = self.request.semantic_values().type_data(ty);

                let TypeData::Array { element, .. } = data.as_ref() else {
                    panic!("checked owned array projection retains its array type: {ty:?}");
                };

                return Ok(*element);
            }
            _ => unreachable!("inferred ownership uses an owning projection"),
        };

        let projected = crate::storage::projected_value_type(self.request, ty, projection)?;

        let (projected, diagnostics) = projected.into_parts();

        self.diagnostics.add_range(diagnostics);

        Ok(projected.expect("checked owned path retains its projected type"))
    }

    pub(super) fn result_path_condition(
        &mut self,
        path: &[bray_bound_tree::StorageProjection],
        presence: bool,
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

        let mut subject = ExecutionCondition::Result;
        let mut guard = None;

        for projection in path
            .iter()
            .copied()
            .map(Some)
            .chain(presence.then_some(None))
        {
            let condition = match projection {
                None if !matches!(
                    self.request.semantic_values().type_data(ty).as_ref(),
                    TypeData::Nullable(_)
                ) =>
                {
                    None
                }
                Some(StorageProjection::NullableValue) | None => {
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
                            ExecutionCondition::Literal(std::sync::Arc::new(
                                ConstantValueData::new(ty, ConstantValueKind::NullableAbsent),
                            )),
                        ],
                    ))
                }
                Some(StorageProjection::ActiveUnionPayloadField { variant, .. }) => {
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

            if let Some(projection) = projection {
                ty = self.owned_projection_type(
                    ty,
                    crate::ExecutionPlace::projection_kind(projection)
                        .expect("owned result retains an owning output projection"),
                )?;

                subject = subject.project(projection);
            }
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
