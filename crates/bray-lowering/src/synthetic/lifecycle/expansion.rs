use bray_ir::{
    MirAbandonmentAction, MirBlockId, MirCleanupPhase, MirGeneratedLifecycleRole, MirOperationKind,
    MirPlace, MirSourceAnchor, MirUnitBuilder,
};
use bray_symbols::{TypeAssociatedLifecycleSlot, TypeData, TypeId};

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};
use crate::cleanup_outcome::CleanupOutcome;

/// Active type/action pairs bound expansion through recursive owner completion payloads.
pub(in crate::synthetic) struct LifecycleExpansion<'parent> {
    pub(super) parent: Option<&'parent LifecycleExpansion<'parent>>,
    pub(super) role: MirGeneratedLifecycleRole,
    pub(super) ty: TypeId,
}

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    /// Expands finite represented cleanup while retaining failures in the caller's outcome.
    pub(super) fn expand_structural_lifecycle_action(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        role: MirGeneratedLifecycleRole,
        place: MirPlace,
        outcome: &CleanupOutcome,
    ) -> Result<Option<MirBlockId>, C::Error> {
        let mut ancestor = self.lifecycle_expansion;

        while let Some(expansion) = ancestor {
            if expansion.ty == place.ty() && expansion.role == role {
                return Ok(None);
            }

            ancestor = expansion.parent;
        }

        let data = self
            .context
            .semantic_values()
            .type_data(place.ty())
            .map_err(SyntheticLoweringError::SemanticValue)?;

        let structural = match data.as_ref() {
            TypeData::Array { .. } | TypeData::Tuple(_) | TypeData::Nullable(_) => true,
            TypeData::Named {
                definition,
                substitution,
            } => {
                self.context.representation_role(*definition).is_none()
                    && self
                        .context
                        .raw_buffer_element(*definition, *substitution)?
                        .is_none()
            }
            // Dynamic containers and owned indirection can lead back to the containing type.
            // Their separate lifecycle body bounds structural expansion at that recursive edge.
            _ => false,
        };

        if !structural {
            return Ok(None);
        }

        let slot = match role {
            MirGeneratedLifecycleRole::Finalize | MirGeneratedLifecycleRole::StaticFinalize => {
                Some(TypeAssociatedLifecycleSlot::Finalizer)
            }
            MirGeneratedLifecycleRole::Destroy
            | MirGeneratedLifecycleRole::Abandon(MirAbandonmentAction::Destroy) => {
                Some(TypeAssociatedLifecycleSlot::Destructor)
            }
            MirGeneratedLifecycleRole::Abandon(MirAbandonmentAction::Destructor) => {
                return Ok(None);
            }
            MirGeneratedLifecycleRole::Cleanup(_)
            | MirGeneratedLifecycleRole::Abandon(MirAbandonmentAction::Quiesce) => None,
        };

        if let Some(slot) = slot
            && self.context.lifecycle_callable(place.ty(), slot)?.is_some()
        {
            return Ok(None);
        }

        let expansion = LifecycleExpansion {
            parent: self.lifecycle_expansion,
            role,
            ty: place.ty(),
        };

        let lowerer = SyntheticLowerer {
            context: self.context,
            lifecycle_expansion: Some(&expansion),
        };

        let completed = match role {
            MirGeneratedLifecycleRole::Finalize | MirGeneratedLifecycleRole::StaticFinalize => {
                block
            }
            MirGeneratedLifecycleRole::Cleanup(MirCleanupPhase::LifecycleResolution) => {
                // Both actions retain the same destination path. A declared finalizer remains a call.
                let block = lowerer.resolve_lifecycle_action(
                    builder,
                    block,
                    source,
                    MirOperationKind::Finalize(place.clone()),
                    outcome,
                )?;

                lowerer.resolve_lifecycle_action(
                    builder,
                    block,
                    source,
                    MirOperationKind::Destroy(place),
                    outcome,
                )?
            }
            _ => lowerer
                .expand_represented_lifecycle(builder, block, source, role, place, outcome)?,
        };

        Ok(Some(completed))
    }
}
