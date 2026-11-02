use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use bray_symbols::TypeData;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(super) fn owned_transfers(
        &mut self,
        candidates: std::collections::BTreeMap<
            bray_symbols::DependencyOwnedTransfer,
            crate::analysis::guarantee::OwnedReturnCondition,
        >,
    ) -> crate::CheckerQueryResult<Vec<bray_symbols::DependencyOwnedTransfer>, C::UpstreamError>
    {
        use bray_symbols::ConstantTermData;

        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        let inputs = self
            .request
            .trusted_contracts()
            .expect("owned transfer inference retains formal inputs");

        let mut result = Vec::new();
        let mut copyable = std::collections::BTreeMap::new();

        for (transfer, shape) in candidates {
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
                    _ => unreachable!("inferred ownership source contains only input projections"),
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
                    self.owned_result_condition(transfer.output(), presence)?
                } else {
                    None
                };

                // A conditional whole result needs an observable inactive shape.
                if conditional && condition.is_none() {
                    continue;
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
                        .expect("conditional nullable input belongs to the same semantic store");

                    bray_symbols::DependencyOwnedTransfer::new(
                        input,
                        transfer
                            .output()
                            .iter()
                            .copied()
                            .chain([bray_symbols::ConstantProjectionKind::NullableValue]),
                    )
                } else {
                    transfer
                };

                result.push(transfer.with_condition(condition));
            }
        }

        Ok(result)
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

    fn owned_result_condition(
        &mut self,
        path: &[bray_symbols::ConstantProjectionKind],
        presence: bool,
    ) -> crate::CheckerQueryResult<Option<bray_symbols::ConstantTermId>, C::UpstreamError> {
        use crate::ExecutionCondition;
        use bray_bound_tree::BoundOperator;
        use bray_symbols::{ConstantProjectionKind, ConstantValueData, ConstantValueKind};

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
                Some(ConstantProjectionKind::NullableValue) | None => {
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
                Some(ConstantProjectionKind::UnionPayloadField(field)) => Some(
                    subject
                        .clone()
                        .matches_variant(self.dependencies.projection_variant(field)),
                ),
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
                ty = self.owned_projection_type(ty, projection)?;
                subject = subject.component(projection);
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
