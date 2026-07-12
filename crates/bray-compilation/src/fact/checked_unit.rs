use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use bray_binder::{BinderDependency, CheckedUnitComputation};
use bray_bound_tree::{
    BoundUnitId, BoundUnitKey, BoundUnitKind, CheckedAnonymousCallable, CheckedCallableBody,
    CheckedConstantTemplateUnit, CheckedConstraintUnit, CheckedContractClauseUnit,
    CheckedPredicateDefinitionUnit, CheckedRuntimeDefaultUnit,
};
use bray_diagnostics::DiagnosticResult;

use super::{CancellationToken, CompilationFactKey, FactCell, FactQueryError, FactRuntime};

#[derive(Debug)]
pub(crate) struct PublishedCheckedUnit<T> {
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

impl<T> PublishedCheckedUnit<T> {
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
pub(crate) struct CheckedUnitFactCache<T> {
    kind: BoundUnitKind,
    cells: Mutex<CheckedUnitCells<T>>,
}

type CheckedUnitCells<T> = BTreeMap<BoundUnitKey, Arc<FactCell<Arc<PublishedCheckedUnit<T>>>>>;

impl<T> CheckedUnitFactCache<T> {
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
    ) -> Result<Arc<PublishedCheckedUnit<T>>, FactQueryError> {
        if key.kind() != self.kind {
            return Err(FactQueryError::InfrastructureFailure);
        }

        let cell = self.cell(&key)?;
        let fact_key = CompilationFactKey::CheckedUnit(key);

        let published = cell.get_or_compute(runtime, fact_key, cancellation, || {
            let (result, dependencies) = compute()?.into_parts();

            Ok(Arc::new(PublishedCheckedUnit {
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
    ) -> Result<Arc<FactCell<Arc<PublishedCheckedUnit<T>>>>, FactQueryError> {
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
pub(crate) struct CheckedUnitFactCaches {
    unit_ids: Mutex<BTreeMap<BoundUnitKey, BoundUnitId>>,
    callable_body: CheckedUnitFactCache<CheckedCallableBody>,
    anonymous_callable: CheckedUnitFactCache<CheckedAnonymousCallable>,
    runtime_default: CheckedUnitFactCache<CheckedRuntimeDefaultUnit>,
    constant_template: CheckedUnitFactCache<CheckedConstantTemplateUnit>,
    predicate_definition: CheckedUnitFactCache<CheckedPredicateDefinitionUnit>,
    constraint: CheckedUnitFactCache<CheckedConstraintUnit>,
    contract_clause: CheckedUnitFactCache<CheckedContractClauseUnit>,
}

impl CheckedUnitFactCaches {
    pub(crate) const fn new() -> Self {
        Self {
            unit_ids: Mutex::new(BTreeMap::new()),
            callable_body: CheckedUnitFactCache::new(BoundUnitKind::CallableBody),
            anonymous_callable: CheckedUnitFactCache::new(BoundUnitKind::AnonymousCallable),
            runtime_default: CheckedUnitFactCache::new(BoundUnitKind::RuntimeDefault),
            constant_template: CheckedUnitFactCache::new(BoundUnitKind::ConstantTemplate),
            predicate_definition: CheckedUnitFactCache::new(BoundUnitKind::PredicateDefinition),
            constraint: CheckedUnitFactCache::new(BoundUnitKind::Constraint),
            contract_clause: CheckedUnitFactCache::new(BoundUnitKind::ContractClause),
        }
    }

    pub(crate) fn get_or_compute<T: CheckedUnitFact>(
        &self,
        runtime: &FactRuntime,
        cancellation: &CancellationToken,
        key: BoundUnitKey,
        compute: impl FnOnce() -> Result<CheckedUnitComputation<T>, FactQueryError>,
    ) -> Result<Arc<PublishedCheckedUnit<T>>, FactQueryError> {
        T::cache(self).get_or_compute(runtime, cancellation, key, compute)
    }

    pub(crate) fn unit_id(&self, key: &BoundUnitKey) -> Result<BoundUnitId, FactQueryError> {
        let mut unit_ids = self
            .unit_ids
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        if let Some(unit) = unit_ids.get(key).copied() {
            return Ok(unit);
        }

        let unit = BoundUnitId::try_from_index(unit_ids.len())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        // The map and checked-unit caches retain the same Arc-backed immutable key.
        unit_ids.insert(key.clone(), unit);

        Ok(unit)
    }

    #[cfg(test)]
    pub(crate) fn is_published<T: CheckedUnitFact>(
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

pub(crate) trait CheckedUnitFact: Sized {
    fn cache(caches: &CheckedUnitFactCaches) -> &CheckedUnitFactCache<Self>;
}

macro_rules! define_checked_unit_facts {
    ($($value:ty => $field:ident),+ $(,)?) => {
        $(
            impl CheckedUnitFact for $value {
                fn cache(caches: &CheckedUnitFactCaches) -> &CheckedUnitFactCache<Self> {
                    &caches.$field
                }
            }
        )+
    };
}

define_checked_unit_facts! {
    CheckedCallableBody => callable_body,
    CheckedAnonymousCallable => anonymous_callable,
    CheckedRuntimeDefaultUnit => runtime_default,
    CheckedConstantTemplateUnit => constant_template,
    CheckedPredicateDefinitionUnit => predicate_definition,
    CheckedConstraintUnit => constraint,
    CheckedContractClauseUnit => contract_clause,
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_bound_tree::{BoundUnitKey, BoundUnitKind};
    use bray_diagnostics::{
        Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticResult, SeverityKind,
    };

    use super::CheckedUnitFactCache;
    use crate::fact::{CancellationToken, FactQueryError, FactRuntime};
    use crate::test_support::{callable_body_key, constant_template_key};

    #[test]
    fn repeated_requests_publish_one_atomic_result() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();
        let cache = CheckedUnitFactCache::new(BoundUnitKind::CallableBody);
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
        let cache = CheckedUnitFactCache::new(BoundUnitKind::CallableBody);
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
    fn cancellation_publishes_no_partial_checked_unit() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();
        let cache = CheckedUnitFactCache::new(BoundUnitKind::CallableBody);

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
        let cache = CheckedUnitFactCache::new(BoundUnitKind::CallableBody);

        let first_key = callable_body_key(3);
        let second_key = callable_body_key(4);

        let first = published(&cache, &runtime, &cancellation, first_key.clone(), || {
            checked(3)
        });

        let second = published(&cache, &runtime, &cancellation, second_key, || checked(4));

        assert_eq!(first.result().value(), &3);
        assert_eq!(second.result().value(), &4);

        let recursive_cache = CheckedUnitFactCache::new(BoundUnitKind::CallableBody);

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
                crate::CompilationFactKey::CheckedUnit(first_key.clone()),
                crate::CompilationFactKey::CheckedUnit(first_key)
            ]
        );
    }

    #[test]
    fn nested_diagnostics_remain_owned_by_nested_facts() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();
        let cache = CheckedUnitFactCache::new(BoundUnitKind::CallableBody);

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
        let cache = CheckedUnitFactCache::<u32>::new(BoundUnitKind::CallableBody);

        let result =
            cache.get_or_compute(&runtime, &cancellation, constant_template_key(7), || {
                checked(1)
            });

        assert!(matches!(result, Err(FactQueryError::InfrastructureFailure)));
    }

    #[test]
    fn checked_unit_fact_caches_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CheckedUnitFactCache<u32>>();
        assert_send_sync::<super::CheckedUnitFactCaches>();
    }

    fn published(
        cache: &CheckedUnitFactCache<u32>,
        runtime: &FactRuntime,
        cancellation: &CancellationToken,
        key: BoundUnitKey,
        compute: impl FnOnce() -> Result<bray_binder::CheckedUnitComputation<u32>, FactQueryError>,
    ) -> std::sync::Arc<super::PublishedCheckedUnit<u32>> {
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
