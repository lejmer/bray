use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use bray_binder::{BinderDependency, CheckedUnitComputation};
use bray_bound_tree::{
    BoundUnitKey, BoundUnitKind, ControlFlowCheckedAnonymousCallable,
    ControlFlowCheckedCallableBody, ControlFlowCheckedConstantTemplateUnit,
    ControlFlowCheckedConstraintUnit, ControlFlowCheckedContractClauseUnit,
    ControlFlowCheckedPredicateDefinitionUnit, ControlFlowCheckedRuntimeDefaultUnit,
};
use bray_diagnostics::DiagnosticResult;

use super::{CancellationToken, CompilationFactKey, FactCell, FactQueryError, FactRuntime};

#[derive(Debug)]
pub(crate) struct PublishedUnit<T> {
    result: Arc<DiagnosticResult<T>>,
    // TODO(compilation): Remove this expectation when incremental invalidation traverses edges.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "incremental dependency invalidation is implemented by a subsequent issue"
        )
    )]
    dependencies: Box<[BinderDependency]>,
}

impl<T> PublishedUnit<T> {
    pub(crate) const fn result(&self) -> &Arc<DiagnosticResult<T>> {
        &self.result
    }

    // TODO(compilation): Remove this expectation when incremental invalidation traverses edges.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "incremental dependency invalidation is implemented by a subsequent issue"
        )
    )]
    pub(crate) fn dependencies(&self) -> &[BinderDependency] {
        &self.dependencies
    }
}

#[derive(Debug)]
pub(crate) struct UnitFactCache<T> {
    kind: BoundUnitKind,
    cells: Mutex<UnitCells<T>>,
}

type UnitCells<T> = BTreeMap<BoundUnitKey, Arc<FactCell<Arc<PublishedUnit<T>>>>>;

impl<T> UnitFactCache<T> {
    pub(crate) const fn new(kind: BoundUnitKind) -> Self {
        Self {
            kind,
            cells: Mutex::new(BTreeMap::new()),
        }
    }

    pub(crate) fn get_or_compute(
        &self,
        runtime: &FactRuntime,
        cancellation: &CancellationToken,
        key: BoundUnitKey,
        compute: impl FnOnce() -> Result<CheckedUnitComputation<T>, FactQueryError>,
    ) -> Result<Arc<PublishedUnit<T>>, FactQueryError> {
        if key.kind() != self.kind {
            return Err(FactQueryError::InfrastructureFailure);
        }

        let cell = self.cell(&key)?;
        let fact_key = CompilationFactKey::ControlFlowUnit(key);

        let published = cell.get_or_compute(runtime, fact_key, cancellation, || {
            let (result, dependencies) = compute()?.into_parts();

            Ok(Arc::new(PublishedUnit {
                result: Arc::new(result),
                dependencies,
            }))
        })?;

        // Publication must outlive the short-lived map and cell borrows returned by this query.
        Ok(Arc::clone(published))
    }

    fn cell(
        &self,
        key: &BoundUnitKey,
    ) -> Result<Arc<FactCell<Arc<PublishedUnit<T>>>>, FactQueryError> {
        let mut cells = self
            .cells
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        // The map owns one shared synchronization cell per exact immutable unit identity.
        Ok(Arc::clone(
            cells
                .entry(key.clone())
                .or_insert_with(|| Arc::new(FactCell::new())),
        ))
    }
}

#[derive(Debug)]
pub(crate) struct ControlFlowUnitFactCaches {
    callable_body: UnitFactCache<ControlFlowCheckedCallableBody>,
    anonymous_callable: UnitFactCache<ControlFlowCheckedAnonymousCallable>,
    runtime_default: UnitFactCache<ControlFlowCheckedRuntimeDefaultUnit>,
    constant_template: UnitFactCache<ControlFlowCheckedConstantTemplateUnit>,
    predicate_definition: UnitFactCache<ControlFlowCheckedPredicateDefinitionUnit>,
    constraint: UnitFactCache<ControlFlowCheckedConstraintUnit>,
    contract_clause: UnitFactCache<ControlFlowCheckedContractClauseUnit>,
}

impl ControlFlowUnitFactCaches {
    pub(crate) const fn new() -> Self {
        Self {
            callable_body: UnitFactCache::new(BoundUnitKind::CallableBody),
            anonymous_callable: UnitFactCache::new(BoundUnitKind::AnonymousCallable),
            runtime_default: UnitFactCache::new(BoundUnitKind::RuntimeDefault),
            constant_template: UnitFactCache::new(BoundUnitKind::ConstantTemplate),
            predicate_definition: UnitFactCache::new(BoundUnitKind::PredicateDefinition),
            constraint: UnitFactCache::new(BoundUnitKind::Constraint),
            contract_clause: UnitFactCache::new(BoundUnitKind::ContractClause),
        }
    }

    pub(crate) fn get_or_compute<T: ControlFlowUnitFact>(
        &self,
        runtime: &FactRuntime,
        cancellation: &CancellationToken,
        key: BoundUnitKey,
        compute: impl FnOnce() -> Result<CheckedUnitComputation<T>, FactQueryError>,
    ) -> Result<Arc<PublishedUnit<T>>, FactQueryError> {
        T::cache(self).get_or_compute(runtime, cancellation, key, compute)
    }

    #[cfg(test)]
    pub(crate) fn is_published<T: ControlFlowUnitFact>(
        &self,
        key: &BoundUnitKey,
    ) -> Result<bool, FactQueryError> {
        let cells = T::cache(self)
            .cells
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        Ok(cells.get(key).is_some_and(|cell| cell.get().is_some()))
    }
}

pub(crate) trait ControlFlowUnitFact: Sized {
    fn cache(caches: &ControlFlowUnitFactCaches) -> &UnitFactCache<Self>;
}

macro_rules! define_control_flow_unit_facts {
    ($($value:ty => $field:ident),+ $(,)?) => {
        $(
            impl ControlFlowUnitFact for $value {
                fn cache(caches: &ControlFlowUnitFactCaches) -> &UnitFactCache<Self> {
                    &caches.$field
                }
            }
        )+
    };
}

define_control_flow_unit_facts! {
    ControlFlowCheckedCallableBody => callable_body,
    ControlFlowCheckedAnonymousCallable => anonymous_callable,
    ControlFlowCheckedRuntimeDefaultUnit => runtime_default,
    ControlFlowCheckedConstantTemplateUnit => constant_template,
    ControlFlowCheckedPredicateDefinitionUnit => predicate_definition,
    ControlFlowCheckedConstraintUnit => constraint,
    ControlFlowCheckedContractClauseUnit => contract_clause,
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_bound_tree::{BoundUnitKey, BoundUnitKind};
    use bray_diagnostics::{
        Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticResult, SeverityKind,
    };

    use super::UnitFactCache;
    use crate::fact::{CancellationToken, FactQueryError, FactRuntime};
    use crate::test_support::{callable_body_key, constant_template_key};

    #[test]
    fn repeated_requests_publish_one_atomic_result() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();
        let cache = UnitFactCache::new(BoundUnitKind::CallableBody);
        let computations = AtomicUsize::new(0);

        let key = callable_body_key(0);

        let first = published(&cache, &runtime, &cancellation, key.clone(), || {
            computations.fetch_add(1, Ordering::SeqCst);

            checked(11)
        });

        let second = published(&cache, &runtime, &cancellation, key, || {
            computations.fetch_add(1, Ordering::SeqCst);

            checked(22)
        });

        assert!(std::sync::Arc::ptr_eq(&first, &second));
        assert_eq!(first.result().value(), &11);
        assert_eq!(computations.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn concurrent_requests_share_one_publication() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();
        let cache = UnitFactCache::new(BoundUnitKind::CallableBody);
        let computations = AtomicUsize::new(0);

        let key = callable_body_key(1);

        std::thread::scope(|scope| {
            let handles = (0..8)
                .map(|_| {
                    let key = key.clone();

                    scope.spawn(|| {
                        published(&cache, &runtime, &cancellation, key, || {
                            computations.fetch_add(1, Ordering::SeqCst);

                            checked(17)
                        })
                    })
                })
                .collect::<Vec<_>>();

            let values = handles
                .into_iter()
                .map(|handle| match handle.join() {
                    Ok(value) => value,
                    Err(_) => panic!("checked-unit request must not panic"),
                })
                .collect::<Vec<_>>();

            for value in &values[1..] {
                assert!(std::sync::Arc::ptr_eq(&values[0], value));
            }
        });

        assert_eq!(computations.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn cancellation_publishes_no_partial_control_flow_unit() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();
        let cache = UnitFactCache::new(BoundUnitKind::CallableBody);

        let key = callable_body_key(2);

        let cancelled = cache.get_or_compute(&runtime, &cancellation, key.clone(), || {
            cancellation.cancel();

            checked(3)
        });

        assert!(matches!(cancelled, Err(FactQueryError::Cancelled)));

        let retried = published(&cache, &runtime, &CancellationToken::new(), key, || {
            checked(4)
        });

        assert_eq!(retried.result().value(), &4);
    }

    #[test]
    fn exact_unit_identity_selects_cache_entries_and_cycle_paths() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();
        let cache = UnitFactCache::new(BoundUnitKind::CallableBody);

        let first_key = callable_body_key(3);
        let second_key = callable_body_key(4);

        let first = published(&cache, &runtime, &cancellation, first_key.clone(), || {
            checked(3)
        });

        let second = published(&cache, &runtime, &cancellation, second_key, || checked(4));

        assert_eq!(first.result().value(), &3);
        assert_eq!(second.result().value(), &4);

        let recursive_cache = UnitFactCache::new(BoundUnitKind::CallableBody);

        let recursive =
            recursive_cache.get_or_compute(&runtime, &cancellation, first_key.clone(), || {
                recursive_cache.get_or_compute(
                    &runtime,
                    &cancellation,
                    first_key.clone(),
                    || checked(9),
                )?;

                checked(8)
            });

        let Err(FactQueryError::Cycle(cycle)) = recursive else {
            panic!("recursive checked-unit request must report a cycle");
        };

        assert_eq!(
            cycle.facts(),
            &[
                crate::CompilationFactKey::ControlFlowUnit(first_key.clone()),
                crate::CompilationFactKey::ControlFlowUnit(first_key)
            ]
        );
    }

    #[test]
    fn nested_diagnostics_remain_owned_by_nested_facts() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();
        let cache = UnitFactCache::new(BoundUnitKind::CallableBody);

        let outer_key = callable_body_key(5);
        let nested_key = callable_body_key(6);

        let nested_diagnostic = diagnostic(1);

        let outer = published(&cache, &runtime, &cancellation, outer_key, || {
            cache.get_or_compute(&runtime, &cancellation, nested_key.clone(), || {
                Ok(bray_binder::CheckedUnitComputation::new(
                    DiagnosticResult::new(2, DiagnosticBag::single(nested_diagnostic.clone())),
                    [],
                ))
            })?;

            Ok(bray_binder::CheckedUnitComputation::new(
                DiagnosticResult::new(1, DiagnosticBag::single(diagnostic(2))),
                [bray_binder::BinderDependency::Unit(nested_key.clone())],
            ))
        });

        let nested = published(&cache, &runtime, &cancellation, nested_key.clone(), || {
            checked(3)
        });

        assert_eq!(
            nested.result().diagnostics().diagnostics(),
            &[nested_diagnostic]
        );

        assert_eq!(outer.result().diagnostics().diagnostics(), &[diagnostic(2)]);

        assert_eq!(
            outer.dependencies(),
            &[bray_binder::BinderDependency::Unit(nested_key)]
        );
    }

    #[test]
    fn caches_reject_unit_categories_owned_by_another_typed_fact() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();
        let cache = UnitFactCache::<u32>::new(BoundUnitKind::CallableBody);

        let result =
            cache.get_or_compute(&runtime, &cancellation, constant_template_key(7), || {
                checked(1)
            });

        assert!(matches!(result, Err(FactQueryError::InfrastructureFailure)));
    }

    #[test]
    fn control_flow_fact_caches_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<UnitFactCache<u32>>();
        assert_send_sync::<super::ControlFlowUnitFactCaches>();
    }

    fn published(
        cache: &UnitFactCache<u32>,
        runtime: &FactRuntime,
        cancellation: &CancellationToken,
        key: BoundUnitKey,
        compute: impl FnOnce() -> Result<bray_binder::CheckedUnitComputation<u32>, FactQueryError>,
    ) -> std::sync::Arc<super::PublishedUnit<u32>> {
        match cache.get_or_compute(runtime, cancellation, key, compute) {
            Ok(value) => value,
            Err(error) => panic!("checked unit must publish: {error:?}"),
        }
    }

    fn checked(value: u32) -> Result<bray_binder::CheckedUnitComputation<u32>, FactQueryError> {
        Ok(bray_binder::CheckedUnitComputation::new(
            DiagnosticResult::without_diagnostics(value),
            [],
        ))
    }

    fn diagnostic(id: u32) -> Diagnostic {
        Diagnostic::new(
            DiagnosticId::new(id),
            DiagnosticKind::DeclarationDuplicateName,
            SeverityKind::Error,
        )
    }
}
