use bray_bound_tree::{BoundDependencyRequirement, BoundExpressionId, CheckedDependencyContracts, CheckedExpressionTypes, CheckedSemanticSelections};

use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

impl super::ValueInputs {
    pub(super) fn collect_copied_reads<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        types: &CheckedExpressionTypes,
        selections: &CheckedSemanticSelections,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        if matches!(request.view().kind(), bray_bound_tree::BoundUnitKind::ContractClause | bray_bound_tree::BoundUnitKind::PredicateDefinition) {
            return Ok(());
        }

        for (id, _) in request.unit().tree().expressions() {
            if super::assignment::member_projection(request.unit(), selections, id).is_none() {
                continue;
            }

            let ty = types.expression(id).expect("checked field read retains its result type").ty();
            let borrow_free = self.classify_copy_type(request, ty)?;

            if borrow_free {
                self.copies_without_borrows.insert(id);
            }
        }

        Ok(())
    }

    pub(crate) fn collect_contract_copies<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        dependencies: &CheckedDependencyContracts,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        let mut pending = dependencies.calls().values()
            .flat_map(|call| call.phase_contracts())
            .flat_map(|contract| contract.requirements()).collect::<Vec<_>>();

        while let Some(requirement) = pending.pop() {
            match requirement {
                BoundDependencyRequirement::Value { ty, .. } => {
                    self.classify_copy_type(request, *ty)?;
                }
                BoundDependencyRequirement::Guarded(guarded) => pending.extend(guarded.requirements()),
                BoundDependencyRequirement::Direct { .. } => {}
            }
        }

        Ok(())
    }

    fn classify_copy_type<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        ty: bray_symbols::TypeId,
    ) -> Result<bool, CheckerQueryError<C::UpstreamError>> {
        Ok(match self.borrow_free_types.entry(ty) {
            std::collections::btree_map::Entry::Occupied(entry) => *entry.get(),
            std::collections::btree_map::Entry::Vacant(entry) => {
                let logical = matches!(request.view().kind(), bray_bound_tree::BoundUnitKind::ContractClause | bray_bound_tree::BoundUnitKind::PredicateDefinition);

                let borrow_free = if logical {
                    crate::representation::reference_kind(request, ty) == Some(bray_symbols::DependencyReferenceKind::Address)
                } else {
                    crate::storage::observation::carries_no_borrows(request, ty)?
                };

                *entry.insert(borrow_free)
            }
        })
    }

    pub(crate) fn value_carries_no_borrows(&self, ty: bray_symbols::TypeId) -> bool {
        self.borrow_free_types.get(&ty) == Some(&true)
    }

    /// A copied field can carry scope dependencies without carrying its reading loan.
    pub(crate) fn copies_without_borrows(&self, expression: BoundExpressionId) -> bool {
        self.copies_without_borrows.contains(&expression)
    }
}
