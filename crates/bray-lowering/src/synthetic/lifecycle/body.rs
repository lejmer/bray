use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockKind, MirCleanupEdge, MirEdge, MirHelperReference, MirOperand, MirPlace, MirProjection,
    MirProjectionKind, MirSourceAnchor, MirStorageKind, MirTargetContract, MirTerminatorKind,
    MirUnit, MirUnitBuilder, MirUnitId, MirUnitKey,
};
use bray_symbols::{BorrowKind, TypeAssociatedLifecycleSlot, TypeData, TypeId};

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
            .ok_or_else(|| SyntheticLoweringError::MissingHelper(reference.clone()))?;

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
            MirHelperReference::StaticFinalize(ty) => {
                let mut end = entry;

                let return_value = if let Some(completed) = self
                    .push_compiler_known_lifecycle_operations(
                        &mut builder,
                        entry,
                        &source,
                        reference,
                        &place,
                        target.runtime_abi(),
                    )? {
                    end = completed;

                    None
                } else if let Some(callable) = self
                    .context
                    .lifecycle_callable(*ty, TypeAssociatedLifecycleSlot::Finalizer)?
                {
                    let returns_void = callable.3 == bray_symbols::CallableExecution::Synchronous
                        && self.is_void_result(callable.2)?;

                    let value = self.push_static_finalizer_call(
                        &mut builder,
                        entry,
                        &source,
                        place,
                        callable,
                    )?;

                    (!returns_void).then_some(MirOperand::Value(value))
                } else {
                    None
                };

                builder
                    .set_terminator(end, source.clone(), MirTerminatorKind::Return(return_value))
                    .map_err(|cause| self.mir_error(&source, cause))?;
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

                builder
                    .set_terminator(end, source.clone(), MirTerminatorKind::Return(None))
                    .map_err(|cause| self.mir_error(&source, cause))?;
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
            | MirHelperReference::MoveInactiveFrame(_)
            | MirHelperReference::ComposeAwaitedFrame(_)
            | MirHelperReference::CommitAwaitedCompletion(_)
            | MirHelperReference::DestroyTerminalTask => {
                return Err(SyntheticLoweringError::MissingHelper(reference.clone()).into());
            }
        }

        builder
            .finish(entry)
            .map_err(|cause| self.mir_error(&source, cause))
    }

    fn is_void_result(&self, ty: TypeId) -> Result<bool, C::Error> {
        let data = self
            .context
            .semantic_values()
            .type_data(ty)
            .map_err(SyntheticLoweringError::SemanticValue)?;

        let TypeData::Named { definition, .. } = data.as_ref() else {
            return Ok(false);
        };

        Ok(matches!(
            self.context.representation_role(*definition),
            Some(RepresentationRole::Unit | RepresentationRole::Never)
        ))
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

        builder
            .set_terminator(
                lifecycle_end,
                source.clone(),
                MirTerminatorKind::Return(None),
            )
            .map_err(|cause| self.mir_error(source, cause))?;

        Ok(())
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

        fn semantic_values(&self) -> &SemanticValueStore {
            &self.0
        }

        fn compiler_known_symbols(&self) -> &AvailableCompilerKnownSymbols {
            panic!("tuple lowering must not query compiler-known declarations");
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

        fn imported_raw_buffer_element(
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
                    | RepresentationRole::PanicReport
                    | RepresentationRole::Unit
                    | RepresentationRole::ScalarUsize
            ));

            self.0
                .intern_type(TypeData::tuple([]))
                .map_err(SyntheticLoweringError::SemanticValue)
        }

        fn array_length(&self, length: ConstantTermId) -> Result<u64, Self::Error> {
            let length = self.0.constant_term_data(length).unwrap();

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
