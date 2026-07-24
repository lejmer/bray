use std::collections::BTreeSet;
use std::sync::Arc;

use bray_binder::BinderDependency;
use bray_bound_tree::BoundUnitKey;
use bray_diagnostics::DiagnosticResult;

use super::{CancellationToken, CompilationFactKey, FactCellMap, FactQueryError, FactRuntime};

#[cfg(test)]
use super::FactCellTestObserver;

#[derive(Debug)]
pub(crate) struct PublishedUnitFact<T> {
    result: Arc<DiagnosticResult<T>>,
    #[cfg(test)]
    dependencies: Box<[BinderDependency]>,
}

impl<T> PublishedUnitFact<T> {
    pub(crate) const fn result(&self) -> &Arc<DiagnosticResult<T>> {
        &self.result
    }

    #[cfg(test)]
    pub(crate) fn dependencies(&self) -> &[BinderDependency] {
        &self.dependencies
    }
}

#[derive(Debug)]
pub(crate) struct UnitFactCache<T> {
    cells: FactCellMap<BoundUnitKey, Arc<PublishedUnitFact<T>>>,
}

impl<T> UnitFactCache<T>
where
    T: Send + Sync,
{
    pub(crate) const fn new() -> Self {
        Self {
            cells: FactCellMap::new(),
        }
    }

    pub(crate) fn updated(
        &self,
        reusable: &BTreeSet<CompilationFactKey>,
        fact_key: impl Fn(&BoundUnitKey) -> CompilationFactKey,
    ) -> Self {
        Self {
            cells: self.cells.updated(reusable, fact_key),
        }
    }

    pub(crate) fn get_or_compute(
        &self,
        runtime: &FactRuntime,
        cancellation: &CancellationToken,
        fact_key: CompilationFactKey,
        unit_key: BoundUnitKey,
        compute: impl FnOnce() -> Result<(DiagnosticResult<T>, Box<[BinderDependency]>), FactQueryError>
        + Send,
    ) -> Result<Arc<PublishedUnitFact<T>>, FactQueryError> {
        if fact_key.bound_unit_key() != Some(&unit_key) {
            return Err(FactQueryError::InfrastructureFailure);
        }

        // The map owns the immutable unit identity independently of the caller's request.
        let cell = self.cells.cell(unit_key.clone())?;

        let published = cell.get_or_compute(runtime, fact_key, cancellation, || {
            let computation = compute()?;

            #[cfg(test)]
            let (result, dependencies) = computation;

            #[cfg(not(test))]
            let (result, _) = computation;

            Ok(Arc::new(PublishedUnitFact {
                result: Arc::new(result),
                #[cfg(test)]
                dependencies,
            }))
        })?;

        // Publication must outlive the short-lived map and cell borrows returned by this query.
        Ok(Arc::clone(published))
    }

    #[cfg(test)]
    pub(crate) fn is_published(&self, key: &BoundUnitKey) -> Result<bool, FactQueryError> {
        self.cells.is_published(key)
    }

    #[cfg(test)]
    pub(crate) fn shares_cell_with(&self, other: &Self, key: &BoundUnitKey) -> bool {
        self.cells.shares_cell_with(&other.cells, key)
    }

    #[cfg(test)]
    pub(crate) fn set_test_observer(
        &self,
        key: &BoundUnitKey,
        observer: FactCellTestObserver,
    ) -> Result<(), FactQueryError> {
        // Test observers attach to the same exact cell used by production publication.
        self.cells.cell(key.clone())?.set_test_observer(observer)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_binder::BinderDependency;
    use bray_diagnostics::DiagnosticResult;

    use super::UnitFactCache;
    use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, FactRuntime};
    use crate::test_support::callable_body_key;

    #[test]
    fn repeated_requests_publish_one_atomic_result() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();
        let cache = UnitFactCache::new();
        let computations = AtomicUsize::new(0);

        let key = callable_body_key(0);

        let first = published(&cache, &runtime, &cancellation, key.clone(), || {
            computations.fetch_add(1, Ordering::SeqCst);
            computation(11)
        });

        let second = published(&cache, &runtime, &cancellation, key, || {
            computations.fetch_add(1, Ordering::SeqCst);
            computation(22)
        });

        assert!(std::sync::Arc::ptr_eq(&first, &second));
        assert_eq!(first.result().value(), &11);
        assert!(first.dependencies().is_empty());
        assert_eq!(computations.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn cancellation_publishes_no_partial_fact() {
        let runtime = FactRuntime::default();
        let cancelled = CancellationToken::new();
        let cache = UnitFactCache::new();

        let key = callable_body_key(1);

        cancelled.cancel();

        let result = cache.get_or_compute(
            &runtime,
            &cancelled,
            CompilationFactKey::BoundUnit(key.clone()),
            key.clone(),
            || computation(3),
        );

        assert!(matches!(result, Err(FactQueryError::Cancelled)));
        assert_eq!(cache.is_published(&key), Ok(false));

        let retried = cache.get_or_compute(
            &runtime,
            &CancellationToken::new(),
            CompilationFactKey::BoundUnit(key.clone()),
            key,
            || computation(4),
        );

        assert!(retried.is_ok());
    }

    fn published(
        cache: &UnitFactCache<u32>,
        runtime: &FactRuntime,
        cancellation: &CancellationToken,
        key: bray_bound_tree::BoundUnitKey,
        compute: impl FnOnce()
            -> Result<(DiagnosticResult<u32>, Box<[BinderDependency]>), FactQueryError>
        + Send,
    ) -> std::sync::Arc<super::PublishedUnitFact<u32>> {
        match cache.get_or_compute(
            runtime,
            cancellation,
            CompilationFactKey::BoundUnit(key.clone()),
            key,
            compute,
        ) {
            Ok(value) => value,
            Err(error) => panic!("unit fact must publish: {error:?}"),
        }
    }

    fn computation(
        value: u32,
    ) -> Result<(DiagnosticResult<u32>, Box<[BinderDependency]>), FactQueryError> {
        Ok((DiagnosticResult::without_diagnostics(value), Box::new([])))
    }
}
