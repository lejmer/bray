use bray_bound_tree::{BoundExpressionId, SelectedConstructionInput, SelectedOperation};
use bray_symbols::{
    BorrowKind, DependencyRequirement, DependencyRequirementKind, DependencySubject,
    DependencySubjectRoot, SymbolOrdinal,
};

#[derive(Clone, Copy)]
pub(crate) enum OperationInput {
    Value(bray_bound_tree::BoundExecutionSite),
    Borrow(BoundExpressionId, BorrowKind),
    Nullable(Option<BoundExpressionId>),
}

impl OperationInput {
    pub(crate) fn borrow_plan(
        self,
        storage: &bray_bound_tree::StoragePlan,
    ) -> Option<bray_bound_tree::StorageAccessPlan> {
        let Self::Borrow(expression, kind) = self else {
            return None;
        };

        Some(
            storage
                .node_plans((expression).into())
                .find(|plan| plan.purpose() == bray_bound_tree::StorageAccessPurpose::Borrow(kind))
                .expect("selected invocation input retains its implicit borrow plan"),
        )
    }

    pub(crate) fn access(
        self,
        storage: &bray_bound_tree::StoragePlan,
    ) -> Option<bray_bound_tree::StorageAccessId> {
        match self {
            Self::Borrow(_, _) => self.borrow_plan(storage).map(|plan| plan.access()),
            Self::Value(bray_bound_tree::BoundExecutionSite::CallInput {
                expression,
                ordinal,
            }) => Some(
                storage
                    .invocation_input(expression, ordinal)
                    .expect("staged operation input has its slot"),
            ),
            _ => self.expression().map(|expression| {
                super::super::instantiation::expression_access(storage, expression)
                    .expect("selected operation input has its source access")
            }),
        }
    }

    pub(crate) fn expression(self) -> Option<BoundExpressionId> {
        match self {
            Self::Value(bray_bound_tree::BoundExecutionSite::Node(
                bray_bound_tree::AnyBoundNodeId::Expression(expression),
            ))
            | Self::Borrow(expression, _) => Some(expression),
            Self::Value(bray_bound_tree::BoundExecutionSite::CallInput { .. }) => None,
            Self::Value(other) => panic!("operation value has an evaluated value site: {other:?}"),
            Self::Nullable(expression) => expression,
        }
    }
}

pub(crate) fn operation_inputs(
    view: bray_bound_tree::BoundUnitView<'_>,
    expression: BoundExpressionId,
    operation: &SelectedOperation,
) -> Vec<(DependencySubjectRoot, OperationInput)> {
    if matches!(
        operation.operator_target(),
        Some(bray_bound_tree::OperatorTarget::BuiltIn(_))
    ) {
        return Vec::new();
    }

    let node = view
        .expression(expression)
        .expect("selected operation retains its source operands");

    if let bray_bound_tree::BoundExpression::Structured(source) = node
        && matches!(operation, SelectedOperation::Index { .. })
    {
        let kind = match operation {
            SelectedOperation::Index { target, .. } => {
                let Some(kind) = target.custom_borrow_kind() else {
                    return Vec::new();
                };

                kind
            }
            _ => unreachable!(),
        };

        let receiver = *source
            .operands()
            .first()
            .expect("indexing has its receiver");

        let mut inputs = vec![(
            DependencySubjectRoot::Receiver,
            OperationInput::Borrow(receiver, kind),
        )];

        if let Some(bounds) = source.slice_bounds() {
            inputs.extend(
                [bounds.lower(), bounds.upper()]
                    .into_iter()
                    .enumerate()
                    .map(|(ordinal, bound)| {
                        (parameter_root(ordinal), OperationInput::Nullable(bound))
                    }),
            );
        } else {
            inputs.extend(source.operands().iter().skip(1).enumerate().map(
                |(ordinal, operand)| {
                    (
                        parameter_root(ordinal),
                        OperationInput::Borrow(*operand, BorrowKind::Shared),
                    )
                },
            ));
        }

        return inputs;
    }

    if let SelectedOperation::Construction(construction) = operation {
        let staged = construction
            .inputs()
            .iter()
            .any(|input| matches!(input, SelectedConstructionInput::Default { .. }));

        return construction
            .inputs()
            .iter()
            .filter_map(|input| {
                let (SelectedConstructionInput::Explicit { ordinal, .. }
                | SelectedConstructionInput::Default { ordinal, .. }) = input;

                let ordinal = SymbolOrdinal::new(*ordinal);

                let source = if staged {
                    bray_bound_tree::BoundExecutionSite::CallInput {
                        expression,
                        ordinal,
                    }
                } else {
                    let SelectedConstructionInput::Explicit { expression, .. } = input else {
                        unreachable!("unstaged construction has no defaults");
                    };

                    (*expression).into()
                };

                Some((
                    DependencySubjectRoot::Parameter(ordinal),
                    OperationInput::Value(source),
                ))
            })
            .collect();
    }

    node.child_expressions()
        .enumerate()
        .map(|(ordinal, expression)| {
            let root = if ordinal == 0 {
                DependencySubjectRoot::Receiver
            } else {
                parameter_root(ordinal - 1)
            };

            let input = if operation.operator_target().is_some() {
                OperationInput::Borrow(expression, BorrowKind::Shared)
            } else {
                OperationInput::Value(expression.into())
            };

            (root, input)
        })
        .collect()
}

fn parameter_root(ordinal: usize) -> DependencySubjectRoot {
    DependencySubjectRoot::Parameter(SymbolOrdinal::new(
        u32::try_from(ordinal).expect("selected invocation has a finite input count"),
    ))
}

pub(super) fn nullable_inputs(
    requirements: &[DependencyRequirement],
    inputs: &[(DependencySubjectRoot, OperationInput)],
) -> Result<Vec<DependencyRequirement>, std::convert::Infallible> {
    // Nullable bounds are caller-created values. Only a present payload can carry
    // the bound expression's dependencies, never the temporary nullable's storage.
    crate::dependency::equations::map_requirements(requirements, 0, &mut |requirement, _| {
        match requirement {
            DependencyRequirement::Guarded(guarded) => {
                if let bray_symbols::DependencyGuard::NullablePresent(subject) = guarded.guard()
                    && subject.projections().is_empty()
                    && let Some((_, OperationInput::Nullable(bound))) = inputs
                        .iter()
                        .find(|(root, _)| *root == subject.subject_root())
                {
                    return Some(if bound.is_some() {
                        nullable_inputs(guarded.requirements(), inputs)
                    } else {
                        Ok(Vec::new())
                    });
                }
            }
            DependencyRequirement::Direct { subject, kind } => {
                if let Some((_, OperationInput::Nullable(bound))) = inputs
                    .iter()
                    .find(|(root, _)| *root == subject.subject_root())
                {
                    if *kind != DependencyRequirementKind::ValueDependencies {
                        return Some(Ok(vec![DependencyRequirement::direct(
                            DependencySubject::root(DependencySubjectRoot::EvaluationStorage),
                            *kind,
                        )]));
                    }

                    if bound.is_none() {
                        return Some(Ok(Vec::new()));
                    }

                    let projections = subject
                        .projections()
                        .strip_prefix(&[bray_symbols::DependencyProjection::NullableValue])
                        .unwrap_or(subject.projections());

                    return Some(Ok(vec![DependencyRequirement::direct(
                        DependencySubject::new(subject.subject_root(), projections.iter().copied()),
                        *kind,
                    )]));
                }
            }
            _ => {}
        }

        None
    })
}

pub(super) fn evaluation_dependencies(
    requirements: &[DependencyRequirement],
    escaping: &mut std::collections::BTreeSet<DependencySubjectRoot>,
) -> Result<Vec<DependencyRequirement>, std::convert::Infallible> {
    let mut result = Vec::with_capacity(requirements.len());

    for requirement in requirements {
        match requirement {
            DependencyRequirement::CleanupDependencies { ty, requirements } => result.push(
                DependencyRequirement::cleanup_dependencies(*ty, evaluation_dependencies(requirements, escaping)?),
            ),
            DependencyRequirement::Direct { subject, kind }
                if subject.subject_root() == DependencySubjectRoot::EvaluationStorage =>
            {
                if *kind != DependencyRequirementKind::ValueDependencies {
                    escaping.insert(DependencySubjectRoot::Result);
                }
            }
            DependencyRequirement::Guarded(guarded) => result.push(DependencyRequirement::guarded(
                guarded.guard().clone(),
                evaluation_dependencies(guarded.requirements(), escaping)?,
            )),
            // Symbolic call inputs describe possible retained storage. An actual
            // selected output must retain it before it can be classified as escaping.
            _ => result.push(requirement.clone()),
        }
    }

    Ok(result)
}
