use std::sync::Arc;

use bray_bound_tree::{LifecycleAction, LifecycleCallable, LifecyclePhase};
use bray_compiler_known::{CompilerKnownDeclarationKey, RepresentationRole};
use bray_symbols::{
    DeclaredStorageShape, DeclaredTypeRepresentation, GenericSubstitutionId, NamedTypeSymbolId,
    SemanticValueStore, TypeAssociatedLifecycleSlot, TypeData, TypeExpressionTemplate, TypeId,
};

/// Semantic queries required to select a local lifecycle action.
pub trait LifecycleSelectionContext {
    /// Retains query failures and invalid semantic inputs.
    type Error: From<LifecycleSelectionError>;

    /// Returns interned semantic values.
    fn semantic_values(&self) -> &SemanticValueStore;
    /// Returns the selected whole-value implementation, if present.
    fn lifecycle_callable(
        &self,
        ty: TypeId,
        slot: TypeAssociatedLifecycleSlot,
    ) -> Result<Option<LifecycleCallable>, Self::Error>;
    /// Returns a selected unary storage-policy operation and its substituted signature.
    fn storage_callable(
        &self,
        storage: TypeId,
        target: TypeId,
        member: &CompilerKnownDeclarationKey,
    ) -> Result<LifecycleCallable, Self::Error>;
    /// Returns the checked stored representation of a declaration.
    fn declared_representation(
        &self,
        definition: NamedTypeSymbolId,
    ) -> Result<DeclaredTypeRepresentation, Self::Error>;
    /// Substitutes a represented member type.
    fn resolve_type(
        &self,
        template: &TypeExpressionTemplate,
        substitution: GenericSubstitutionId,
    ) -> Result<TypeId, Self::Error>;
    /// Recognizes a source or imported buffer under the standard-library authority contract.
    fn raw_buffer_element(
        &self,
        definition: NamedTypeSymbolId,
        substitution: GenericSubstitutionId,
    ) -> Result<Option<TypeId>, Self::Error>;
    /// Returns a declaration's compiler-known representation role.
    fn representation_role(&self, definition: NamedTypeSymbolId) -> Option<RepresentationRole>;
}

/// A semantic input that cannot select a lifecycle action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LifecycleSelectionError {
    /// An internal storage protocol identity is malformed.
    InvalidStorageMemberKey(String),
    /// The type has no supported represented cleanup.
    UnsupportedType(TypeId),
    /// The declared representation remains unresolved.
    UnresolvedType(TypeId),
    /// A borrowed or callable value incorrectly requires destruction.
    UnexpectedType {
        /// Requested type.
        ty: TypeId,
        /// Exact semantic input.
        actual: TypeData,
    },
}

/// Selects only this owner's action, leaving child owners demand-driven.
/// Repeated child types remain separate entries in declaration order.
pub fn select_lifecycle_action<C: LifecycleSelectionContext + ?Sized>(
    context: &C,
    ty: TypeId,
    phase: LifecyclePhase,
) -> Result<LifecycleAction, C::Error> {
    let data = context.semantic_values().type_data(ty);

    if let TypeData::Named {
        definition,
        substitution,
    } = data.as_ref()
    {
        if matches!(
            phase,
            LifecyclePhase::Destroy | LifecyclePhase::Cancel | LifecyclePhase::Quiesce
        ) && let Some(element) = context.raw_buffer_element(*definition, *substitution)?
        {
            return Ok(LifecycleAction::RawBuffer(element));
        }

        let release = matches!(phase, LifecyclePhase::Destroy | LifecyclePhase::Resolve);

        match context.representation_role(*definition) {
            Some(RepresentationRole::String) => {
                return Ok(if release {
                    LifecycleAction::ReleaseString
                } else {
                    LifecycleAction::None
                });
            }
            Some(RepresentationRole::PanicReport) => {
                return Ok(if release {
                    LifecycleAction::DestroyReport
                } else {
                    LifecycleAction::None
                });
            }
            Some(RepresentationRole::Task) => return Ok(LifecycleAction::Task),
            Some(RepresentationRole::Future) if phase == LifecyclePhase::Quiesce => {
                panic!(
                    "inactive future {ty:?} quiescence requires its admitted capture-cleanup activation"
                );
            }
            _ => {}
        }
    }

    let slot = match phase {
        LifecyclePhase::Finalize => Some(TypeAssociatedLifecycleSlot::Finalizer),
        LifecyclePhase::Destroy => Some(TypeAssociatedLifecycleSlot::Destructor),
        LifecyclePhase::Cancel | LifecyclePhase::Quiesce | LifecyclePhase::Resolve => None,
    };

    if let Some(slot) = slot
        && let Some(callable) = context.lifecycle_callable(ty, slot)?
    {
        return Ok(LifecycleAction::Call(callable));
    }

    match phase {
        LifecyclePhase::Finalize => Ok(LifecycleAction::None),
        LifecyclePhase::Resolve => Ok(LifecycleAction::Resolve),
        LifecyclePhase::Destroy | LifecyclePhase::Cancel | LifecyclePhase::Quiesce => {
            select_represented_action(context, ty, phase, &data)
        }
    }
}

fn select_represented_action<C: LifecycleSelectionContext + ?Sized>(
    context: &C,
    ty: TypeId,
    phase: LifecyclePhase,
    data: &TypeData,
) -> Result<LifecycleAction, C::Error> {
    match data {
        TypeData::Tuple(elements) => Ok(LifecycleAction::Members(Arc::clone(elements))),
        TypeData::Array { element, length } => Ok(LifecycleAction::Array {
            element: *element,
            length: *length,
        }),
        TypeData::Nullable(target) => Ok(LifecycleAction::Nullable(*target)),
        TypeData::OwnedIndirection { storage, target } => {
            let select = |name: &str| {
                let member = CompilerKnownDeclarationKey::try_new(name).ok_or_else(|| {
                    LifecycleSelectionError::InvalidStorageMemberKey(name.to_owned())
                })?;

                context.storage_callable(*storage, *target, &member)
            };

            let borrow = select("StorageBorrowMut")?;

            let teardown = if phase == LifecyclePhase::Destroy {
                Some([select("StorageDestroy")?, select("StorageRelease")?])
            } else {
                None
            };

            Ok(LifecycleAction::Owned {
                storage: *storage,
                target: *target,
                borrow,
                teardown,
            })
        }
        TypeData::Generator(element) => Ok(LifecycleAction::Generator(*element)),
        TypeData::Named {
            definition,
            substitution,
        } => {
            if matches!(definition, NamedTypeSymbolId::Struct(_))
                && context.representation_role(*definition).is_some()
            {
                return Err(LifecycleSelectionError::UnsupportedType(ty).into());
            }

            let representation = context.declared_representation(*definition)?;

            match (definition, representation.storage()) {
                (NamedTypeSymbolId::Struct(_), DeclaredStorageShape::Structure(members)) => {
                    Ok(LifecycleAction::Members(member_types(
                        context,
                        members.iter().map(|member| member.ty()),
                        *substitution,
                    )?))
                }
                (NamedTypeSymbolId::Union(_), DeclaredStorageShape::Union(variants)) => {
                    let alternatives = variants
                        .iter()
                        .map(|variant| {
                            Ok((
                                variant.variant(),
                                member_types(
                                    context,
                                    variant.members().iter().map(|member| member.ty()),
                                    *substitution,
                                )?,
                            ))
                        })
                        .collect::<Result<Arc<[_]>, C::Error>>()?;

                    Ok(LifecycleAction::Alternatives(alternatives))
                }
                _ => Err(LifecycleSelectionError::UnresolvedType(ty).into()),
            }
        }
        TypeData::Borrow { .. } | TypeData::Callable(_) => {
            Err(LifecycleSelectionError::UnexpectedType {
                ty,
                // The failure owns the exact semantic input beyond this query borrow.
                actual: data.clone(),
            }
            .into())
        }
        _ => Err(LifecycleSelectionError::UnsupportedType(ty).into()),
    }
}

fn member_types<'a, C: LifecycleSelectionContext + ?Sized>(
    context: &C,
    members: impl Iterator<Item = &'a TypeExpressionTemplate>,
    substitution: GenericSubstitutionId,
) -> Result<Arc<[TypeId]>, C::Error> {
    members
        .map(|member| context.resolve_type(member, substitution))
        .collect()
}

/// Returns requirements of invoking a checked lifecycle callable. Async body
/// requirements belong to its child frame, while invocation requirements remain local.
pub fn invocation_execution_requirements(
    values: &SemanticValueStore,
    known: &bray_symbols::AvailableCompilerKnownSymbols,
    callable_type: TypeId,
) -> std::collections::BTreeSet<bray_symbols::CallableExecutionRequirement> {
    let data = values.type_data(callable_type);

    let TypeData::Callable(callable) = data.as_ref() else {
        panic!("selected cleanup has a checked callable type");
    };

    let mut requirements = std::collections::BTreeSet::new();

    requirements.extend(
        callable
            .phase_behaviors()
            .invocation()
            .execution_requirements()
            .iter()
            .copied(),
    );

    // Declared source requirements are available before body-effect inference.
    // Imported callable signatures already include their phase requirements.
    if callable.execution() == bray_symbols::CallableExecution::Synchronous {
        for condition in callable
            .phase_behaviors()
            .invocation()
            .predicate_requirements()
            .iter()
            .filter_map(|predicate| predicate.condition())
        {
            let term = values.constant_term_data(condition);

            let bray_symbols::ConstantTermData::PredicateCall { predicate, .. } = term.as_ref()
            else {
                continue;
            };

            let declaration = predicate.definition().into_any();

            if known
                .symbol_implementation(declaration)
                .is_some_and(|hook| hook.is_execution_requirement())
            {
                requirements.insert(bray_symbols::CallableExecutionRequirement::new(declaration));
            }
        }
    }

    // Deferred lifecycle execution is dispatched in its own described child frame.
    requirements
}

/// The certified disposition of a selected finalizer at an ownership end.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinalizerCompletion {
    /// No certified successful completion is available for this invocation.
    Unresolved,
    /// Invocation can only return success, but must still execute.
    Infallible,
    /// The graceful step is pure, total and successful, so invocation is unnecessary.
    Completed,
}

/// Checks implicit finalization against its certified completion and current execution context.
/// Abnormal exit permits unresolved fallible duties, but cannot create an async execution root.
pub fn check_implicit_finalization<C: crate::CheckerRequestContext + ?Sized>(
    request: crate::CheckerUnitView<'_, C>,
    node: bray_bound_tree::AnyBoundNodeId,
    ty: TypeId,
    cleanup: &crate::ExecutionCleanup,
    normal: bool,
    completion: FinalizerCompletion,
    diagnostics: &mut bray_diagnostics::DiagnosticBag,
) -> Result<(), crate::CheckerQueryError<C::UpstreamError>> {
    use bray_bound_tree::BoundUnitRoot;

    use bray_diagnostics::{
        Diagnostic, DiagnosticArg, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
        DiagnosticNote, DiagnosticNoteKind, SeverityKind,
    };

    use bray_symbols::CallableExecution;

    if completion == FinalizerCompletion::Completed {
        return Ok(());
    }

    let asynchronous = matches!(
        request.unit().root(),
        BoundUnitRoot::CallableBody {
            execution: CallableExecution::Asynchronous,
            ..
        } | BoundUnitRoot::AnonymousCallable {
            execution: CallableExecution::Asynchronous,
            ..
        }
    );

    let unit_result = crate::representation::type_representation(request, cleanup.result)
        == Some(RepresentationRole::Unit);

    let kind = if normal && !unit_result && completion == FinalizerCompletion::Unresolved {
        DiagnosticKind::CheckingUnresolvedFinalization
    } else if !cleanup.synchronous && !asynchronous {
        DiagnosticKind::CheckingAsyncFinalizationOutsideAsyncCallable
    } else {
        return Ok(());
    };

    let origin = crate::diagnostic::bound_node_origin(request, node)
        .expect("selected cleanup retains its source ownership end");

    let span = request.source(origin.source_anchor()).span();
    let ty = crate::diagnostic_type(request.context(), ty)?;

    diagnostics.add(
        Diagnostic::new(
            crate::diagnostic::diagnostic_id(diagnostics.len()),
            kind,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::actual_type(ty))
        .with_primary_span(span)
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::UnresolvedFinalization,
            span,
        ))
        .with_note(DiagnosticNote::new(DiagnosticNoteKind::ResolveFinalization)),
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{LifecycleSelectionContext, LifecycleSelectionError, select_lifecycle_action};
    use bray_bound_tree::{LifecycleAction, LifecycleCallable, LifecyclePhase};
    use bray_compiler_known::RepresentationRole;
    use bray_symbols::{
        ConstantTermData, DeclaredTypeRepresentation, GenericSubstitutionId, IntegerConstant,
        NamedTypeSymbolId, SemanticValueStore, TargetSizedIntegerType, TypeAssociatedLifecycleSlot,
        TypeData, TypeExpressionTemplate, TypeId,
    };

    struct Context(SemanticValueStore);

    impl LifecycleSelectionContext for Context {
        type Error = LifecycleSelectionError;
        fn semantic_values(&self) -> &SemanticValueStore {
            &self.0
        }
        fn lifecycle_callable(
            &self,
            _: TypeId,
            _: TypeAssociatedLifecycleSlot,
        ) -> Result<Option<LifecycleCallable>, Self::Error> {
            Ok(None)
        }
        fn storage_callable(
            &self,
            _: TypeId,
            _: TypeId,
            _: &bray_compiler_known::CompilerKnownDeclarationKey,
        ) -> Result<LifecycleCallable, Self::Error> {
            panic!("no storage policy in this fixture")
        }
        fn declared_representation(
            &self,
            _: NamedTypeSymbolId,
        ) -> Result<DeclaredTypeRepresentation, Self::Error> {
            panic!("no declared type in this fixture")
        }
        fn resolve_type(
            &self,
            _: &TypeExpressionTemplate,
            _: GenericSubstitutionId,
        ) -> Result<TypeId, Self::Error> {
            panic!("no member substitution in this fixture")
        }
        fn raw_buffer_element(
            &self,
            _: NamedTypeSymbolId,
            _: GenericSubstitutionId,
        ) -> Result<Option<TypeId>, Self::Error> {
            panic!("no imported type in this fixture")
        }
        fn representation_role(&self, _: NamedTypeSymbolId) -> Option<RepresentationRole> {
            panic!("no named type in this fixture")
        }
    }

    #[test]
    fn repeated_members_preserve_distinct_ownership_positions() {
        let context = Context(SemanticValueStore::try_new().unwrap());
        let leaf = context.0.intern_type(TypeData::tuple([])).unwrap();

        let ty = context
            .0
            .intern_type(TypeData::tuple([leaf, leaf, leaf]))
            .unwrap();

        assert_eq!(
            select_lifecycle_action(&context, ty, LifecyclePhase::Destroy).unwrap(),
            LifecycleAction::Members([leaf, leaf, leaf].into())
        );

        for phase in [LifecyclePhase::Cancel, LifecyclePhase::Quiesce] {
            assert_eq!(
                select_lifecycle_action(&context, ty, phase).unwrap(),
                LifecycleAction::Members([leaf, leaf, leaf].into())
            );
        }
    }

    #[test]
    fn array_selection_retains_symbolic_extent_without_visiting_elements() {
        let context = Context(SemanticValueStore::try_new().unwrap());
        let element = context.0.intern_type(TypeData::tuple([])).unwrap();

        for extent in [0, 1, u64::MAX] {
            let length = context
                .0
                .intern_constant_term(ConstantTermData::IntegerLiteral {
                    ty: TargetSizedIntegerType::Usize,
                    value: IntegerConstant::from_u64(extent),
                })
                .unwrap();

            let ty = context
                .0
                .intern_type(TypeData::Array { element, length })
                .unwrap();

            assert_eq!(
                select_lifecycle_action(&context, ty, LifecyclePhase::Destroy).unwrap(),
                LifecycleAction::Array { element, length }
            );
        }
    }

    #[test]
    fn resolution_keeps_finalization_and_destruction_as_separate_obligations() {
        let context = Context(SemanticValueStore::try_new().unwrap());
        let leaf = context.0.intern_type(TypeData::tuple([])).unwrap();
        let ty = context.0.intern_type(TypeData::Nullable(leaf)).unwrap();

        assert_eq!(
            select_lifecycle_action(&context, ty, LifecyclePhase::Finalize).unwrap(),
            LifecycleAction::None
        );

        assert_eq!(
            select_lifecycle_action(&context, ty, LifecyclePhase::Resolve).unwrap(),
            LifecycleAction::Resolve
        );

        assert_eq!(
            select_lifecycle_action(&context, ty, LifecyclePhase::Destroy).unwrap(),
            LifecycleAction::Nullable(leaf)
        );
    }
}
