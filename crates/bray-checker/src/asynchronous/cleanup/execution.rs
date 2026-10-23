use std::collections::{BTreeMap, BTreeSet};

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
        let mut active = BTreeSet::new();
        let mut known = BTreeMap::new();

        for ty in &cleanup {
            if !self.cleanup_is_synchronous(*ty, completed, &mut active, &mut known)? {
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

    fn cleanup_is_synchronous<F>(
        &mut self,
        ty: TypeId,
        completed: &mut F,
        active: &mut BTreeSet<TypeId>,
        known: &mut BTreeMap<TypeId, bool>,
    ) -> Result<bool, CheckerQueryError<C::UpstreamError>>
    where
        F: FnMut(
            bray_bound_tree::LifecycleCallable,
        ) -> Result<bool, CheckerQueryError<C::UpstreamError>>,
    {
        if let Some(known) = known.get(&ty) {
            return Ok(*known);
        }

        if !active.insert(ty) {
            return Ok(true);
        }

        let result = self.cleanup_parts_are_synchronous(ty, completed, active, known)?;

        active.remove(&ty);
        known.insert(ty, result);

        Ok(result)
    }

    fn cleanup_parts_are_synchronous<F>(
        &mut self,
        ty: TypeId,
        completed: &mut F,
        active: &mut BTreeSet<TypeId>,
        known: &mut BTreeMap<TypeId, bool>,
    ) -> Result<bool, CheckerQueryError<C::UpstreamError>>
    where
        F: FnMut(
            bray_bound_tree::LifecycleCallable,
        ) -> Result<bool, CheckerQueryError<C::UpstreamError>>,
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
            if part.as_ref().is_some_and(|part| part.release().is_some()) {
                continue;
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

            for slot in [
                bray_symbols::TypeAssociatedLifecycleSlot::Finalizer,
                bray_symbols::TypeAssociatedLifecycleSlot::Destructor,
            ] {
                let selected = self.request.context().lifecycle_callable(*ty, slot)?;

                self.diagnostics
                    .add_range(selected.diagnostics().iter().cloned());

                let Some((target, signature)) = selected.value() else {
                    continue;
                };

                let data = self
                    .request
                    .semantic_values()
                    .type_data(signature.callable_type());

                let TypeData::Callable(callable) = data.as_ref() else {
                    panic!("selected lifecycle retains its callable type");
                };

                if slot == bray_symbols::TypeAssociatedLifecycleSlot::Finalizer {
                    let lifecycle = bray_bound_tree::LifecycleCallable::from_signature(
                        self.request.semantic_values(),
                        self.request.available_compiler_known_symbols(),
                        *target,
                        signature,
                    )
                    .expect("selected lifecycle uses checked semantic types");

                    if !completed(lifecycle)?
                        && (callable.execution() == bray_symbols::CallableExecution::Asynchronous
                            || self.resolve(callable.result())?.quiescence)
                    {
                        return Ok(false);
                    }
                } else {
                    for child in &self.receiver_cleanup(*target)? {
                        if !self.cleanup_is_synchronous(*child, completed, active, known)? {
                            return Ok(false);
                        }
                    }
                }
            }
        }

        Ok(true)
    }
}
