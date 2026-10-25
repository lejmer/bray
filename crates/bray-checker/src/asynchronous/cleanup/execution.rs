use std::collections::BTreeSet;

use bray_symbols::{
    CallableContractsQuery, CallableInstanceData, SymbolQueryRequest, TypeData, TypeId,
};

use super::shape::CleanupShapeResolver;
use crate::{CheckerQueryError, CheckerRequestContext, CheckerSemanticQueryProvider};

impl<C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableContractsQuery> + ?Sized>
    CleanupShapeResolver<'_, C>
{
    pub(crate) fn destruction_is_synchronous<F>(
        &mut self,
        ty: TypeId,
        completed: &mut F,
    ) -> Result<bool, CheckerQueryError<C::UpstreamError>>
    where
        F: FnMut(
            bray_bound_tree::LifecycleCallable,
        ) -> Result<bool, CheckerQueryError<C::UpstreamError>>,
    {
        let selected = self
            .request
            .context()
            .lifecycle_callable(ty, bray_symbols::TypeAssociatedLifecycleSlot::Destructor)?;

        self.diagnostics
            .add_range(selected.diagnostics().iter().cloned());

        let Some((target, _)) = selected.value() else {
            return Ok(true);
        };

        let cleanup = self.receiver_cleanup(*target)?;

        for ty in &cleanup {
            if !self.cleanup_is_synchronous(*ty, completed, &mut |_| {})? {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn receiver_cleanup(
        &mut self,
        target: CallableInstanceData,
    ) -> Result<Vec<TypeId>, CheckerQueryError<C::UpstreamError>> {
        let contract = self
            .request
            .context()
            .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(
                target.definition().callable_symbol(),
            ))?;

        self.diagnostics
            .add_range(contract.diagnostics().iter().cloned());

        Ok(contract
            .value()
            .phase_behaviors()
            .invocation()
            .receiver_cleanup()
            .iter()
            .map(|ty| {
                self.request
                    .semantic_values()
                    .substitute_type(*ty, target.substitution())
                    .expect("checked receiver cleanup substitution retains valid semantic types")
            })
            .collect())
    }

    pub(crate) fn cleanup_is_synchronous<F, G>(
        &mut self,
        ty: TypeId,
        completed: &mut F,
        dependency: &mut G,
    ) -> Result<bool, CheckerQueryError<C::UpstreamError>>
    where
        F: FnMut(
            bray_bound_tree::LifecycleCallable,
        ) -> Result<bool, CheckerQueryError<C::UpstreamError>>,
        G: FnMut(bray_symbols::DependencyRequirement),
    {
        let mut pending = vec![ty];
        let mut expanded = BTreeSet::new();

        while let Some(ty) = pending.pop() {
            if !expanded.insert(ty) {
                continue;
            }

            if !self.cleanup_parts_are_synchronous(ty, completed, &mut pending, dependency)? {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn cleanup_parts_are_synchronous<F, G>(
        &mut self,
        ty: TypeId,
        completed: &mut F,
        pending: &mut Vec<TypeId>,
        dependency: &mut G,
    ) -> Result<bool, CheckerQueryError<C::UpstreamError>>
    where
        F: FnMut(
            bray_bound_tree::LifecycleCallable,
        ) -> Result<bool, CheckerQueryError<C::UpstreamError>>,
        G: FnMut(bray_symbols::DependencyRequirement),
    {
        let parts = super::super::parts::full_cleanup_parts(
            self,
            ty,
            self.request.unit().key().source(),
            false,
        )?;

        self.diagnostics
            .add_range(parts.diagnostics().iter().cloned());

        for (ty, part) in parts.value() {
            if let TypeData::TypeValuedMemberProjection {
                subject,
                application,
                member,
            } = self.request.semantic_values().type_data(*ty).as_ref()
            {
                let selected = self.request.context().selected_type_valued_member(
                    *subject,
                    *application,
                    *member,
                )?;

                self.diagnostics
                    .add_range(selected.diagnostics().iter().cloned());

                if let Some(selected) = selected.value() {
                    pending.push(*selected);
                } else {
                    dependency(bray_symbols::DependencyRequirement::SynchronousCleanup(*ty));
                }

                continue;
            }

            if matches!(
                self.request.semantic_values().type_data(*ty).as_ref(),
                TypeData::TypeParameter(_) | TypeData::ContextualSelf(_)
            ) {
                dependency(bray_symbols::DependencyRequirement::SynchronousCleanup(*ty));

                continue;
            }

            if let Some(part) = part {
                for projection in part.projections() {
                    if let bray_bound_tree::StorageCleanupProjectionKind::OwnedTarget(borrow) =
                        projection.projection()
                    {
                        self.observe_cleanup_call(borrow.callable(), dependency);
                    }
                }

                if let Some(release) = part.release() {
                    self.observe_cleanup_call(release.callable(), dependency);

                    continue;
                }
            }

            if let TypeData::Named {
                definition,
                substitution,
            } = self.request.semantic_values().type_data(*ty).as_ref()
                && let Some(element) = self
                    .request
                    .context()
                    .raw_buffer_element(*definition, *substitution)?
            {
                pending.push(element);
            }

            if part
                .as_ref()
                .is_none_or(|part| part.phases().includes_cancellation())
                && self.resolve(*ty)?.quiescence
            {
                return Ok(false);
            }

            if part
                .as_ref()
                .is_some_and(|part| !part.phases().includes_lifecycle())
            {
                continue;
            }

            if let Some(lifecycle) = self.pending_finalizer(*ty, completed)? {
                if lifecycle.result.execution() == bray_symbols::CallableExecution::Asynchronous
                    || self.resolve(lifecycle.result.completion_type())?.quiescence
                {
                    return Ok(false);
                }

                self.observe_cleanup_call(lifecycle.callable, dependency);
            }

            let selected = self
                .request
                .context()
                .lifecycle_callable(*ty, bray_symbols::TypeAssociatedLifecycleSlot::Destructor)?;

            self.diagnostics
                .add_range(selected.diagnostics().iter().cloned());

            if let Some((target, _)) = selected.value() {
                self.observe_cleanup_call(*target, dependency);

                for child in &self.receiver_cleanup(*target)? {
                    pending.push(*child);
                }
            }
        }

        Ok(true)
    }

    pub(crate) fn pending_finalizer<F>(
        &mut self,
        ty: TypeId,
        completed: &mut F,
    ) -> Result<Option<bray_bound_tree::LifecycleCallable>, CheckerQueryError<C::UpstreamError>>
    where
        F: FnMut(
            bray_bound_tree::LifecycleCallable,
        ) -> Result<bool, CheckerQueryError<C::UpstreamError>>,
    {
        let selected = self
            .request
            .context()
            .lifecycle_callable(ty, bray_symbols::TypeAssociatedLifecycleSlot::Finalizer)?;

        self.diagnostics
            .add_range(selected.diagnostics().iter().cloned());

        let Some((target, signature)) = selected.value() else {
            return Ok(None);
        };

        let lifecycle = bray_bound_tree::LifecycleCallable::from_signature(
            self.request.semantic_values(),
            self.request.available_compiler_known_symbols(),
            *target,
            signature,
        )
        .expect("selected lifecycle uses checked semantic types");

        Ok((!completed(lifecycle)?).then_some(lifecycle))
    }

    fn observe_cleanup_call(
        &self,
        callable: CallableInstanceData,
        dependency: &mut impl FnMut(bray_symbols::DependencyRequirement),
    ) {
        let callable = self
            .request
            .semantic_values()
            .intern_callable_instance(callable)
            .expect("selected cleanup retains its callable identity");

        dependency(bray_symbols::DependencyRequirement::result_call(
            callable,
            None,
            [],
            bray_symbols::DependencyCallOutput::SynchronousCleanup,
        ))
    }
}
