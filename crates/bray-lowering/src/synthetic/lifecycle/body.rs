use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockKind, MirCleanupEdge, MirEdge, MirHelperReference, MirPlace, MirProjection,
    MirProjectionKind, MirSourceAnchor, MirStorageKind, MirTargetContract, MirTerminatorKind,
    MirUnit, MirUnitBuilder, MirUnitId, MirUnitKey,
};
use bray_symbols::{BorrowKind, TypeData};

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};

/// Lowers one closed lifecycle specialization into its generated MIR body.
pub fn lower_lifecycle<C: SyntheticLoweringContext + ?Sized>(
    context: &C,
    key: MirUnitKey,
    reference: &MirHelperReference,
    unit: MirUnitId,
    target: &MirTargetContract,
) -> Result<MirUnit, C::Error> {
    SyntheticLowerer::new(context).lower_lifecycle(key, reference, unit, target)
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
            .ok_or_else(|| SyntheticLoweringError::MissingHelper(reference.clone()))?;

        let values = self.context.semantic_values();

        let pointer = values
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Mutable,
                target: ty,
            })
            .map_err(SyntheticLoweringError::SemanticValue)?;

        let source = MirSourceAnchor::generated_lifecycle(reference.clone());

        let role = bray_ir::MirGeneratedLifecycleRole::from_reference(reference)
            .ok_or_else(|| SyntheticLoweringError::MissingHelper(reference.clone()))?;

        let cleanup = self.context.cleanup_type_execution(ty)?;

        let execution = role
            .execution(&cleanup)
            .ok_or(SyntheticLoweringError::UnresolvedType(ty))?;

        let frame = (execution == bray_symbols::CallableExecution::Asynchronous)
            .then(|| crate::identity::protected_frame_identity(&key, target));

        let mut builder = MirUnitBuilder::for_generated_lifecycle(
            unit,
            key,
            reference.clone(),
            frame,
            target.clone(),
        );

        let entry = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .map_err(|cause| self.mir_error(&source, cause))?;

        let storage = builder
            .push_storage(source.clone(), MirStorageKind::Parameter(0), pointer)
            .map_err(|cause| self.mir_error(&source, cause))?;

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
            MirHelperReference::StaticFinalize(_) => {
                if cleanup.finalization_execution()
                    == Some(bray_symbols::CallableExecution::Asynchronous)
                {
                    let (created, rejected, value) = self.create_lifecycle_frame(
                        &mut builder,
                        entry,
                        &source,
                        bray_ir::MirGeneratedLifecycleRole::Finalize,
                        place,
                    )?;

                    let report = self
                        .context
                        .representation_type(RepresentationRole::PanicReport)?;

                    let report = crate::frame_creation::allocation_panic(
                        &mut builder,
                        rejected,
                        &source,
                        report,
                    )
                    .map_err(|cause| self.mir_error(&source, cause))?;

                    builder
                        .set_terminator(
                            rejected,
                            source.clone(),
                            MirTerminatorKind::PropagatePanic {
                                report,
                                runtime: bray_ir::MirRuntimeReference::new(
                                    bray_runtime_interface::RuntimeAbiRole::PanicPropagation,
                                    target.runtime_abi(),
                                ),
                            },
                        )
                        .map_err(|cause| self.mir_error(&source, cause))?;

                    builder
                        .set_terminator(
                            created,
                            source.clone(),
                            MirTerminatorKind::Return(Some(bray_ir::MirOperand::Move(value))),
                        )
                        .map_err(|cause| self.mir_error(&source, cause))?;
                } else {
                    let end = self.push_generated_lifecycle_operations(
                        &mut builder,
                        entry,
                        &source,
                        &MirHelperReference::Finalize(ty),
                        place,
                        target.runtime_abi(),
                    )?;

                    self.finish_lifecycle_body(&mut builder, end, &source)?;
                }
            }
            MirHelperReference::Abandon {
                action: bray_ir::MirAbandonmentAction::Quiesce,
                ..
            } => {
                self.lower_quiescence_body(
                    &mut builder,
                    entry,
                    &source,
                    place,
                    target.runtime_abi(),
                )?;
            }
            MirHelperReference::Finalize(_)
            | MirHelperReference::Destroy(_)
            | MirHelperReference::Abandon { .. } => {
                let body_entry = if frame.is_some() {
                    self.lifecycle_resolution_block(&mut builder, entry, &source)?
                } else {
                    entry
                };

                let end = self.push_generated_lifecycle_operations(
                    &mut builder,
                    body_entry,
                    &source,
                    reference,
                    place,
                    target.runtime_abi(),
                )?;

                self.finish_lifecycle_body(&mut builder, end, &source)?;
            }
            MirHelperReference::AnonymousCallable(_)
            | MirHelperReference::DeclaredCallable(_)
            | MirHelperReference::CallableDefault(_)
            | MirHelperReference::ConstructionDefault(_)
            | MirHelperReference::TypeForm(_)
            | MirHelperReference::Conversion(_)
            | MirHelperReference::BeginGenerator
            | MirHelperReference::PushGenerator
            | MirHelperReference::FinishGenerator
            | MirHelperReference::PanicReport
            | MirHelperReference::StandardLibrary(_)
            | MirHelperReference::CreateFrame(_)
            | MirHelperReference::ComposeAwaitedFrame(_)
            | MirHelperReference::DestroyTerminalTask => {
                return Err(SyntheticLoweringError::MissingHelper(reference.clone()).into());
            }
        }

        if let Some(frame) = frame {
            self.attach_lifecycle_frame(
                &mut builder,
                frame,
                entry,
                MirPlace::new(storage, [], pointer),
                &[],
                &source,
            )?;
        }

        builder
            .finish(entry)
            .map_err(|cause| self.mir_error(&source, cause))
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
            .map_err(|cause| self.mir_error(source, cause))?;

        let lifecycle = builder
            .push_block(source.clone(), MirBlockKind::LifecycleResolution)
            .map_err(|cause| self.mir_error(source, cause))?;

        builder
            .set_terminator(
                entry,
                source.clone(),
                MirTerminatorKind::BeginCleanup(MirCleanupEdge::new(
                    bray_ir::MirCleanupPhase::TaskCancellation,
                    MirEdge::new(broadcast, []),
                )),
            )
            .map_err(|cause| self.mir_error(source, cause))?;

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

        builder
            .set_terminator(
                broadcast_end,
                source.clone(),
                MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                    bray_ir::MirCleanupPhase::LifecycleResolution,
                    MirEdge::new(lifecycle, []),
                )),
            )
            .map_err(|cause| self.mir_error(source, cause))?;

        self.finish_lifecycle_body(builder, lifecycle_end, source)
    }
}

#[cfg(test)]
mod tests {
    use bray_compiler_known::{CompilerKnownDeclarationKey, RepresentationRole};
    use bray_ir::{
        MirCallableReference, MirCleanupPhase, MirGeneratedLifecycleKey, MirGeneratedLifecycleRole,
        MirHelperReference, MirOperationKind, MirProjectionKind, MirStandardLibraryHelper,
        MirUnitId, MirUnitKey,
    };
    use bray_symbols::{
        AvailableCompilerKnownSymbols, CallableExecution, CallableInstanceData, CallableSignature,
        ConstantTermId, DeclaredTypeRepresentation, GenericSubstitutionId, NamedTypeSymbolId,
        SemanticValueStore, TypeAssociatedLifecycleSlot, TypeData, TypeExpressionTemplate, TypeId,
    };

    use super::lower_lifecycle;
    use crate::{SyntheticLoweringContext, SyntheticLoweringError};

    struct TupleContext(SemanticValueStore);

    impl SyntheticLoweringContext for TupleContext {
        type Error = SyntheticLoweringError;

        fn finalization_complete(&self, _: TypeId) -> Result<bool, Self::Error> {
            Ok(false)
        }

        fn semantic_values(&self) -> &SemanticValueStore {
            &self.0
        }

        fn compiler_known_symbols(&self) -> &AvailableCompilerKnownSymbols {
            panic!("tuple lowering must not query compiler-known declarations");
        }

        fn cleanup_type_execution(
            &self,
            ty: TypeId,
        ) -> Result<bray_bound_tree::StorageCleanupType, Self::Error> {
            assert!(matches!(
                self.0.type_data(ty).unwrap().as_ref(),
                TypeData::Tuple(_) | TypeData::Nullable(_) | TypeData::Array { .. }
            ));

            let requirement = match self.0.type_data(ty).unwrap().as_ref() {
                TypeData::Tuple(elements) if elements.is_empty() => {
                    bray_bound_tree::AsyncStorageCleanupRequirement::None
                }
                _ => bray_bound_tree::AsyncStorageCleanupRequirement::Cleanup(
                    bray_bound_tree::AsyncCleanupPhases::Lifecycle,
                ),
            };

            Ok(
                bray_bound_tree::StorageCleanupType::new(ty, requirement).with_execution(
                    Some(CallableExecution::Synchronous),
                    Some(CallableExecution::Synchronous),
                    Some(CallableExecution::Synchronous),
                ),
            )
        }

        fn lifecycle_callable(
            &self,
            ty: TypeId,
            _: TypeAssociatedLifecycleSlot,
        ) -> Result<Option<(MirCallableReference, TypeId, TypeId, CallableExecution)>, Self::Error>
        {
            assert!(matches!(
                self.0.type_data(ty).unwrap().as_ref(),
                TypeData::Tuple(_) | TypeData::Nullable(_) | TypeData::Array { .. }
            ));

            Ok(None)
        }

        fn storage_callable(
            &self,
            _: TypeId,
            _: TypeId,
            _: &CompilerKnownDeclarationKey,
        ) -> Result<(MirCallableReference, CallableSignature), Self::Error> {
            panic!("tuple lowering must not select a storage policy");
        }

        fn declared_representation(
            &self,
            _: NamedTypeSymbolId,
        ) -> Result<DeclaredTypeRepresentation, Self::Error> {
            panic!("tuple lowering must not resolve declared representations");
        }

        fn resolve_type(
            &self,
            _: &TypeExpressionTemplate,
            _: GenericSubstitutionId,
        ) -> Result<TypeId, Self::Error> {
            panic!("tuple lowering already has closed element types");
        }

        fn raw_buffer_element(
            &self,
            _: NamedTypeSymbolId,
            _: GenericSubstitutionId,
        ) -> Result<Option<TypeId>, Self::Error> {
            panic!("tuple lowering must not resolve imported buffers");
        }

        fn representation_role(&self, _: NamedTypeSymbolId) -> Option<RepresentationRole> {
            panic!("tuple lowering must not resolve named representation roles");
        }

        fn representation_type(&self, role: RepresentationRole) -> Result<TypeId, Self::Error> {
            assert!(matches!(
                role,
                RepresentationRole::ScalarBool
                    | RepresentationRole::ScalarUsize
                    | RepresentationRole::PanicReport
                    | RepresentationRole::Unit
            ));

            self.0
                .intern_type(TypeData::tuple([]))
                .map_err(SyntheticLoweringError::SemanticValue)
        }

        fn array_length(&self, length: ConstantTermId) -> Result<u64, Self::Error> {
            let term = self.0.constant_term_data(length).unwrap();

            let bray_symbols::ConstantTermData::Value(value) = term.as_ref() else {
                panic!("test array length must be a closed constant");
            };

            let value = self.0.constant_value_data(*value).unwrap();

            let bray_symbols::ConstantValueKind::Integer(value) = value.kind() else {
                panic!("test array length must be an integer");
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
    fn inert_represented_cleanup_does_not_acquire_an_outcome() {
        let context = TupleContext(SemanticValueStore::try_new().unwrap());
        let leaf = context.0.intern_type(TypeData::tuple([])).unwrap();
        let role = MirGeneratedLifecycleRole::Destroy;

        let mir = lower_lifecycle(
            &context,
            MirUnitKey::GeneratedLifecycle(MirGeneratedLifecycleKey::new(role, [10; 32])),
            &role.reference(leaf),
            MirUnitId::new(10),
            &bray_testing::test_mir_target(),
        )
        .unwrap();

        assert!(mir.operations().is_empty());
        assert!(mir.frame_descriptor().is_none());
    }

    #[test]
    fn array_lifecycle_body_size_is_independent_of_length() {
        let context = TupleContext(SemanticValueStore::try_new().unwrap());
        let leaf = context.0.intern_type(TypeData::tuple([])).unwrap();

        for role in [
            MirGeneratedLifecycleRole::Destroy,
            MirGeneratedLifecycleRole::Cleanup(MirCleanupPhase::TaskCancellation),
            MirGeneratedLifecycleRole::Abandon(bray_ir::MirAbandonmentAction::Destroy),
        ] {
            let mut nonempty_size = None;

            for length in [0, 1, 4, u64::from(u32::MAX)] {
                let bray_ir::MirOperand::Constant { value, .. } =
                    crate::operand::integer_constant(&context.0, leaf, length).unwrap()
                else {
                    panic!("array length must be constant");
                };

                let length = context
                    .0
                    .intern_constant_term(bray_symbols::ConstantTermData::Value(value))
                    .unwrap();

                let array = context
                    .0
                    .intern_type(TypeData::Array {
                        element: leaf,
                        length,
                    })
                    .unwrap();

                let mir = lower_lifecycle(
                    &context,
                    MirUnitKey::GeneratedLifecycle(MirGeneratedLifecycleKey::new(role, [9; 32])),
                    &role.reference(array),
                    MirUnitId::new(9),
                    &bray_testing::test_mir_target(),
                )
                .unwrap();

                let array_length = context.array_length(length).unwrap();

                let indexed = mir
                    .operations()
                    .iter()
                    .filter_map(|operation| {
                        let place = match operation.kind() {
                            MirOperationKind::Finalize(place)
                            | MirOperationKind::Destroy(place)
                            | MirOperationKind::Cleanup { place, .. }
                            | MirOperationKind::Abandon { place, .. } => place,
                            _ => return None,
                        };

                        place.projections().last().filter(|projection| {
                            matches!(projection.kind(), MirProjectionKind::Index(_))
                        })
                    })
                    .count();

                if array_length == 0 {
                    assert_eq!(indexed, 0);
                } else {
                    assert!(indexed > 0);
                    let size = (mir.blocks().len(), mir.operations().len(), indexed);
                    assert_eq!(*nonempty_size.get_or_insert(size), size);
                }
            }
        }
    }

    #[test]
    fn specialized_frame_errors_preserve_source_instead_of_panicking() {
        let context = TupleContext(SemanticValueStore::try_new().unwrap());
        let lowerer = crate::synthetic::SyntheticLowerer::new(&context);

        let source =
            bray_ir::MirSourceAnchor::from(bray_testing::test_bound_unit(27).key().source());

        let cause = bray_ir::MirUnitBuildError::ProtectedFrameMismatch;

        assert_eq!(
            lowerer.mir_error(&source, cause),
            SyntheticLoweringError::SpecializedMir { source, cause }
        );
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
    fn synchronous_nullable_quiescence_keeps_nested_branches_in_the_resolution_phase() {
        let context = TupleContext(SemanticValueStore::try_new().unwrap());
        let leaf = context.0.intern_type(TypeData::tuple([])).unwrap();
        let nullable = context.0.intern_type(TypeData::Nullable(leaf)).unwrap();
        let role = MirGeneratedLifecycleRole::Abandon(bray_ir::MirAbandonmentAction::Quiesce);
        let reference = role.reference(nullable);
        let key = MirUnitKey::GeneratedLifecycle(MirGeneratedLifecycleKey::new(role, [8; 32]));

        let mir = lower_lifecycle(
            &context,
            key,
            &reference,
            MirUnitId::new(6),
            &bray_testing::test_mir_target(),
        )
        .unwrap();

        assert!(mir.frame_descriptor().is_none());

        assert!(mir.operations().iter().any(
            |operation| matches!(operation.kind(), MirOperationKind::Abandon {
            action: bray_ir::MirAbandonmentAction::Quiesce, place,
        } if place.ty() == leaf)
        ));

        assert!(!mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            MirOperationKind::Finalize(_) | MirOperationKind::Destroy(_)
        )));

        let phases = mir
            .blocks()
            .iter()
            .filter(|block| {
                matches!(
                    block.terminator().kind(),
                    bray_ir::MirTerminatorKind::PatternBranch { .. }
                )
            })
            .map(|block| block.kind())
            .collect::<Vec<_>>();

        assert_eq!(
            phases,
            [
                bray_ir::MirBlockKind::CleanupBroadcast,
                bray_ir::MirBlockKind::LifecycleResolution
            ]
        );
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
