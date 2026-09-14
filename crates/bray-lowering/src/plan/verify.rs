use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, AsyncScopeExitPlan, AsyncSuspensionPoint, AsyncTaskOperationKind, BoundBlockId,
    BoundDependencySubject, BoundExpressionId, BoundUnit, BoundUnitId, BoundUnitKind, CheckedAsync,
    CheckedDependencyContracts, CheckedSemanticSelections, Liveness, StorageExitPoint, StorageFlow,
    StorageIdentityId, StoragePlan,
};
use bray_symbols::AvailableCompilerKnownSymbols;

use super::expression::{verify_suspensions, verify_task_operations};
use super::scope::verify_scope_exits;
use super::{LoweringPlanFailure, LoweringPlanFailureCause};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum CleanupPlanLookupError {
    InvalidScopeDepth {
        scope_depth: usize,
        active_scope_count: usize,
        exit: AnyBoundNodeId,
    },
    MissingScopeExit {
        scope: BoundBlockId,
        exit: AnyBoundNodeId,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ScopeExitCleanupStatus {
    /// Checked control flow proves that this syntactic completion cannot run.
    Unreachable,
    /// The reachable exit performs no cleanup.
    NoCleanup,
    /// The reachable exit performs cancellation or lifecycle cleanup.
    Cleanup,
}

/// Complete checked async, cleanup, and task plans safe for MIR lowering.
#[derive(Clone)]
pub struct VerifiedLoweringPlans<'unit> {
    storage: &'unit StoragePlan,
    liveness: &'unit Liveness,
    flow: &'unit StorageFlow,
    dependencies: &'unit CheckedDependencyContracts,
    selections: &'unit CheckedSemanticSelections,
    symbols: &'unit AvailableCompilerKnownSymbols,
    analysis: &'unit CheckedAsync,
    suspensions: BTreeMap<BoundExpressionId, usize>,
    task_operations: BTreeMap<BoundExpressionId, AsyncTaskOperationKind>,
    scope_exits: BTreeMap<(BoundBlockId, AnyBoundNodeId), usize>,
    lifecycle_storage: BTreeSet<StorageIdentityId>,
    replacements: BTreeMap<BoundExpressionId, usize>,
    completed: BTreeSet<(AnyBoundNodeId, bray_bound_tree::StorageAccessId)>,
}

impl<'unit> VerifiedLoweringPlans<'unit> {
    /// Verifies partial checked analyses and publishes one lowering-ready plan set.
    #[expect(
        clippy::too_many_arguments,
        reason = "verification compares independently published semantic inputs"
    )]
    pub fn try_new(
        unit: &BoundUnit,
        storage: &'unit StoragePlan,
        liveness: &'unit Liveness,
        flow: &'unit StorageFlow,
        dependencies: &'unit CheckedDependencyContracts,
        selections: &'unit CheckedSemanticSelections,
        symbols: &'unit AvailableCompilerKnownSymbols,
        analysis: &'unit CheckedAsync,
    ) -> Result<Self, LoweringPlanFailure> {
        if storage.unit() != unit.unit()
            || storage.kind() != unit.key().kind()
            || liveness.unit() != unit.unit()
            || liveness.kind() != unit.key().kind()
            || flow.unit() != unit.unit()
            || flow.kind() != unit.key().kind()
            || dependencies.unit() != unit.unit()
            || dependencies.kind() != unit.key().kind()
            || selections.unit() != unit.unit()
            || selections.kind() != unit.key().kind()
            || analysis.unit() != unit.unit()
            || analysis.kind() != unit.key().kind()
        {
            return Err(LoweringPlanFailure::analysis(
                LoweringPlanFailureCause::Unexpected,
            ));
        }

        if storage.is_recovered()
            || liveness.is_recovered()
            || flow.is_recovered()
            || dependencies.is_recovered()
        {
            return Err(LoweringPlanFailure::analysis(
                LoweringPlanFailureCause::Recovered,
            ));
        }

        if !dependencies.is_complete_for(unit, storage) {
            return Err(LoweringPlanFailure::analysis(
                LoweringPlanFailureCause::Missing,
            ));
        }

        if !analysis
            .frame_dependencies()
            .iter()
            .all(|subject| dependency_subject_exists(unit, storage, *subject))
        {
            return Err(LoweringPlanFailure::analysis(
                LoweringPlanFailureCause::Unexpected,
            ));
        }

        let suspensions = verify_suspensions(
            unit,
            storage,
            liveness,
            dependencies,
            selections,
            symbols,
            analysis,
        )?;

        let task_operations = verify_task_operations(unit, selections, symbols, analysis)?;

        let retained = analysis
            .suspensions()
            .iter()
            .flat_map(|suspension| suspension.retained_subjects().iter().copied())
            .collect::<BTreeSet<_>>();

        if !analysis.frame_dependencies().iter().copied().eq(retained) {
            return Err(LoweringPlanFailure::analysis(
                LoweringPlanFailureCause::Contradictory,
            ));
        }

        let (scope_exits, lifecycle_storage) =
            verify_scope_exits(unit, storage, flow, dependencies, analysis)?;

        let replacements = super::replacement::verify_replacements(unit, storage, flow, analysis)?;

        // Detailed producer failures take precedence over the summary recovery bit.
        if analysis.is_recovered() {
            return Err(LoweringPlanFailure::analysis(
                LoweringPlanFailureCause::Recovered,
            ));
        }

        Ok(Self {
            storage,
            liveness,
            flow,
            dependencies,
            selections,
            symbols,
            analysis,
            suspensions,
            task_operations,
            scope_exits,
            lifecycle_storage,
            replacements,
            completed: BTreeSet::new(),
        })
    }

    /// Adds certified whole-value completion at existing cleanup sites.
    /// Represented-part cleanup retains its separately selected obligations.
    pub fn with_completed_finalizers(
        mut self,
        completed: BTreeSet<(AnyBoundNodeId, bray_bound_tree::StorageAccessId)>,
    ) -> Result<Self, LoweringPlanFailure> {
        for (node, access) in &completed {
            let scope =
                self.analysis.scope_exits().iter().any(|plan| {
                    plan.exit() == *node && plan.lifecycle_resolution().contains(access)
                });

            let replacement = self.analysis.replacements().iter().any(|plan| {
                AnyBoundNodeId::from(plan.expression()) == *node
                    && plan.access() == *access
                    && plan.parts().is_none()
            });

            if !scope && !replacement {
                return Err(LoweringPlanFailure::analysis(
                    LoweringPlanFailureCause::Unexpected,
                ));
            }
        }

        self.completed = completed;

        Ok(self)
    }

    pub(crate) fn finalizer_is_complete(
        &self,
        node: AnyBoundNodeId,
        access: bray_bound_tree::StorageAccessId,
    ) -> bool {
        self.completed.contains(&(node, access))
    }

    /// Returns the bound unit whose plan set was verified.
    pub const fn unit(&self) -> BoundUnitId {
        self.analysis.unit()
    }

    /// Returns the semantic unit category whose plan set was verified.
    pub const fn kind(&self) -> BoundUnitKind {
        self.analysis.kind()
    }

    pub(crate) const fn storage_plan(&self) -> &'unit StoragePlan {
        self.storage
    }

    pub(crate) const fn storage_flow(&self) -> &'unit StorageFlow {
        self.flow
    }

    pub(crate) const fn liveness(&self) -> &'unit Liveness {
        self.liveness
    }

    pub(crate) const fn dependency_contracts(&self) -> &'unit CheckedDependencyContracts {
        self.dependencies
    }

    pub(crate) const fn semantic_selections(&self) -> &'unit CheckedSemanticSelections {
        self.selections
    }

    pub(crate) const fn available_compiler_known_symbols(
        &self,
    ) -> &'unit AvailableCompilerKnownSymbols {
        self.symbols
    }

    /// Returns the verified old-value cleanup for one evaluated assignment.
    pub fn replacement(
        &self,
        expression: BoundExpressionId,
    ) -> Option<&'unit bray_bound_tree::StorageReplacementPlan> {
        self.replacements
            .get(&expression)
            .and_then(|index| self.analysis.replacements().get(*index))
    }

    /// Returns frame dependencies after complete-plan verification.
    pub fn frame_dependencies(&self) -> &[BoundDependencySubject] {
        self.analysis.frame_dependencies()
    }

    /// Returns the verified suspension plan for one suspending expression.
    pub fn suspension(&self, expression: BoundExpressionId) -> Option<&AsyncSuspensionPoint> {
        self.suspensions
            .get(&expression)
            .and_then(|index| self.analysis.suspensions().get(*index))
    }

    /// Returns the verified task operation for one selected call.
    pub fn task_operation(&self, expression: BoundExpressionId) -> Option<AsyncTaskOperationKind> {
        self.task_operations.get(&expression).copied()
    }

    /// Returns verified scope-exit plans for active scopes in cleanup order.
    pub(crate) fn cleanup_plans(
        &self,
        active_scopes: &[BoundBlockId],
        scope_depth: usize,
        exit: AnyBoundNodeId,
    ) -> Result<Vec<AsyncScopeExitPlan>, CleanupPlanLookupError> {
        let scopes =
            active_scopes
                .get(scope_depth..)
                .ok_or(CleanupPlanLookupError::InvalidScopeDepth {
                    scope_depth,
                    active_scope_count: active_scopes.len(),
                    exit,
                })?;

        scopes
            .iter()
            .rev()
            .map(|scope| {
                self.scope_exits
                    .get(&(*scope, exit))
                    .and_then(|index| self.analysis.scope_exits().get(*index))
                    // Lowering mutates its builder while retaining these shared immutable plans.
                    .cloned()
                    .ok_or(CleanupPlanLookupError::MissingScopeExit {
                        scope: *scope,
                        exit,
                    })
            })
            .collect()
    }

    /// Returns the verified cleanup status for one scope exit.
    pub(crate) fn scope_cleanup_status(
        &self,
        scope: BoundBlockId,
        exit: AnyBoundNodeId,
    ) -> Result<ScopeExitCleanupStatus, CleanupPlanLookupError> {
        let Some(index) = self.scope_exits.get(&(scope, exit)) else {
            let point = StorageExitPoint::new(scope, exit);

            return if self.flow.reachable_exits().binary_search(&point).is_ok() {
                Err(CleanupPlanLookupError::MissingScopeExit { scope, exit })
            } else {
                Ok(ScopeExitCleanupStatus::Unreachable)
            };
        };

        let plan = self
            .analysis
            .scope_exits()
            .get(*index)
            .ok_or(CleanupPlanLookupError::MissingScopeExit { scope, exit })?;

        Ok(if !plan.has_cleanup() {
            ScopeExitCleanupStatus::NoCleanup
        } else {
            ScopeExitCleanupStatus::Cleanup
        })
    }

    /// Returns storage occurrences whose cleanup depends on runtime initialization state.
    pub(crate) fn initialization_guards(
        &self,
    ) -> impl Iterator<Item = bray_bound_tree::StorageAccessId> + '_ {
        self.analysis
            .scope_exits()
            .iter()
            .flat_map(|exit| {
                exit.storage()
                    .iter()
                    .filter_map(|decision| match decision.disposition() {
                        bray_bound_tree::AsyncStorageExitDisposition::Cleanup {
                            access, ..
                        } => Some(access),
                        _ => None,
                    })
            })
            .chain(self.flow.replacements().iter().filter_map(|decision| {
                if decision.state() != bray_bound_tree::StorageReplacementState::Conditional {
                    return None;
                }

                self.storage
                    .root_identity(decision.access())
                    .and_then(|identity| self.storage.root_access(identity))
            }))
    }

    /// Returns the checked represented-part partition borrowed from the source analysis.
    pub(crate) fn cleanup_parts(
        &self,
        identity: StorageIdentityId,
    ) -> Option<&'unit [bray_bound_tree::StorageCleanupPart]> {
        self.analysis
            .storage_requirements()
            .iter()
            .find(|requirement| requirement.identity() == identity)
            .and_then(bray_bound_tree::AsyncStorageRequirement::parts)
    }

    /// Rejects represented projections that contradict their semantic type constructors.
    pub(crate) fn validate_cleanup_types(
        &self,
        values: &bray_symbols::SemanticValueStore,
    ) -> Result<(), crate::LoweringInputError> {
        for (owner, kind, call) in self.storage.owned_borrows() {
            let owner = values.type_data(owner)?;

            if !storage_protocol_call_matches(values, self.symbols, &owner, call, Some(kind))? {
                return Err(
                    LoweringPlanFailure::analysis(LoweringPlanFailureCause::Contradictory).into(),
                );
            }
        }

        for requirement in self.analysis.storage_requirements() {
            for part in requirement.parts().into_iter().flatten() {
                for projection in part.projections() {
                    let source = values.type_data(projection.source_type())?;
                    values.type_data(projection.result_type())?;

                    if !cleanup_projection_matches_type(*projection, &source) {
                        return Err(LoweringPlanFailure::storage_requirement(
                            LoweringPlanFailureCause::Contradictory,
                            requirement.identity(),
                        )
                        .into());
                    }

                    if let bray_bound_tree::StorageCleanupProjectionKind::OwnedTarget(call) =
                        projection.projection()
                        && !storage_protocol_call_matches(
                            values,
                            self.symbols,
                            &source,
                            call,
                            Some(bray_symbols::BorrowKind::Mutable),
                        )?
                    {
                        return Err(LoweringPlanFailure::storage_requirement(
                            LoweringPlanFailureCause::Contradictory,
                            requirement.identity(),
                        )
                        .into());
                    }
                }

                if let Some(call) = part.release() {
                    let owner = part
                        .projections()
                        .last()
                        .map(|projection| projection.result_type())
                        .or_else(|| self.storage.storage_type(requirement.identity()));

                    let valid = if let Some(owner) = owner {
                        storage_protocol_call_matches(
                            values,
                            self.symbols,
                            values.type_data(owner)?.as_ref(),
                            call,
                            None,
                        )?
                    } else {
                        false
                    };

                    if !valid {
                        return Err(LoweringPlanFailure::storage_requirement(
                            LoweringPlanFailureCause::Contradictory,
                            requirement.identity(),
                        )
                        .into());
                    }
                }
            }
        }

        Ok(())
    }

    /// Returns whether one storage identity participates in any lifecycle phase.
    pub fn requires_lifecycle_storage(&self, storage: StorageIdentityId) -> bool {
        self.lifecycle_storage.contains(&storage)
    }
}

fn storage_protocol_call_matches(
    values: &bray_symbols::SemanticValueStore,
    symbols: &AvailableCompilerKnownSymbols,
    owner: &bray_symbols::TypeData,
    call: bray_bound_tree::StorageProtocolCall,
    borrow: Option<bray_symbols::BorrowKind>,
) -> Result<bool, bray_symbols::SemanticValueStoreError> {
    use bray_symbols::TypeData;

    let TypeData::OwnedIndirection { storage, target } = owner else {
        return Ok(false);
    };

    let parameter = values.type_data(call.parameter())?;
    let result = values.type_data(call.result())?;
    let signature = values.type_data(call.callable_type())?;
    values.generic_substitution_data(call.callable().substitution())?;

    let TypeData::Callable(signature) = signature.as_ref() else {
        return Ok(false);
    };

    if signature.abi() != bray_symbols::CallableAbi::Bray
        || signature.execution() != bray_symbols::CallableExecution::Synchronous
        || signature.is_variadic()
        || signature.parameters().len() != 1
        || signature
            .parameters()
            .first()
            .map(|parameter| parameter.ty())
            != Some(call.parameter())
        || signature.result() != call.result()
    {
        return Ok(false);
    }

    Ok(if let Some(borrow) = borrow {
        matches!(parameter.as_ref(), TypeData::Borrow { kind, target } if *kind == borrow && target == storage)
            && matches!(result.as_ref(), TypeData::Borrow { kind, target: result } if *kind == borrow && result == target)
    } else {
        call.parameter() == *storage
            && matches!(result.as_ref(), TypeData::Named { definition: bray_symbols::NamedTypeSymbolId::Struct(definition), .. }
                if symbols.symbol_representation(*definition) == Some(bray_compiler_known::RepresentationRole::Unit))
    })
}

fn cleanup_projection_matches_type(
    projection: bray_bound_tree::StorageCleanupProjection,
    source: &bray_symbols::TypeData,
) -> bool {
    use bray_bound_tree::{StorageCleanupProjectionKind, StorageProjection};
    use bray_symbols::TypeData;

    let result = projection.result_type();

    match (projection.projection(), source) {
        (
            StorageCleanupProjectionKind::OwnedTarget(_),
            TypeData::OwnedIndirection { target, .. },
        ) => result == *target,
        (
            StorageCleanupProjectionKind::ArrayElements(extent),
            TypeData::Array { element, length },
        ) => extent == *length && result == *element,
        (
            StorageCleanupProjectionKind::Component(StorageProjection::NullableValue),
            TypeData::Nullable(element),
        ) => result == *element,
        (
            StorageCleanupProjectionKind::Component(StorageProjection::TupleElement(ordinal)),
            TypeData::Tuple(elements),
        ) => {
            usize::try_from(ordinal.raw())
                .ok()
                .and_then(|index| elements.get(index))
                == Some(&result)
        }
        (
            StorageCleanupProjectionKind::UnionPayloadElement { .. }
            | StorageCleanupProjectionKind::Component(StorageProjection::ActiveUnionPayloadField {
                ..
            }),
            TypeData::Named {
                definition: bray_symbols::NamedTypeSymbolId::Union(_),
                ..
            },
        ) => true,
        (
            StorageCleanupProjectionKind::Component(
                StorageProjection::ProductField(_) | StorageProjection::TupleElement(_),
            ),
            TypeData::Named {
                definition: bray_symbols::NamedTypeSymbolId::Struct(_),
                ..
            },
        ) => true,
        _ => false,
    }
}

pub(crate) fn dependency_subject_exists(
    unit: &BoundUnit,
    storage: &StoragePlan,
    subject: BoundDependencySubject,
) -> bool {
    match subject {
        BoundDependencySubject::Storage(identity) => storage.identity(identity).is_some(),
        BoundDependencySubject::StorageAccess(access) => storage.access(access).is_some(),
        BoundDependencySubject::BorrowCapability(borrow) => {
            storage.borrow_capability(borrow).is_some()
        }
        BoundDependencySubject::ScopedCapability(capability) => capability.unit() == unit.unit(),
        BoundDependencySubject::LifecycleObligation(obligation) => obligation.unit() == unit.unit(),
        BoundDependencySubject::ImplementationWitness(_)
        | BoundDependencySubject::ProductStatic(_)
        | BoundDependencySubject::ExactThreadStatic(_) => true,
    }
}

#[cfg(test)]
mod tests {
    fn policy_callable_type(
        values: &SemanticValueStore,
        parameters: impl IntoIterator<Item = bray_symbols::TypeId>,
        result: bray_symbols::TypeId,
        abi: bray_symbols::CallableAbi,
        execution: bray_symbols::CallableExecution,
        variadic: bool,
    ) -> bray_symbols::TypeId {
        use bray_symbols::{
            CallableConstness, CallableDependencyContracts, CallableParameterData,
            CallableParameterMode, CallableParameterName, CallablePosition, CallableTrust,
            CallableTypeData, DependencyContractTemplateData,
        };

        let dependency = values
            .intern_dependency_contract_template(DependencyContractTemplateData::new([]))
            .unwrap();

        let signature = CallableTypeData::new(
            parameters.into_iter().map(|ty| {
                CallableParameterData::new(
                    CallableParameterName::try_new("value").unwrap(),
                    CallablePosition::PositionalOrNamed,
                    CallableParameterMode::Immutable,
                    ty,
                )
            }),
            result,
            CallableConstness::Runtime,
            CallableTrust::Safe,
            abi,
            CallableDependencyContracts::for_execution(execution, dependency, dependency),
        )
        .with_variadic(variadic);

        values.intern_type(TypeData::Callable(signature)).unwrap()
    }

    #[test]
    fn owned_cleanup_calls_require_the_exact_policy_and_target_types() {
        use bray_bound_tree::StorageProtocolCall;

        use bray_symbols::{
            BorrowKind, CallableDefinitionId, CallableInstanceData, FunctionSymbolId, SymbolId,
        };

        let values = SemanticValueStore::try_new().unwrap();
        let policy = values.intern_type(TypeData::tuple([])).unwrap();
        let target = values.intern_type(TypeData::Nullable(policy)).unwrap();
        let symbols = available_compiler_known_symbols();

        let unit = values
            .intern_non_generic_named_type(bray_symbols::NamedTypeSymbolId::Struct(
                symbols
                    .representation_symbol(bray_compiler_known::RepresentationRole::Unit)
                    .unwrap(),
            ))
            .unwrap();

        let owner = TypeData::OwnedIndirection {
            storage: policy,
            target,
        };

        let policy_borrow = values
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Mutable,
                target: policy,
            })
            .unwrap();

        let target_borrow = values
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Mutable,
                target,
            })
            .unwrap();

        let shared_target = values
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Shared,
                target,
            })
            .unwrap();

        let shared_policy = values
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Shared,
                target: policy,
            })
            .unwrap();

        let function = FunctionSymbolId::from_symbol_id(SymbolId::new(1));
        let owner_id = bray_symbols::GenericOwnerId::try_new(function.into()).unwrap();

        let substitution = values
            .intern_generic_substitution(
                bray_symbols::GenericSubstitutionData::try_new(owner_id, [], []).unwrap(),
            )
            .unwrap();

        let callable = CallableInstanceData::new(
            CallableDefinitionId::try_new(function.into()).unwrap(),
            substitution,
        );

        for (parameter, result, borrow, valid) in [
            (
                policy_borrow,
                target_borrow,
                Some(BorrowKind::Mutable),
                true,
            ),
            (shared_policy, shared_target, Some(BorrowKind::Shared), true),
            (
                policy_borrow,
                target_borrow,
                Some(BorrowKind::Shared),
                false,
            ),
            (
                shared_policy,
                shared_target,
                Some(BorrowKind::Mutable),
                false,
            ),
            (policy, target_borrow, Some(BorrowKind::Mutable), false),
            (
                policy_borrow,
                shared_target,
                Some(BorrowKind::Mutable),
                false,
            ),
            (
                policy_borrow,
                policy_borrow,
                Some(BorrowKind::Mutable),
                false,
            ),
            (policy, unit, None, true),
            (policy, policy, None, false),
            (target, unit, None, false),
        ] {
            let signature = policy_callable_type(
                &values,
                [parameter],
                result,
                bray_symbols::CallableAbi::Bray,
                bray_symbols::CallableExecution::Synchronous,
                false,
            );

            let call = StorageProtocolCall::new(callable, signature, parameter, result);

            assert_eq!(
                super::storage_protocol_call_matches(&values, symbols, &owner, call, borrow)
                    .unwrap(),
                valid
            );

            assert!(
                !super::storage_protocol_call_matches(
                    &values,
                    symbols,
                    &TypeData::Nullable(target),
                    call,
                    borrow
                )
                .unwrap()
            );
        }

        for (parameters, result, abi, execution, variadic) in [
            (
                vec![policy_borrow],
                target_borrow,
                bray_symbols::CallableAbi::C,
                bray_symbols::CallableExecution::Synchronous,
                false,
            ),
            (
                vec![policy_borrow],
                target_borrow,
                bray_symbols::CallableAbi::System,
                bray_symbols::CallableExecution::Synchronous,
                false,
            ),
            (
                vec![policy_borrow],
                target_borrow,
                bray_symbols::CallableAbi::Bray,
                bray_symbols::CallableExecution::Asynchronous,
                false,
            ),
            (
                vec![policy_borrow],
                target_borrow,
                bray_symbols::CallableAbi::Bray,
                bray_symbols::CallableExecution::Synchronous,
                true,
            ),
            (
                vec![],
                target_borrow,
                bray_symbols::CallableAbi::Bray,
                bray_symbols::CallableExecution::Synchronous,
                false,
            ),
            (
                vec![policy_borrow, policy_borrow],
                target_borrow,
                bray_symbols::CallableAbi::Bray,
                bray_symbols::CallableExecution::Synchronous,
                false,
            ),
            (
                vec![shared_policy],
                target_borrow,
                bray_symbols::CallableAbi::Bray,
                bray_symbols::CallableExecution::Synchronous,
                false,
            ),
            (
                vec![policy_borrow],
                shared_target,
                bray_symbols::CallableAbi::Bray,
                bray_symbols::CallableExecution::Synchronous,
                false,
            ),
        ] {
            let signature =
                policy_callable_type(&values, parameters, result, abi, execution, variadic);

            let call = StorageProtocolCall::new(callable, signature, policy_borrow, target_borrow);

            assert!(
                !super::storage_protocol_call_matches(
                    &values,
                    symbols,
                    &owner,
                    call,
                    Some(BorrowKind::Mutable)
                )
                .unwrap()
            );
        }

        let call = StorageProtocolCall::new(callable, policy, policy_borrow, target_borrow);

        assert!(
            !super::storage_protocol_call_matches(
                &values,
                symbols,
                &owner,
                call,
                Some(BorrowKind::Mutable)
            )
            .unwrap()
        );
    }

    #[test]
    fn cleanup_projection_types_reject_wrong_components_results_and_ordinals() {
        use bray_bound_tree::{StorageCleanupProjection, StorageProjection};
        use bray_symbols::{SemanticValueStore, SymbolOrdinal, TypeData};

        let values = SemanticValueStore::try_new().unwrap();
        let unit = values.intern_type(TypeData::tuple([])).unwrap();
        let nullable = values.intern_type(TypeData::Nullable(unit)).unwrap();
        let tuple = TypeData::tuple([unit, nullable]);
        let tuple_id = values.intern_type(tuple.clone()).unwrap();

        for (component, result, valid) in [
            (
                StorageProjection::TupleElement(SymbolOrdinal::new(0)),
                unit,
                true,
            ),
            (
                StorageProjection::TupleElement(SymbolOrdinal::new(1)),
                nullable,
                true,
            ),
            (
                StorageProjection::TupleElement(SymbolOrdinal::new(1)),
                unit,
                false,
            ),
            (
                StorageProjection::TupleElement(SymbolOrdinal::new(2)),
                unit,
                false,
            ),
            (StorageProjection::NullableValue, unit, false),
        ] {
            assert_eq!(
                super::cleanup_projection_matches_type(
                    StorageCleanupProjection::new(component, tuple_id, result),
                    &tuple,
                ),
                valid,
            );
        }

        for (result, valid) in [(unit, true), (nullable, false)] {
            assert_eq!(
                super::cleanup_projection_matches_type(
                    StorageCleanupProjection::new(
                        StorageProjection::NullableValue,
                        nullable,
                        result
                    ),
                    &TypeData::Nullable(unit),
                ),
                valid,
            );
        }
    }

    #[test]
    fn array_cleanup_projections_require_the_checked_extent_and_element_type() {
        use bray_bound_tree::{StorageCleanupProjection, StorageCleanupProjectionKind};
        use bray_symbols::{ConstantTermData, SemanticValueStore, SymbolOrdinal, TypeData};

        let values = SemanticValueStore::try_new().unwrap();
        let element = values.intern_type(TypeData::tuple([])).unwrap();
        let other = values.intern_type(TypeData::Nullable(element)).unwrap();

        let length = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let other_length = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(1)))
            .unwrap();

        let array = TypeData::Array { element, length };
        let array_type = values.intern_type(array.clone()).unwrap();

        for (extent, result, valid) in [
            (length, element, true),
            (other_length, element, false),
            (length, other, false),
        ] {
            let projection = StorageCleanupProjection::new(
                StorageCleanupProjectionKind::ArrayElements(extent),
                array_type,
                result,
            );

            assert_eq!(
                super::cleanup_projection_matches_type(projection, &array),
                valid
            );

            assert!(!super::cleanup_projection_matches_type(
                projection,
                &TypeData::Nullable(element)
            ));
        }
    }

    use bray_bound_tree::{
        AnyBoundNodeId, AsyncCleanupPhases, AsyncScopeExitPlan, AsyncStorageCleanupRequirement,
        AsyncStorageExitDecision, AsyncStorageExitDisposition, AsyncStorageExitRecoveryCause,
        AsyncStorageRequirement, BoundAwaitExpression, BoundBlock, BoundBlockId, BoundBlockItem,
        BoundDependencyContract, BoundDependencyRequirement, BoundDependencyRequirementKind,
        BoundDependencySubject, BoundExpression, BoundExpressionId, BoundNodeOrigin,
        BoundStructuredExpression, BoundStructuredExpressionKind, BoundTreeBuilder, BoundUnit,
        CheckedAsync, CheckedDependencyContracts, CheckedExpressionTypes,
        CheckedSemanticSelections, LiveAcrossSuspension, Liveness, StorageAccess, StorageAccessId,
        StorageAccessRoot, StorageExitDecision, StorageExitPoint, StorageFlow, StorageIdentity,
        StorageIdentityId, StoragePlan, StoragePlanBuilder, StorageProjection,
    };
    use bray_symbols::testing::available_compiler_known_symbols;
    use bray_symbols::{SemanticValueStore, SymbolOrdinal, TypeData};
    use bray_testing::test_runtime_default_unit;

    use crate::plan::{
        CleanupPlanLookupError, LoweringPlanFailure, LoweringPlanFailureCause, LoweringPlanKind,
        ScopeExitCleanupStatus, VerifiedLoweringPlans,
    };

    struct ScopeExitFixture {
        unit: BoundUnit,
        scope: BoundBlockId,
        exit: AnyBoundNodeId,
        storage: StoragePlan,
        liveness: Liveness,
        flow: StorageFlow,
        dependencies: CheckedDependencyContracts,
        selections: CheckedSemanticSelections,
        first: StorageIdentityId,
        second: StorageIdentityId,
        first_access: StorageAccessId,
        second_access: StorageAccessId,
        second_projected_access: StorageAccessId,
        intermediate: StorageIdentityId,
        intermediate_access: StorageAccessId,
    }

    impl ScopeExitFixture {
        fn new() -> Self {
            let mut expressions = None;
            let mut scope = None;

            let unit = test_runtime_default_unit(93, |tree, origin| {
                let first = push_unit_expression(tree, origin);
                let second = push_unit_expression(tree, origin);

                let block = tree
                    .push_block(BoundBlock::new(
                        origin,
                        [
                            BoundBlockItem::Expression(first),
                            BoundBlockItem::Expression(second),
                        ],
                        false,
                    ))
                    .unwrap_or_else(|error| panic!("test block must fit: {error:?}"));

                expressions = Some([first, second]);
                scope = Some(block);

                tree.push_expression(BoundExpression::Structured(BoundStructuredExpression::new(
                    origin,
                    BoundStructuredExpressionKind::Unit,
                    [],
                    [block],
                    [],
                    None,
                    false,
                )))
                .unwrap_or_else(|error| panic!("test root expression must fit: {error:?}"))
            });

            let [first_expression, second_expression] =
                expressions.unwrap_or_else(|| panic!("test expressions must be captured"));

            let scope = scope.unwrap_or_else(|| panic!("test scope must be captured"));
            let exit = AnyBoundNodeId::Expression(second_expression);

            let values = SemanticValueStore::try_new()
                .unwrap_or_else(|error| panic!("test semantic values must initialize: {error:?}"));

            let ty = values
                .intern_type(TypeData::tuple([]))
                .unwrap_or_else(|error| panic!("test type must intern: {error:?}"));

            let mut storage = StoragePlanBuilder::new(unit.unit(), unit.key().kind());

            let first = storage
                .push_identity(StorageIdentity::Temporary(first_expression))
                .unwrap_or_else(|error| panic!("first test identity must fit: {error:?}"));

            let second = storage
                .push_identity(StorageIdentity::Temporary(second_expression))
                .unwrap_or_else(|error| panic!("second test identity must fit: {error:?}"));

            let source = unit
                .view()
                .expression(first_expression)
                .map(BoundExpression::origin)
                .map(BoundNodeOrigin::source_anchor)
                .unwrap_or_else(|| panic!("test source anchor must exist"));

            let first_access = push_access(&mut storage, first, ty, source);
            let second_access = push_access(&mut storage, second, ty, source);

            let second_projected_access = storage
                .push_access(StorageAccess::new(
                    StorageAccessRoot::Storage(second),
                    [StorageProjection::TupleElement(SymbolOrdinal::new(0))],
                    ty,
                    source,
                    false,
                ))
                .unwrap_or_else(|error| panic!("projected test access must fit: {error:?}"));

            let intermediate_expression = unit.tree().expressions().last().unwrap().0;

            let intermediate = storage
                .push_identity(StorageIdentity::Temporary(intermediate_expression))
                .unwrap();

            let intermediate_access = push_access(&mut storage, intermediate, ty, source);
            let storage = storage.finish();

            let flow = StorageFlow::try_new(
                unit.unit(),
                unit.key().kind(),
                [],
                [],
                [StorageExitPoint::new(scope, exit)],
                [StorageExitDecision::new(
                    scope,
                    exit,
                    [first, second],
                    [first, second],
                    [],
                    [],
                    [],
                    [],
                    false,
                )],
                false,
            )
            .unwrap_or_else(|error| panic!("test storage flow must build: {error:?}"));

            let liveness = Liveness::try_new(unit.unit(), unit.key().kind(), [], [], [], [], false)
                .unwrap_or_else(|error| panic!("test liveness must build: {error:?}"));

            let expressions = unit
                .tree()
                .expressions()
                .map(|(expression, _)| (expression, BoundDependencyContract::new([])));

            let accesses = storage
                .access_entries()
                .map(|(access, _)| (access, BoundDependencyContract::new([])));

            let dependencies = CheckedDependencyContracts::try_new(
                &unit,
                &storage,
                expressions,
                [],
                accesses,
                [],
                false,
            )
            .unwrap_or_else(|error| panic!("test dependencies must build: {error:?}"));

            let types = CheckedExpressionTypes::new(unit.unit(), unit.key().kind(), []);

            let selections = CheckedSemanticSelections::try_new(&unit, &types, [])
                .unwrap_or_else(|error| panic!("test selections must build: {error:?}"));

            Self {
                unit,
                scope,
                exit,
                storage,
                liveness,
                flow,
                dependencies,
                selections,
                first,
                second,
                first_access,
                second_access,
                second_projected_access,
                intermediate,
                intermediate_access,
            }
        }

        fn plan(
            &self,
            storage: impl IntoIterator<Item = AsyncStorageExitDecision>,
            cancellation: impl IntoIterator<Item = StorageAccessId>,
            lifecycle: impl IntoIterator<Item = StorageAccessId>,
            recovered: bool,
        ) -> AsyncScopeExitPlan {
            self.plan_with_moved(storage, cancellation, lifecycle, [], recovered)
        }

        fn plan_with_moved(
            &self,
            storage: impl IntoIterator<Item = AsyncStorageExitDecision>,
            cancellation: impl IntoIterator<Item = StorageAccessId>,
            lifecycle: impl IntoIterator<Item = StorageAccessId>,
            moved: impl IntoIterator<Item = StorageAccessId>,
            recovered: bool,
        ) -> AsyncScopeExitPlan {
            AsyncScopeExitPlan::new(
                self.scope,
                self.exit,
                storage,
                cancellation,
                lifecycle,
                moved,
                recovered,
            )
        }

        fn analysis(
            &self,
            exits: impl IntoIterator<Item = AsyncScopeExitPlan>,
            recovered: bool,
        ) -> CheckedAsync {
            self.analysis_with_requirements(self.complete_requirements(), exits, recovered)
        }

        fn analysis_with_requirements(
            &self,
            requirements: impl IntoIterator<Item = AsyncStorageRequirement>,
            exits: impl IntoIterator<Item = AsyncScopeExitPlan>,
            recovered: bool,
        ) -> CheckedAsync {
            let ty = self.storage.storage_type(self.second).unwrap();

            let cleanup = bray_bound_tree::StorageCleanupType::new(
                ty,
                AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::Lifecycle),
            )
            .with_components(
                [0, 1].map(|index| {
                    bray_bound_tree::StorageCleanupProjection::new(
                        StorageProjection::TupleElement(SymbolOrdinal::new(index)),
                        ty,
                        ty,
                    )
                }),
                None,
                false,
            );

            CheckedAsync::try_new(
                self.unit.unit(),
                self.unit.key().kind(),
                [],
                [],
                [],
                requirements,
                [cleanup],
                exits,
                recovered,
            )
            .unwrap_or_else(|error| panic!("test async analysis must build: {error:?}"))
        }

        fn verify<'analysis>(
            &'analysis self,
            analysis: &'analysis CheckedAsync,
        ) -> Result<VerifiedLoweringPlans<'analysis>, LoweringPlanFailure> {
            self.verify_with_flow(&self.flow, analysis)
        }

        fn verify_with_flow<'analysis>(
            &'analysis self,
            flow: &'analysis StorageFlow,
            analysis: &'analysis CheckedAsync,
        ) -> Result<VerifiedLoweringPlans<'analysis>, LoweringPlanFailure> {
            VerifiedLoweringPlans::try_new(
                &self.unit,
                &self.storage,
                &self.liveness,
                flow,
                &self.dependencies,
                &self.selections,
                available_compiler_known_symbols(),
                analysis,
            )
        }

        fn complete_decisions(&self) -> [AsyncStorageExitDecision; 2] {
            [
                AsyncStorageExitDecision::new(self.second, AsyncStorageExitDisposition::NoCleanup),
                AsyncStorageExitDecision::new(self.first, AsyncStorageExitDisposition::NoCleanup),
            ]
        }

        fn complete_requirements(&self) -> [AsyncStorageRequirement; 2] {
            [
                AsyncStorageRequirement::new(
                    self.first,
                    Some(self.scope),
                    false,
                    AsyncStorageCleanupRequirement::None,
                ),
                AsyncStorageRequirement::new(
                    self.second,
                    Some(self.scope),
                    false,
                    AsyncStorageCleanupRequirement::None,
                ),
            ]
        }
    }

    #[test]
    fn complete_scope_exit_plans_publish_direct_lookup() {
        let fixture = ScopeExitFixture::new();

        let analysis = fixture.analysis(
            [fixture.plan(fixture.complete_decisions(), [], [], false)],
            false,
        );

        let plans = fixture
            .verify(&analysis)
            .unwrap_or_else(|error| panic!("complete plans must verify: {error:?}"));

        assert_eq!(
            plans
                .cleanup_plans(&[fixture.scope], 0, fixture.exit)
                .map(|plans| plans.len()),
            Ok(1)
        );

        assert_eq!(
            plans.scope_cleanup_status(fixture.scope, fixture.exit),
            Ok(ScopeExitCleanupStatus::NoCleanup)
        );

        assert_eq!(
            plans.cleanup_plans(&[fixture.scope], 2, fixture.exit),
            Err(CleanupPlanLookupError::InvalidScopeDepth {
                scope_depth: 2,
                active_scope_count: 1,
                exit: fixture.exit,
            })
        );

        assert_eq!(
            plans.cleanup_plans(&[fixture.scope], 0, fixture.scope.into()),
            Err(CleanupPlanLookupError::MissingScopeExit {
                scope: fixture.scope,
                exit: fixture.scope.into(),
            })
        );

        assert_eq!(
            plans.scope_cleanup_status(fixture.scope, fixture.scope.into()),
            Ok(ScopeExitCleanupStatus::Unreachable)
        );
    }

    #[test]
    fn storage_recovery_causes_survive_summary_recovery() {
        let fixture = ScopeExitFixture::new();

        for cause in [
            AsyncStorageExitRecoveryCause::UnavailableRootAccess,
            AsyncStorageExitRecoveryCause::UnavailableCleanupShape,
            AsyncStorageExitRecoveryCause::UnavailablePartialCleanup,
            AsyncStorageExitRecoveryCause::UnavailableCleanupOrder,
        ] {
            let mut requirements = fixture.complete_requirements();

            requirements[1] = AsyncStorageRequirement::new(
                fixture.second,
                Some(fixture.scope),
                false,
                AsyncStorageCleanupRequirement::Recovered(cause),
            );

            let analysis = fixture.analysis_with_requirements(
                requirements,
                [fixture.plan(fixture.complete_decisions(), [], [], true)],
                true,
            );

            assert_plan_failure(
                fixture.verify(&analysis),
                LoweringPlanKind::StorageDisposition,
                LoweringPlanFailureCause::StorageRecovery(cause),
                Some(fixture.second),
            );

            let analysis = fixture.analysis(
                [fixture.plan(
                    [
                        AsyncStorageExitDecision::new(
                            fixture.second,
                            AsyncStorageExitDisposition::Recovered(cause),
                        ),
                        AsyncStorageExitDecision::new(
                            fixture.first,
                            AsyncStorageExitDisposition::NoCleanup,
                        ),
                    ],
                    [],
                    [],
                    true,
                )],
                true,
            );

            assert_plan_failure(
                fixture.verify(&analysis),
                LoweringPlanKind::StorageDisposition,
                LoweringPlanFailureCause::StorageRecovery(cause),
                Some(fixture.second),
            );
        }
    }

    #[test]
    fn lifecycle_order_respects_guarded_dependencies_and_rejects_cycles() {
        let mut fixture = ScopeExitFixture::new();

        for cyclic in [false, true] {
            fixture.dependencies = CheckedDependencyContracts::try_new(
                &fixture.unit,
                &fixture.storage,
                fixture
                    .unit
                    .tree()
                    .expressions()
                    .map(|(id, _)| (id, BoundDependencyContract::new([]))),
                [],
                fixture.storage.access_entries().map(|(access, _)| {
                    let target = if access == fixture.first_access {
                        Some(fixture.intermediate)
                    } else if access == fixture.intermediate_access {
                        Some(fixture.second)
                    } else if cyclic && access == fixture.second_access {
                        Some(fixture.first)
                    } else {
                        None
                    };

                    let requirements = target.map(|identity| {
                        BoundDependencyRequirement::guarded(
                            bray_bound_tree::BoundDependencyGuard::NullablePresent(access),
                            [BoundDependencyRequirement::direct(
                                BoundDependencySubject::Storage(identity),
                                BoundDependencyRequirementKind::StorageAlive,
                            )],
                        )
                    });

                    (access, BoundDependencyContract::new(requirements))
                }),
                [],
                false,
            )
            .unwrap();

            let requirements = [fixture.first, fixture.second].map(|identity| {
                AsyncStorageRequirement::new(
                    identity,
                    Some(fixture.scope),
                    false,
                    AsyncStorageCleanupRequirement::Cleanup(
                        AsyncCleanupPhases::CancellationThenLifecycle,
                    ),
                )
            });

            let decisions = [
                (fixture.second, fixture.second_access),
                (fixture.first, fixture.first_access),
            ]
            .map(|(identity, access)| {
                AsyncStorageExitDecision::new(
                    identity,
                    AsyncStorageExitDisposition::Cleanup {
                        access,
                        phases: AsyncCleanupPhases::CancellationThenLifecycle,
                        guard: bray_bound_tree::AsyncCleanupGuard::Always,
                    },
                )
            });

            let reversed = fixture.analysis_with_requirements(
                requirements.clone(),
                [fixture.plan(
                    decisions,
                    [fixture.second_access, fixture.first_access],
                    [fixture.second_access, fixture.first_access],
                    false,
                )],
                false,
            );

            assert!(fixture.verify(&reversed).is_err());

            let ordered = fixture.analysis_with_requirements(
                requirements,
                [fixture.plan(
                    decisions,
                    [fixture.second_access, fixture.first_access],
                    [fixture.first_access, fixture.second_access],
                    false,
                )],
                false,
            );

            if cyclic {
                assert_plan_failure(
                    fixture.verify(&ordered),
                    LoweringPlanKind::LifecyclePhase,
                    LoweringPlanFailureCause::OutOfOrder,
                    None,
                );
            } else {
                assert!(fixture.verify(&ordered).is_ok());
            }
        }
    }

    #[test]
    fn aggregate_recovery_flags_are_rejected() {
        let fixture = ScopeExitFixture::new();

        let analysis = fixture.analysis(
            [fixture.plan(fixture.complete_decisions(), [], [], false)],
            true,
        );

        assert_plan_failure(
            fixture.verify(&analysis),
            LoweringPlanKind::Analysis,
            LoweringPlanFailureCause::Recovered,
            None,
        );
    }

    #[test]
    fn suspension_verification_rejects_an_omitted_live_subject() {
        let mut expressions = None;

        let unit = test_runtime_default_unit(94, |tree, origin| {
            let operand = push_unit_expression(tree, origin);

            let suspension = tree
                .push_expression(BoundExpression::Await(BoundAwaitExpression::pending(
                    origin, operand, false,
                )))
                .unwrap_or_else(|error| panic!("test suspension must fit: {error:?}"));

            expressions = Some((operand, suspension));

            suspension
        });

        let (operand, suspension) =
            expressions.unwrap_or_else(|| panic!("test suspension must be captured"));

        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("test semantic values must initialize: {error:?}"));

        let ty = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("test type must intern: {error:?}"));

        let mut storage = StoragePlanBuilder::new(unit.unit(), unit.key().kind());

        let identity = storage
            .push_identity(StorageIdentity::Temporary(operand))
            .unwrap_or_else(|error| panic!("test identity must fit: {error:?}"));

        let source = unit
            .view()
            .expression(operand)
            .map(BoundExpression::origin)
            .map(BoundNodeOrigin::source_anchor)
            .unwrap_or_else(|| panic!("test source anchor must exist"));

        let access = push_access(&mut storage, identity, ty, source);
        let storage = storage.finish();

        let contract = BoundDependencyContract::new([]);

        let dependencies = CheckedDependencyContracts::try_new(
            &unit,
            &storage,
            [(operand, contract.clone()), (suspension, contract.clone())],
            [],
            [(access, contract)],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("test dependencies must build: {error:?}"));

        let liveness = Liveness::try_new(
            unit.unit(),
            unit.key().kind(),
            [],
            [],
            [LiveAcrossSuspension::new(
                suspension,
                BoundDependencySubject::Storage(identity),
            )],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("test liveness must build: {error:?}"));

        let flow = StorageFlow::try_new(unit.unit(), unit.key().kind(), [], [], [], [], false)
            .unwrap_or_else(|error| panic!("test flow must build: {error:?}"));

        let types = CheckedExpressionTypes::new(unit.unit(), unit.key().kind(), []);

        let selections = CheckedSemanticSelections::try_new(&unit, &types, [])
            .unwrap_or_else(|error| panic!("test selections must build: {error:?}"));

        let analysis = CheckedAsync::try_new(
            unit.unit(),
            unit.key().kind(),
            [],
            [bray_bound_tree::AsyncSuspensionPoint::new(
                suspension,
                bray_bound_tree::AsyncSuspensionKind::Await { operand },
                None,
                [],
                [],
                false,
            )],
            [],
            [],
            [],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("test async analysis must build: {error:?}"));

        let result = VerifiedLoweringPlans::try_new(
            &unit,
            &storage,
            &liveness,
            &flow,
            &dependencies,
            &selections,
            available_compiler_known_symbols(),
            &analysis,
        );

        assert_plan_failure(
            result,
            LoweringPlanKind::Suspension,
            LoweringPlanFailureCause::Contradictory,
            None,
        );
    }

    #[test]
    fn suspension_verification_rejects_a_substituted_dependency_contract() {
        let mut expressions = None;

        let unit = test_runtime_default_unit(95, |tree, origin| {
            let operand = push_unit_expression(tree, origin);

            let suspension = tree
                .push_expression(BoundExpression::Await(BoundAwaitExpression::pending(
                    origin, operand, false,
                )))
                .unwrap_or_else(|error| panic!("test suspension must fit: {error:?}"));

            expressions = Some((operand, suspension));

            suspension
        });

        let (operand, suspension) =
            expressions.unwrap_or_else(|| panic!("test suspension must be captured"));

        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("test semantic values must initialize: {error:?}"));

        let ty = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("test type must intern: {error:?}"));

        let mut storage = StoragePlanBuilder::new(unit.unit(), unit.key().kind());

        let identity = storage
            .push_identity(StorageIdentity::Temporary(operand))
            .unwrap_or_else(|error| panic!("test identity must fit: {error:?}"));

        let source = unit
            .view()
            .expression(operand)
            .map(BoundExpression::origin)
            .map(BoundNodeOrigin::source_anchor)
            .unwrap_or_else(|| panic!("test source anchor must exist"));

        let access = push_access(&mut storage, identity, ty, source);
        let storage = storage.finish();
        let empty = BoundDependencyContract::new([]);

        let retained = BoundDependencyContract::new([BoundDependencyRequirement::direct(
            BoundDependencySubject::Storage(identity),
            BoundDependencyRequirementKind::StorageAlive,
        )]);

        let dependencies = CheckedDependencyContracts::try_new(
            &unit,
            &storage,
            [(operand, empty.clone()), (suspension, empty)],
            [
                (operand, BoundDependencyContract::new([])),
                (suspension, retained),
            ],
            [(access, BoundDependencyContract::new([]))],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("test dependencies must build: {error:?}"));

        let substituted = dependencies
            .deferred_expression(suspension)
            .unwrap_or_else(|| panic!("substituted contract must exist"));

        let liveness = Liveness::try_new(unit.unit(), unit.key().kind(), [], [], [], [], false)
            .unwrap_or_else(|error| panic!("test liveness must build: {error:?}"));

        let flow = StorageFlow::try_new(unit.unit(), unit.key().kind(), [], [], [], [], false)
            .unwrap_or_else(|error| panic!("test flow must build: {error:?}"));

        let types = CheckedExpressionTypes::new(unit.unit(), unit.key().kind(), []);

        let selections = CheckedSemanticSelections::try_new(&unit, &types, [])
            .unwrap_or_else(|error| panic!("test selections must build: {error:?}"));

        let analysis = CheckedAsync::try_new(
            unit.unit(),
            unit.key().kind(),
            [BoundDependencySubject::Storage(identity)],
            [bray_bound_tree::AsyncSuspensionPoint::new(
                suspension,
                bray_bound_tree::AsyncSuspensionKind::Await { operand },
                Some(substituted),
                [],
                [BoundDependencySubject::Storage(identity)],
                false,
            )],
            [],
            [],
            [],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("test async analysis must build: {error:?}"));

        let result = VerifiedLoweringPlans::try_new(
            &unit,
            &storage,
            &liveness,
            &flow,
            &dependencies,
            &selections,
            available_compiler_known_symbols(),
            &analysis,
        );

        assert_plan_failure(
            result,
            LoweringPlanKind::Suspension,
            LoweringPlanFailureCause::Contradictory,
            None,
        );
    }

    #[test]
    fn exact_recovered_scope_exit_inputs_are_rejected() {
        let fixture = ScopeExitFixture::new();

        let recovered_plan = fixture.analysis(
            [fixture.plan(fixture.complete_decisions(), [], [], true)],
            false,
        );

        assert_plan_failure(
            fixture.verify(&recovered_plan),
            LoweringPlanKind::ScopeExit,
            LoweringPlanFailureCause::Recovered,
            None,
        );

        let recovered_flow = StorageFlow::try_new(
            fixture.unit.unit(),
            fixture.unit.key().kind(),
            [],
            [],
            [StorageExitPoint::new(fixture.scope, fixture.exit)],
            [StorageExitDecision::new(
                fixture.scope,
                fixture.exit,
                [fixture.first, fixture.second],
                [fixture.first, fixture.second],
                [],
                [],
                [],
                [],
                true,
            )],
            false,
        )
        .unwrap_or_else(|error| panic!("recovered test flow must build: {error:?}"));

        let complete = fixture.analysis(
            [fixture.plan(fixture.complete_decisions(), [], [], false)],
            false,
        );

        assert_plan_failure(
            fixture.verify_with_flow(&recovered_flow, &complete),
            LoweringPlanKind::ScopeExit,
            LoweringPlanFailureCause::Recovered,
            None,
        );
    }

    #[test]
    fn scope_exit_verification_rejects_missing_and_duplicate_rows() {
        let fixture = ScopeExitFixture::new();
        let missing = fixture.analysis([], false);

        assert_plan_failure(
            fixture.verify(&missing),
            LoweringPlanKind::ScopeExit,
            LoweringPlanFailureCause::Missing,
            None,
        );

        let first = fixture.plan(fixture.complete_decisions(), [], [], false);
        let duplicate = fixture.analysis([first.clone(), first], false);

        assert_plan_failure(
            fixture.verify(&duplicate),
            LoweringPlanKind::ScopeExit,
            LoweringPlanFailureCause::Duplicate,
            None,
        );
    }

    #[test]
    fn storage_dispositions_are_exhaustive_unique_and_ordered() {
        let fixture = ScopeExitFixture::new();

        let missing = fixture.analysis(
            [fixture.plan(
                [AsyncStorageExitDecision::new(
                    fixture.second,
                    AsyncStorageExitDisposition::NoCleanup,
                )],
                [],
                [],
                false,
            )],
            false,
        );

        assert_plan_failure(
            fixture.verify(&missing),
            LoweringPlanKind::StorageDisposition,
            LoweringPlanFailureCause::Missing,
            Some(fixture.first),
        );

        let duplicate = fixture.analysis(
            [fixture.plan(
                [
                    AsyncStorageExitDecision::new(
                        fixture.second,
                        AsyncStorageExitDisposition::NoCleanup,
                    ),
                    AsyncStorageExitDecision::new(
                        fixture.second,
                        AsyncStorageExitDisposition::NoCleanup,
                    ),
                    AsyncStorageExitDecision::new(
                        fixture.first,
                        AsyncStorageExitDisposition::NoCleanup,
                    ),
                ],
                [],
                [],
                false,
            )],
            false,
        );

        assert_plan_failure(
            fixture.verify(&duplicate),
            LoweringPlanKind::StorageDisposition,
            LoweringPlanFailureCause::Duplicate,
            Some(fixture.second),
        );

        let reversed = fixture.analysis(
            [fixture.plan(
                fixture.complete_decisions().into_iter().rev(),
                [],
                [],
                false,
            )],
            false,
        );

        assert_plan_failure(
            fixture.verify(&reversed),
            LoweringPlanKind::StorageDisposition,
            LoweringPlanFailureCause::OutOfOrder,
            Some(fixture.first),
        );
    }

    #[test]
    fn recovered_and_contradictory_storage_dispositions_are_rejected() {
        let fixture = ScopeExitFixture::new();

        let recovered = fixture.analysis(
            [fixture.plan(
                [
                    AsyncStorageExitDecision::new(
                        fixture.second,
                        AsyncStorageExitDisposition::Recovered(
                            AsyncStorageExitRecoveryCause::UnavailableCleanupShape,
                        ),
                    ),
                    AsyncStorageExitDecision::new(
                        fixture.first,
                        AsyncStorageExitDisposition::NoCleanup,
                    ),
                ],
                [],
                [],
                false,
            )],
            false,
        );

        assert_plan_failure(
            fixture.verify(&recovered),
            LoweringPlanKind::StorageDisposition,
            LoweringPlanFailureCause::StorageRecovery(
                AsyncStorageExitRecoveryCause::UnavailableCleanupShape,
            ),
            Some(fixture.second),
        );

        let contradictory = fixture.analysis_with_requirements(
            [
                AsyncStorageRequirement::new(
                    fixture.first,
                    Some(fixture.scope),
                    false,
                    AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::Lifecycle),
                ),
                AsyncStorageRequirement::new(
                    fixture.second,
                    Some(fixture.scope),
                    false,
                    AsyncStorageCleanupRequirement::Cleanup(
                        AsyncCleanupPhases::CancellationThenLifecycle,
                    ),
                ),
            ],
            [fixture.plan(
                [
                    AsyncStorageExitDecision::new(
                        fixture.second,
                        AsyncStorageExitDisposition::Cleanup {
                            access: fixture.second_access,
                            phases: AsyncCleanupPhases::CancellationThenLifecycle,
                            guard: bray_bound_tree::AsyncCleanupGuard::Always,
                        },
                    ),
                    AsyncStorageExitDecision::new(
                        fixture.first,
                        AsyncStorageExitDisposition::Cleanup {
                            access: fixture.first_access,
                            phases: AsyncCleanupPhases::Lifecycle,
                            guard: bray_bound_tree::AsyncCleanupGuard::Always,
                        },
                    ),
                ],
                [fixture.second_access],
                [fixture.first_access, fixture.second_access],
                false,
            )],
            false,
        );

        let Err(error) = fixture.verify(&contradictory) else {
            panic!("contradictory lifecycle phase must fail verification");
        };

        assert_eq!(error.kind(), LoweringPlanKind::LifecyclePhase);
        assert_eq!(error.cause(), LoweringPlanFailureCause::Contradictory);
        assert_eq!(error.access(), Some(fixture.first_access));

        let falsely_moved = fixture.analysis(
            [fixture.plan(
                [
                    AsyncStorageExitDecision::new(
                        fixture.second,
                        AsyncStorageExitDisposition::Moved,
                    ),
                    AsyncStorageExitDecision::new(
                        fixture.first,
                        AsyncStorageExitDisposition::NoCleanup,
                    ),
                ],
                [],
                [],
                false,
            )],
            false,
        );

        assert_plan_failure(
            fixture.verify(&falsely_moved),
            LoweringPlanKind::StorageDisposition,
            LoweringPlanFailureCause::Contradictory,
            Some(fixture.second),
        );
    }

    #[test]
    fn scope_exit_verification_rejects_storage_state_outside_the_exit_set() {
        let fixture = ScopeExitFixture::new();

        let flow = StorageFlow::try_new(
            fixture.unit.unit(),
            fixture.unit.key().kind(),
            [],
            [],
            [StorageExitPoint::new(fixture.scope, fixture.exit)],
            [StorageExitDecision::new(
                fixture.scope,
                fixture.exit,
                [fixture.first],
                [fixture.first],
                [],
                [],
                [fixture.second],
                [],
                false,
            )],
            false,
        )
        .unwrap_or_else(|error| panic!("test storage flow must build: {error:?}"));

        let analysis = fixture.analysis(
            [fixture.plan(
                [AsyncStorageExitDecision::new(
                    fixture.first,
                    AsyncStorageExitDisposition::NoCleanup,
                )],
                [],
                [],
                false,
            )],
            false,
        );

        assert_plan_failure(
            fixture.verify_with_flow(&flow, &analysis),
            LoweringPlanKind::StorageDisposition,
            LoweringPlanFailureCause::Unexpected,
            Some(fixture.second),
        );
    }

    #[test]
    fn trivial_partially_initialized_storage_has_no_cleanup() {
        let fixture = ScopeExitFixture::new();

        let flow = StorageFlow::try_new(
            fixture.unit.unit(),
            fixture.unit.key().kind(),
            [],
            [],
            [StorageExitPoint::new(fixture.scope, fixture.exit)],
            [StorageExitDecision::new(
                fixture.scope,
                fixture.exit,
                [fixture.first, fixture.second],
                [fixture.first],
                [],
                [],
                [],
                [],
                false,
            )],
            false,
        )
        .unwrap_or_else(|error| panic!("partial test flow must build: {error:?}"));

        let analysis = fixture.analysis(
            [fixture.plan(
                [
                    AsyncStorageExitDecision::new(
                        fixture.second,
                        AsyncStorageExitDisposition::NoCleanup,
                    ),
                    AsyncStorageExitDecision::new(
                        fixture.first,
                        AsyncStorageExitDisposition::NoCleanup,
                    ),
                ],
                [],
                [],
                false,
            )],
            false,
        );

        fixture
            .verify_with_flow(&flow, &analysis)
            .unwrap_or_else(|error| panic!("trivial partial storage must verify: {error:?}"));
    }

    #[test]
    fn recovered_partial_initialization_cannot_reach_lowering() {
        let fixture = ScopeExitFixture::new();

        let flow = StorageFlow::try_new(
            fixture.unit.unit(),
            fixture.unit.key().kind(),
            [],
            [],
            [StorageExitPoint::new(fixture.scope, fixture.exit)],
            [StorageExitDecision::new(
                fixture.scope,
                fixture.exit,
                [fixture.first, fixture.second],
                [fixture.first],
                [],
                [],
                [],
                [],
                false,
            )],
            false,
        )
        .unwrap_or_else(|error| panic!("partial test flow must build: {error:?}"));

        let analysis = fixture.analysis_with_requirements(
            [
                AsyncStorageRequirement::new(
                    fixture.first,
                    Some(fixture.scope),
                    false,
                    AsyncStorageCleanupRequirement::None,
                ),
                AsyncStorageRequirement::new(
                    fixture.second,
                    Some(fixture.scope),
                    false,
                    AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::Lifecycle),
                ),
            ],
            [fixture.plan(
                [
                    AsyncStorageExitDecision::new(
                        fixture.second,
                        AsyncStorageExitDisposition::Recovered(
                            AsyncStorageExitRecoveryCause::UnavailablePartialCleanup,
                        ),
                    ),
                    AsyncStorageExitDecision::new(
                        fixture.first,
                        AsyncStorageExitDisposition::NoCleanup,
                    ),
                ],
                [],
                [],
                true,
            )],
            false,
        );

        assert_plan_failure(
            fixture.verify_with_flow(&flow, &analysis),
            LoweringPlanKind::StorageDisposition,
            LoweringPlanFailureCause::StorageRecovery(
                AsyncStorageExitRecoveryCause::UnavailablePartialCleanup,
            ),
            Some(fixture.second),
        );
    }

    #[test]
    fn conditional_root_cleanup_requires_an_exact_initialization_guard() {
        use bray_bound_tree::AsyncCleanupGuard;

        let fixture = ScopeExitFixture::new();

        for moved in [Vec::new(), vec![fixture.second_access]] {
            let flow = StorageFlow::try_new(
                fixture.unit.unit(),
                fixture.unit.key().kind(),
                [],
                [],
                [StorageExitPoint::new(fixture.scope, fixture.exit)],
                [StorageExitDecision::new(
                    fixture.scope,
                    fixture.exit,
                    [fixture.first, fixture.second],
                    [fixture.first],
                    moved.iter().copied(),
                    [],
                    [],
                    [],
                    false,
                )],
                false,
            )
            .unwrap();

            for guard in [AsyncCleanupGuard::Initialized, AsyncCleanupGuard::Always] {
                let analysis = fixture.analysis_with_requirements(
                    [
                        AsyncStorageRequirement::new(
                            fixture.first,
                            Some(fixture.scope),
                            false,
                            AsyncStorageCleanupRequirement::None,
                        ),
                        AsyncStorageRequirement::new(
                            fixture.second,
                            Some(fixture.scope),
                            false,
                            AsyncStorageCleanupRequirement::Cleanup(
                                AsyncCleanupPhases::CancellationThenLifecycle,
                            ),
                        ),
                    ],
                    [fixture.plan_with_moved(
                        [
                            AsyncStorageExitDecision::new(
                                fixture.second,
                                AsyncStorageExitDisposition::Cleanup {
                                    access: fixture.second_access,
                                    phases: AsyncCleanupPhases::CancellationThenLifecycle,
                                    guard,
                                },
                            ),
                            AsyncStorageExitDecision::new(
                                fixture.first,
                                AsyncStorageExitDisposition::NoCleanup,
                            ),
                        ],
                        [fixture.second_access],
                        [fixture.second_access],
                        moved.iter().copied(),
                        false,
                    )],
                    false,
                );

                match guard {
                    AsyncCleanupGuard::Initialized => {
                        let verified = fixture.verify_with_flow(&flow, &analysis).unwrap();

                        assert_eq!(
                            verified.initialization_guards().collect::<Vec<_>>(),
                            [fixture.second_access]
                        );
                    }
                    AsyncCleanupGuard::Always => assert_plan_failure(
                        fixture.verify_with_flow(&flow, &analysis),
                        LoweringPlanKind::StorageDisposition,
                        LoweringPlanFailureCause::Contradictory,
                        Some(fixture.second),
                    ),
                }
            }
        }
    }

    #[test]
    fn fully_moved_storage_needs_no_cleanup_even_when_not_initialized() {
        let fixture = ScopeExitFixture::new();

        let flow = StorageFlow::try_new(
            fixture.unit.unit(),
            fixture.unit.key().kind(),
            [],
            [],
            [StorageExitPoint::new(fixture.scope, fixture.exit)],
            [StorageExitDecision::new(
                fixture.scope,
                fixture.exit,
                [fixture.first, fixture.second],
                [fixture.first],
                [fixture.second_access],
                [],
                [fixture.second],
                [],
                false,
            )],
            false,
        )
        .unwrap_or_else(|error| panic!("moved test flow must build: {error:?}"));

        let analysis = fixture.analysis_with_requirements(
            [
                AsyncStorageRequirement::new(
                    fixture.first,
                    Some(fixture.scope),
                    false,
                    AsyncStorageCleanupRequirement::None,
                ),
                AsyncStorageRequirement::new(
                    fixture.second,
                    Some(fixture.scope),
                    false,
                    AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::Lifecycle),
                ),
            ],
            [fixture.plan_with_moved(
                [
                    AsyncStorageExitDecision::new(
                        fixture.second,
                        AsyncStorageExitDisposition::Moved,
                    ),
                    AsyncStorageExitDecision::new(
                        fixture.first,
                        AsyncStorageExitDisposition::NoCleanup,
                    ),
                ],
                [],
                [],
                [fixture.second_access],
                false,
            )],
            false,
        );

        fixture
            .verify_with_flow(&flow, &analysis)
            .unwrap_or_else(|error| panic!("fully moved storage must verify: {error:?}"));
    }

    #[test]
    fn nontrivial_partially_moved_storage_requires_a_represented_partition() {
        let fixture = ScopeExitFixture::new();

        let flow = StorageFlow::try_new(
            fixture.unit.unit(),
            fixture.unit.key().kind(),
            [],
            [],
            [StorageExitPoint::new(fixture.scope, fixture.exit)],
            [StorageExitDecision::new(
                fixture.scope,
                fixture.exit,
                [fixture.first, fixture.second],
                [fixture.first, fixture.second],
                [fixture.second_projected_access],
                [],
                [],
                [],
                false,
            )],
            false,
        )
        .unwrap_or_else(|error| panic!("partial-move test flow must build: {error:?}"));

        let analysis = fixture.analysis_with_requirements(
            [
                AsyncStorageRequirement::new(
                    fixture.first,
                    Some(fixture.scope),
                    false,
                    AsyncStorageCleanupRequirement::None,
                ),
                AsyncStorageRequirement::new(
                    fixture.second,
                    Some(fixture.scope),
                    false,
                    AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::Lifecycle),
                ),
            ],
            [fixture.plan_with_moved(
                [
                    AsyncStorageExitDecision::new(
                        fixture.second,
                        AsyncStorageExitDisposition::Recovered(
                            AsyncStorageExitRecoveryCause::UnavailablePartialCleanup,
                        ),
                    ),
                    AsyncStorageExitDecision::new(
                        fixture.first,
                        AsyncStorageExitDisposition::NoCleanup,
                    ),
                ],
                [],
                [],
                [fixture.second_projected_access],
                true,
            )],
            false,
        );

        assert_plan_failure(
            fixture.verify_with_flow(&flow, &analysis),
            LoweringPlanKind::StorageDisposition,
            LoweringPlanFailureCause::StorageRecovery(
                AsyncStorageExitRecoveryCause::UnavailablePartialCleanup,
            ),
            Some(fixture.second),
        );
    }

    #[test]
    fn represented_part_plans_reject_overlaps_missing_phases_and_broken_types() {
        let fixture = ScopeExitFixture::new();
        let ty = fixture.storage.storage_type(fixture.second).unwrap();
        let values = SemanticValueStore::try_new().unwrap();

        let function =
            bray_symbols::FunctionSymbolId::from_symbol_id(bray_symbols::SymbolId::new(1));

        let owner = bray_symbols::GenericOwnerId::try_new(function.into()).unwrap();

        let substitution = values
            .intern_generic_substitution(
                bray_symbols::GenericSubstitutionData::try_new(owner, [], []).unwrap(),
            )
            .unwrap();

        let callable = bray_symbols::CallableInstanceData::new(
            bray_symbols::CallableDefinitionId::try_new(function.into()).unwrap(),
            substitution,
        );

        let release = bray_bound_tree::StorageCleanupPart::release_storage(
            [],
            bray_bound_tree::StorageProtocolCall::new(callable, ty, ty, ty),
        );

        let part = |index| {
            bray_bound_tree::StorageCleanupPart::new(
                [bray_bound_tree::StorageCleanupProjection::new(
                    StorageProjection::TupleElement(SymbolOrdinal::new(index)),
                    ty,
                    ty,
                )],
                AsyncCleanupPhases::Lifecycle,
            )
        };

        for (parts, valid) in [
            (vec![part(1), part(0)], true),
            (vec![part(1), part(0), release.clone()], false),
            (vec![release.clone(), part(0)], false),
            (vec![part(1), release, part(0)], false),
            (vec![part(0), part(0)], false),
            (Vec::new(), false),
            (
                vec![bray_bound_tree::StorageCleanupPart::new(
                    [],
                    AsyncCleanupPhases::Lifecycle,
                )],
                false,
            ),
        ] {
            let analysis = fixture.analysis_with_requirements(
                [
                    AsyncStorageRequirement::new(
                        fixture.first,
                        Some(fixture.scope),
                        false,
                        AsyncStorageCleanupRequirement::None,
                    ),
                    AsyncStorageRequirement::new(
                        fixture.second,
                        Some(fixture.scope),
                        false,
                        AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::Lifecycle),
                    )
                    .with_parts(parts),
                ],
                [fixture.plan(
                    [
                        AsyncStorageExitDecision::new(
                            fixture.second,
                            AsyncStorageExitDisposition::Cleanup {
                                access: fixture.second_access,
                                phases: AsyncCleanupPhases::Lifecycle,
                                guard: bray_bound_tree::AsyncCleanupGuard::Initialized,
                            },
                        ),
                        AsyncStorageExitDecision::new(
                            fixture.first,
                            AsyncStorageExitDisposition::NoCleanup,
                        ),
                    ],
                    [],
                    [fixture.second_access],
                    false,
                )],
                false,
            );

            let result = fixture.verify(&analysis);

            if valid {
                result.unwrap();
            } else {
                assert_plan_failure(
                    result,
                    LoweringPlanKind::StorageDisposition,
                    LoweringPlanFailureCause::Contradictory,
                    Some(fixture.second),
                );
            }
        }
    }

    #[test]
    fn storage_requirements_are_checked_independently_from_exit_dispositions() {
        let fixture = ScopeExitFixture::new();

        let fabricated = fixture.analysis_with_requirements(
            [
                AsyncStorageRequirement::new(
                    fixture.first,
                    None,
                    true,
                    AsyncStorageCleanupRequirement::None,
                ),
                AsyncStorageRequirement::new(
                    fixture.second,
                    Some(fixture.scope),
                    false,
                    AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::Lifecycle),
                ),
            ],
            [fixture.plan(
                [
                    AsyncStorageExitDecision::new(
                        fixture.second,
                        AsyncStorageExitDisposition::NoCleanup,
                    ),
                    AsyncStorageExitDecision::new(
                        fixture.first,
                        AsyncStorageExitDisposition::Transferred,
                    ),
                ],
                [],
                [],
                false,
            )],
            false,
        );

        assert_plan_failure(
            fixture.verify(&fabricated),
            LoweringPlanKind::StorageDisposition,
            LoweringPlanFailureCause::Contradictory,
            Some(fixture.first),
        );

        let suppressed_cleanup = fixture.analysis_with_requirements(
            [
                AsyncStorageRequirement::new(
                    fixture.first,
                    Some(fixture.scope),
                    false,
                    AsyncStorageCleanupRequirement::None,
                ),
                AsyncStorageRequirement::new(
                    fixture.second,
                    Some(fixture.scope),
                    false,
                    AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::Lifecycle),
                ),
            ],
            [fixture.plan(fixture.complete_decisions(), [], [], false)],
            false,
        );

        assert_plan_failure(
            fixture.verify(&suppressed_cleanup),
            LoweringPlanKind::StorageDisposition,
            LoweringPlanFailureCause::Contradictory,
            Some(fixture.second),
        );
    }

    #[test]
    fn scope_exit_verification_rejects_rows_outside_reachable_exit_set() {
        let fixture = ScopeExitFixture::new();

        let flow = StorageFlow::try_new(
            fixture.unit.unit(),
            fixture.unit.key().kind(),
            [],
            [],
            [],
            [fixture.flow.exits()[0].clone()],
            false,
        )
        .unwrap_or_else(|error| panic!("dead-exit test flow must build: {error:?}"));

        let analysis = fixture.analysis(
            [fixture.plan(fixture.complete_decisions(), [], [], false)],
            false,
        );

        assert_plan_failure(
            fixture.verify_with_flow(&flow, &analysis),
            LoweringPlanKind::ScopeExit,
            LoweringPlanFailureCause::Unexpected,
            None,
        );
    }

    fn assert_plan_failure(
        result: Result<VerifiedLoweringPlans<'_>, LoweringPlanFailure>,
        kind: LoweringPlanKind,
        cause: LoweringPlanFailureCause,
        storage: Option<StorageIdentityId>,
    ) {
        let Err(error) = result else {
            panic!("invalid plan must fail verification");
        };

        assert_eq!(error.kind(), kind);
        assert_eq!(error.cause(), cause);
        assert_eq!(error.storage(), storage);
    }

    fn push_unit_expression(
        tree: &mut BoundTreeBuilder,
        origin: BoundNodeOrigin,
    ) -> BoundExpressionId {
        tree.push_expression(BoundExpression::Structured(BoundStructuredExpression::new(
            origin,
            BoundStructuredExpressionKind::Unit,
            [],
            [],
            [],
            None,
            false,
        )))
        .unwrap_or_else(|error| panic!("test unit expression must fit: {error:?}"))
    }

    fn push_access(
        storage: &mut StoragePlanBuilder,
        identity: StorageIdentityId,
        ty: bray_symbols::TypeId,
        source: bray_bound_tree::BoundSourceAnchor,
    ) -> StorageAccessId {
        storage
            .push_access(StorageAccess::new(
                StorageAccessRoot::Storage(identity),
                [],
                ty,
                source,
                false,
            ))
            .unwrap_or_else(|error| panic!("test storage access must fit: {error:?}"))
    }
}
