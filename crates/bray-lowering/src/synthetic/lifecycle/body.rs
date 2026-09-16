use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockKind, MirCleanupEdge, MirEdge, MirHelperReference, MirOperand, MirPlace, MirProjection,
    MirProjectionKind, MirSourceAnchor, MirStorageKind, MirTargetContract, MirTerminatorKind,
    MirUnit, MirUnitBuilder, MirUnitId, MirUnitKey,
};
use bray_symbols::{BorrowKind, TypeData, TypeId};

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};

/// Lowers one closed lifecycle specialization into its generated MIR body.
pub fn lower_lifecycle<C: SyntheticLoweringContext + ?Sized>(
    context: &C,
    key: MirUnitKey,
    reference: &MirHelperReference,
    unit: MirUnitId,
    target: &MirTargetContract,
) -> Result<MirUnit, C::Error> {
    SyntheticLowerer { context }.lower_lifecycle(key, reference, unit, target)
}
impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    fn lower_lifecycle(
        &self,
        key: MirUnitKey,
        reference: &MirHelperReference,
        unit: MirUnitId,
        target: &MirTargetContract,
    ) -> Result<MirUnit, C::Error> {
        let ty = reference
            .lifecycle_type()
            .unwrap_or_else(|| panic!("synthetic lowering contract violation: MissingHelper {value:?}", value = reference.clone()));

        let values = self.context.semantic_values();

        let pointer = values
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Mutable,
                target: ty,
            })
            .map_err(SyntheticLoweringError::SemanticValue)?;

        let source = MirSourceAnchor::generated_lifecycle(reference.clone());

        let mut builder =
            MirUnitBuilder::for_generated_lifecycle(unit, key, reference.clone(), target.clone());

        let entry = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .map_err(|cause| self.capacity_error(cause))?;

        let storage = builder
            .push_storage(source.clone(), MirStorageKind::Parameter(0), pointer)
            .map_err(|cause| self.capacity_error(cause))?;

        let place = MirPlace::new(
            storage,
            [MirProjection::new(
                MirProjectionKind::Dereference,
                pointer,
                ty,
            )],
            ty,
        );

        match reference {
            MirHelperReference::Cleanup { phase, .. } => {
                self.lower_cleanup_body(
                    &mut builder,
                    entry,
                    &source,
                    reference,
                    *phase,
                    place,
                    target.runtime_abi(),
                )?;
            }
            MirHelperReference::StaticFinalize(ty) => {
                let end;

                let action = self
                    .context
                    .lifecycle_action(*ty, bray_bound_tree::LifecyclePhase::Finalize)?;

                let return_value = if let bray_bound_tree::LifecycleAction::Call(callable) = action
                {
                    let returns_void = callable.execution
                        == bray_symbols::CallableExecution::Synchronous
                        && self.is_void_result(callable.result);

                    let (completed, value) = self.push_static_finalizer_call(
                        &mut builder,
                        entry,
                        &source,
                        place,
                        callable,
                    )?;

                    end = completed;

                    (!returns_void).then_some(MirOperand::Value(value))
                } else {
                    end = self.expand_lifecycle_action(
                        &mut builder,
                        entry,
                        &source,
                        bray_ir::MirGeneratedLifecycleRole::StaticFinalize,
                        place,
                        target.runtime_abi(),
                        action,
                    )?;

                    None
                };

                builder.set_terminator(
                    end,
                    source.clone(),
                    MirTerminatorKind::Return(return_value),
                );
            }
            MirHelperReference::Finalize(_) | MirHelperReference::Destroy(_) => {
                let end = self.push_generated_lifecycle_operations(
                    &mut builder,
                    entry,
                    &source,
                    reference,
                    place,
                    target.runtime_abi(),
                )?;

                builder.set_terminator(end, source.clone(), MirTerminatorKind::Return(None));
            }
            MirHelperReference::AnonymousCallable(_)
            | MirHelperReference::DeclaredCallable(_)
            | MirHelperReference::DefaultValue(_)
            | MirHelperReference::TypeForm(_)
            | MirHelperReference::Conversion(_)
            | MirHelperReference::BeginGenerator
            | MirHelperReference::PushGenerator
            | MirHelperReference::FinishGenerator
            | MirHelperReference::PanicReport
            | MirHelperReference::StandardLibrary(_)
            | MirHelperReference::CreateFrame(_)
            | MirHelperReference::MoveInactiveFrame(_)
            | MirHelperReference::ComposeAwaitedFrame(_)
            | MirHelperReference::CommitAwaitedCompletion(_)
            | MirHelperReference::DestroyTerminalTask => {
                panic!("synthetic lowering helper {reference:?} must be available");
            }
        }

        if let MirHelperReference::Destroy(ty) = reference {
            super::outgoing::discharge_owner(&mut builder, *ty)
            .map_err(|cause| self.capacity_error(cause))?;
        }

        Ok(builder.finish(entry))
    }

    fn is_void_result(&self, ty: TypeId) -> bool {
        let data = self.context.semantic_values().type_data(ty);

        let TypeData::Named { definition, .. } = data.as_ref() else {
            return false;
        };

        matches!(
            self.context.representation_role(*definition),
            Some(RepresentationRole::Unit | RepresentationRole::Never)
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "cleanup lowering retains its body, phase, target, and entry place"
    )]
    fn lower_cleanup_body(
        &self,
        builder: &mut MirUnitBuilder,
        entry: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        reference: &MirHelperReference,
        phase: bray_ir::MirCleanupPhase,
        place: MirPlace,
        runtime_abi: bray_runtime_interface::RuntimeAbiVersion,
    ) -> Result<(), C::Error> {
        let broadcast = builder
            .push_block(source.clone(), MirBlockKind::CleanupBroadcast)
            .map_err(|cause| self.capacity_error(cause))?;

        let lifecycle = builder
            .push_block(source.clone(), MirBlockKind::LifecycleResolution)
            .map_err(|cause| self.capacity_error(cause))?;

        builder.set_terminator(
            entry,
            source.clone(),
            MirTerminatorKind::BeginCleanup(MirCleanupEdge::new(
                bray_ir::MirCleanupPhase::TaskCancellation,
                MirEdge::new(broadcast, []),
            )),
        );

        let (broadcast_end, lifecycle_end) = match phase {
            bray_ir::MirCleanupPhase::TaskCancellation => (
                self.push_generated_lifecycle_operations(
                    builder,
                    broadcast,
                    source,
                    reference,
                    place,
                    runtime_abi,
                )?,
                lifecycle,
            ),
            bray_ir::MirCleanupPhase::LifecycleResolution => (
                broadcast,
                self.push_generated_lifecycle_operations(
                    builder,
                    lifecycle,
                    source,
                    reference,
                    place,
                    runtime_abi,
                )?,
            ),
        };

        builder.set_terminator(
            broadcast_end,
            source.clone(),
            MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                bray_ir::MirCleanupPhase::LifecycleResolution,
                MirEdge::new(lifecycle, []),
            )),
        );

        builder.set_terminator(
            lifecycle_end,
            source.clone(),
            MirTerminatorKind::Return(None),
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use bray_compiler_known::RepresentationRole;
    use bray_ir::{
        MirCleanupPhase, MirGeneratedLifecycleKey, MirGeneratedLifecycleRole, MirHelperReference,
        MirOperationKind, MirProjectionKind, MirStandardLibraryHelper, MirUnitId, MirUnitKey,
    };
    use bray_symbols::{
        AvailableCompilerKnownSymbols, CallableInstanceData, ConstantTermId, NamedTypeSymbolId,
        SemanticValueStore, TypeData, TypeId,
    };

    use super::lower_lifecycle;
    use crate::{SyntheticLoweringContext, SyntheticLoweringError};

    struct TupleContext(SemanticValueStore);

    impl SyntheticLoweringContext for TupleContext {
        type Error = SyntheticLoweringError;

        fn semantic_values(&self) -> &SemanticValueStore {
            &self.0
        }

        fn compiler_known_symbols(&self) -> &AvailableCompilerKnownSymbols {
            panic!("tuple lowering must not query compiler-known declarations");
        }

        fn lifecycle_action(
            &self,
            ty: TypeId,
            phase: bray_bound_tree::LifecyclePhase,
        ) -> Result<bray_bound_tree::LifecycleAction, Self::Error> {
            use bray_bound_tree::{LifecycleAction, LifecyclePhase};

            if phase == LifecyclePhase::Finalize {
                return Ok(LifecycleAction::None);
            }

            if phase == LifecyclePhase::Resolve {
                return Ok(LifecycleAction::Resolve);
            }

            Ok(match self.0.type_data(ty).as_ref() {
                TypeData::Tuple(members) => {
                    LifecycleAction::Members(std::sync::Arc::clone(members))
                }
                TypeData::Nullable(target) => LifecycleAction::Nullable(*target),
                TypeData::Array { element, length } => LifecycleAction::Array {
                    element: *element,
                    length: *length,
                },
                _ => panic!("unexpected test lifecycle type"),
            })
        }

        fn representation_role(&self, _: NamedTypeSymbolId) -> Option<RepresentationRole> {
            panic!("tuple lowering must not resolve named representation roles");
        }

        fn representation_type(&self, role: RepresentationRole) -> Result<TypeId, Self::Error> {
            assert!(matches!(
                role,
                RepresentationRole::ScalarBool
                    | RepresentationRole::PanicReport
                    | RepresentationRole::Unit
                    | RepresentationRole::ScalarUsize
            ));

            self.0
                .intern_type(TypeData::tuple([]))
                .map_err(SyntheticLoweringError::SemanticValue)
        }

        fn array_length(&self, length: ConstantTermId) -> Result<u64, Self::Error> {
            let length = self.0.constant_term_data(length);

            let bray_symbols::ConstantTermData::IntegerLiteral { value, .. } = length.as_ref()
            else {
                panic!("test extent must be an integer literal");
            };

            Ok(value.to_u64().unwrap())
        }

        fn standard_library_callable(
            &self,
            _: MirStandardLibraryHelper,
        ) -> Result<CallableInstanceData, Self::Error> {
            panic!("tuple lowering must not resolve standard-library helpers");
        }
    }

    #[test]
    fn array_cleanup_mir_size_is_independent_of_the_extent() {
        let context = TupleContext(SemanticValueStore::try_new().unwrap());
        let leaf = context.0.intern_type(TypeData::tuple([])).unwrap();
        let mut sizes = Vec::new();

        for length in [1, 4, 257, u64::from(u32::MAX) + 1] {
            let length = context
                .0
                .intern_constant_term(bray_symbols::ConstantTermData::IntegerLiteral {
                    ty: bray_symbols::TargetSizedIntegerType::Usize,
                    value: bray_symbols::IntegerConstant::from_u64(length),
                })
                .unwrap();

            let array = context
                .0
                .intern_type(TypeData::Array {
                    element: leaf,
                    length,
                })
                .unwrap();

            let reference = MirHelperReference::Destroy(array);

            let key = MirUnitKey::GeneratedLifecycle(MirGeneratedLifecycleKey::new(
                MirGeneratedLifecycleRole::Destroy,
                [9; 32],
            ));

            let mir = lower_lifecycle(
                &context,
                key,
                &reference,
                MirUnitId::new(7),
                &bray_testing::test_mir_target(),
            )
            .unwrap();

            sizes.push((mir.blocks().len(), mir.operations().len()));

            assert_eq!(
                mir.operations()
                    .iter()
                    .filter(|operation| matches!(
                        operation.kind(),
                        MirOperationKind::Finalize(_) | MirOperationKind::Destroy(_)
                    ))
                    .count(),
                2
            );
        }

        assert!(sizes.windows(2).all(|pair| pair[0] == pair[1]), "{sizes:?}");
    }

    #[test]
    fn tuple_destruction_lowers_reverse_order_without_compilation_queries() {
        let context = TupleContext(SemanticValueStore::try_new().unwrap());
        let leaf = context.0.intern_type(TypeData::tuple([])).unwrap();

        let pair = context
            .0
            .intern_type(TypeData::tuple([leaf, leaf]))
            .unwrap();

        let reference = MirHelperReference::Destroy(pair);

        let key = MirUnitKey::GeneratedLifecycle(MirGeneratedLifecycleKey::new(
            MirGeneratedLifecycleRole::Destroy,
            [4; 32],
        ));

        let mir = lower_lifecycle(
            &context,
            key.clone(),
            &reference,
            MirUnitId::new(3),
            &bray_testing::test_mir_target(),
        )
        .unwrap();

        assert_eq!(mir.key(), &key);

        let operations = mir
            .operations()
            .iter()
            .filter_map(|operation| {
                let (finalize, place) = match operation.kind() {
                    MirOperationKind::Finalize(place) => (true, place),
                    MirOperationKind::Destroy(place) => (false, place),
                    _ => return None,
                };

                let MirProjectionKind::TupleField(index) =
                    place.projections().last().unwrap().kind()
                else {
                    panic!("tuple teardown must retain the element projection");
                };

                Some((finalize, *index))
            })
            .collect::<Vec<_>>();

        assert_eq!(operations, [(true, 1), (false, 1), (true, 0), (false, 0)]);
    }

    #[test]
    fn nullable_cleanup_lowers_both_phases_with_generated_provenance() {
        let context = TupleContext(SemanticValueStore::try_new().unwrap());
        let leaf = context.0.intern_type(TypeData::tuple([])).unwrap();
        let nullable = context.0.intern_type(TypeData::Nullable(leaf)).unwrap();

        for phase in [
            MirCleanupPhase::TaskCancellation,
            MirCleanupPhase::LifecycleResolution,
        ] {
            let reference = MirHelperReference::Cleanup {
                phase,
                ty: nullable,
            };

            let key = MirUnitKey::GeneratedLifecycle(MirGeneratedLifecycleKey::new(
                MirGeneratedLifecycleRole::Cleanup(phase),
                [7; 32],
            ));

            let mir = lower_lifecycle(
                &context,
                key,
                &reference,
                MirUnitId::new(5),
                &bray_testing::test_mir_target(),
            )
            .unwrap();

            assert!(mir.operations().iter().all(|operation| operation.source()
                == &bray_ir::MirSourceAnchor::generated_lifecycle(reference.clone())));

            assert!(
                mir.blocks()
                    .iter()
                    .any(|block| block.kind() == bray_ir::MirBlockKind::CleanupBroadcast)
            );

            assert!(
                mir.blocks()
                    .iter()
                    .any(|block| block.kind() == bray_ir::MirBlockKind::LifecycleResolution)
            );

            match phase {
                MirCleanupPhase::TaskCancellation => assert!(mir.operations().iter().any(|operation| matches!(operation.kind(), MirOperationKind::Cleanup { phase: MirCleanupPhase::TaskCancellation, place } if place.ty() == leaf))),
                MirCleanupPhase::LifecycleResolution => assert!(mir.operations().iter().any(|operation| matches!(operation.kind(), MirOperationKind::Destroy(place) if place.ty() == nullable))),
            }
        }
    }
}
