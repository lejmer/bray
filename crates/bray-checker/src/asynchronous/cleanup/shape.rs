use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AsyncCleanupPhases, AsyncStorageCleanupRequirement, AsyncStorageExitRecoveryCause,
    AsyncStorageRequirement, StorageCleanupType,
};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{GenericArgument, TypeData, TypeExpressionTemplate, TypeId};

use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct CleanupShape {
    pub(in crate::asynchronous) cancellation: bool,
    pub(in crate::asynchronous) lifecycle: bool,
    pub(crate) graceful: bool,
    pub(super) suspending: bool,
    pub(super) quiescence: bool,
    pub(super) recovered: bool,
}

impl CleanupShape {
    const BOTH: Self = Self {
        cancellation: true,
        lifecycle: true,
        graceful: true,
        suspending: true,
        quiescence: true,
        recovered: false,
    };

    const LIFECYCLE: Self = Self {
        cancellation: false,
        lifecycle: true,
        graceful: false,
        suspending: false,
        quiescence: false,
        recovered: false,
    };

    pub(in crate::asynchronous) fn opaque(role: RepresentationRole) -> Option<Self> {
        match role {
            RepresentationRole::Future => Some(Self::BOTH),
            RepresentationRole::Task => Some(Self {
                suspending: false,
                quiescence: false,
                ..Self::BOTH
            }),
            RepresentationRole::String | RepresentationRole::PanicReport => Some(Self::LIFECYCLE),
            _ => None,
        }
    }

    fn merge(&mut self, other: Self) {
        self.cancellation |= other.cancellation;
        self.lifecycle |= other.lifecycle;
        self.graceful |= other.graceful;
        self.suspending |= other.suspending;
        self.quiescence |= other.quiescence;
        self.recovered |= other.recovered;
    }
}

pub(crate) struct CleanupShapeResolver<'request, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(in crate::asynchronous) request: CheckerUnitView<'request, C>,
    pub(crate) completed: std::sync::Arc<BTreeMap<TypeId, CleanupShape>>,
    pub(in crate::asynchronous) cleanup_types: BTreeMap<TypeId, StorageCleanupType>,
    pub(in crate::asynchronous) recursive_cleanup: bool,
    pub(in crate::asynchronous) expanded_types: BTreeSet<TypeId>,
    pub(in crate::asynchronous) full_parts:
        BTreeMap<TypeId, std::sync::Arc<[(TypeId, Option<bray_bound_tree::StorageCleanupPart>)]>>,
    pub(super) requirements: Option<std::sync::Arc<[AsyncStorageRequirement]>>,
    active: BTreeSet<TypeId>,
    // Query diagnostics are cloned because the cleanup result owns them independently.
    pub(crate) diagnostics: DiagnosticBag,
}

impl<'request, C> CleanupShapeResolver<'request, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(crate) fn new(request: CheckerUnitView<'request, C>) -> Self {
        Self {
            request,
            completed: std::sync::Arc::new(BTreeMap::new()),
            cleanup_types: BTreeMap::new(),
            recursive_cleanup: false,
            expanded_types: BTreeSet::new(),
            full_parts: BTreeMap::new(),
            requirements: None,
            active: BTreeSet::new(),
            diagnostics: DiagnosticBag::new(),
        }
    }

    pub(crate) fn requires_suspension(
        &mut self,
        ty: TypeId,
    ) -> Result<bool, CheckerQueryError<C::UpstreamError>> {
        self.resolve(ty).map(|shape| shape.suspending)
    }

    pub(crate) fn quiescence_requires_suspension(
        &mut self,
        ty: TypeId,
    ) -> Result<bool, CheckerQueryError<C::UpstreamError>> {
        self.resolve(ty).map(|shape| shape.quiescence)
    }

    pub(crate) fn requirement(
        &mut self,
        ty: TypeId,
    ) -> Result<AsyncStorageCleanupRequirement, CheckerQueryError<C::UpstreamError>> {
        let cleanup = cleanup_requirement(self.resolve(ty)?);

        self.cleanup_types
            .entry(ty)
            .or_insert_with(|| StorageCleanupType::new(ty, cleanup));

        Ok(cleanup)
    }

    pub(in crate::asynchronous) fn require_inputs(
        &mut self,
        selections: &bray_bound_tree::CheckedSemanticSelections,
        types: &bray_bound_tree::CheckedExpressionTypes,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        for entry in selections.entries() {
            match entry.selection() {
                bray_bound_tree::SemanticSelection::Operation(
                    bray_bound_tree::SelectedOperation::Construction(construction),
                ) => {
                    self.requirement(construction.result_type())?;

                    for input in construction.inputs() {
                        match input {
                            bray_bound_tree::SelectedConstructionInput::Explicit {
                                conversion,
                                ..
                            } => self.require_conversion(conversion)?,
                            bray_bound_tree::SelectedConstructionInput::Default { ty, .. } => {
                                self.requirement(*ty)?;
                            }
                        }
                    }
                }
                bray_bound_tree::SemanticSelection::Operation(
                    bray_bound_tree::SelectedOperation::Conversion(conversion),
                ) => {
                    self.require_conversion(conversion)?;
                }
                bray_bound_tree::SemanticSelection::Operation(
                    bray_bound_tree::SelectedOperation::Index { target, .. },
                ) if target.custom_borrow_kind().is_some() => {
                    let Some(bray_bound_tree::BoundExpression::Structured(source)) =
                        self.request.view().expression(entry.expression())
                    else {
                        panic!("selected custom indexing retains its structured source");
                    };

                    if let Some(bounds) = source.slice_bounds() {
                        let bound = bounds
                            .lower()
                            .or(bounds.upper())
                            .expect("checked custom slice has a bound supplying its type");

                        let bound_type = types
                            .expression(bound)
                            .expect("checked slice bound retains its type")
                            .ty();

                        let nullable = self
                            .request
                            .semantic_values()
                            .intern_type(bray_symbols::TypeData::Nullable(bound_type))
                            .expect("checked slice bound admits its nullable input type");

                        self.requirement(nullable)?;
                    }
                }
                bray_bound_tree::SemanticSelection::Call(call) => {
                    for argument in call.arguments() {
                        match argument {
                            bray_bound_tree::SelectedArgument::Explicit { conversion, .. } => {
                                self.require_conversion(conversion)?
                            }
                            bray_bound_tree::SelectedArgument::Default { ty, .. } => {
                                self.requirement(*ty)?;
                            }
                        }
                    }

                    if let Some(receiver) = call.receiver() {
                        let ty = receiver
                            .input_type(self.request.semantic_values()).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in scope_exit_plans: {error:?}"));

                        self.requirement(ty)?;
                    }
                }
                _ => {}
            }
        }

        for (id, expression) in self.request.unit().tree().expressions() {
            if matches!(expression, bray_bound_tree::BoundExpression::Await(_)) {
                // A completed await owns its result before delivering cancellation.
                // Publish its cleanup shape with the other implicit input owners.
                let ty = types
                    .expression(id)
                    .expect("checked await retains its completion type")
                    .ty();

                self.requirement(ty)?;
            }

            let bray_bound_tree::BoundExpression::Structured(expression) = expression else {
                continue;
            };

            use bray_bound_tree::BoundStructuredExpressionKind as Kind;

            if matches!(
                expression.kind(),
                Kind::Tuple | Kind::Array | Kind::RepeatedArray | Kind::Range
            ) {
                for operand in expression.operands() {
                    let ty = types
                        .expression(*operand).unwrap_or_else(|| panic!("scope_exit_plans requires checked expression type or node, operand: {operand:?}"))
                        .ty();

                    self.requirement(ty)?;
                }
            }
        }

        Ok(())
    }

    pub(crate) fn require_conversion(
        &mut self,
        conversion: &bray_bound_tree::SelectedConversion,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        let mut pending = vec![conversion];

        while let Some(conversion) = pending.pop() {
            self.requirement(conversion.source_type())?;
            self.requirement(conversion.target_type())?;

            if let bray_bound_tree::ConversionTarget::Composite(children) = conversion.target() {
                pending.extend(children.iter());
            }
        }

        Ok(())
    }

    pub(crate) fn resolve(
        &mut self,
        ty: TypeId,
    ) -> Result<CleanupShape, CheckerQueryError<C::UpstreamError>> {
        if let Some(shape) = self.completed.get(&ty) {
            return Ok(*shape);
        }

        if !self.active.insert(ty) {
            return Ok(CleanupShape::BOTH);
        }

        let data = self.request.semantic_values().type_data(ty);

        let shape = match data.as_ref() {
            TypeData::Error => CleanupShape {
                recovered: true,
                ..CleanupShape::BOTH
            },
            TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_)
            | TypeData::TypeValuedMemberProjection { .. }
            | TypeData::Generator(_) => CleanupShape::BOTH,
            TypeData::Named {
                definition,
                substitution,
            } => self.named_shape(ty, *definition, *substitution)?,
            TypeData::Tuple(elements) => self.aggregate(elements.iter().copied())?,
            TypeData::Array { element, .. } | TypeData::Nullable(element) => {
                self.resolve(*element)?
            }
            TypeData::OwnedIndirection { target, .. } => {
                let mut shape = self.resolve(*target)?;

                shape.lifecycle = true;

                shape
            }
            TypeData::ExactInteger { .. }
            | TypeData::FlexibleArray(_)
            | TypeData::Slice(_)
            | TypeData::Borrow { .. }
            | TypeData::Callable(_)
            | TypeData::TraitView(_) => CleanupShape::default(),
        };

        self.active.remove(&ty);

        // Prepared consumers share all known shapes. A newly demanded type extends
        // only this resolver's cache, without mutating the published preparation.
        std::sync::Arc::make_mut(&mut self.completed).insert(ty, shape);

        Ok(shape)
    }

    fn named_shape(
        &mut self,
        ty: TypeId,
        definition: bray_symbols::NamedTypeSymbolId,
        substitution: bray_symbols::GenericSubstitutionId,
    ) -> Result<CleanupShape, CheckerQueryError<C::UpstreamError>> {
        let role = match definition {
            bray_symbols::NamedTypeSymbolId::Struct(definition) => self
                .request
                .available_compiler_known_symbols()
                .provider()
                .role_registry()
                .symbol_representation(definition),
            bray_symbols::NamedTypeSymbolId::Union(definition) => self
                .request
                .available_compiler_known_symbols()
                .provider()
                .role_registry()
                .symbol_representation(definition),
        };

        if let Some(shape) = role.and_then(CleanupShape::opaque) {
            return Ok(shape);
        }

        match role {
            Some(
                RepresentationRole::Result
                | RepresentationRole::RunResult
                | RepresentationRole::ConversionError,
            ) => {
                let substitution = self
                    .request
                    .semantic_values()
                    .generic_substitution_data(substitution);

                self.aggregate(substitution.bindings().iter().filter_map(|binding| {
                    match binding.argument() {
                        GenericArgument::Type(ty) => Some(ty),
                        GenericArgument::Constant(_) => None,
                    }
                }))
            }
            Some(RepresentationRole::Uninit) => {
                let substitution = self
                    .request
                    .semantic_values()
                    .generic_substitution_data(substitution);

                let payload =
                    self.aggregate(substitution.bindings().iter().filter_map(|binding| {
                        match binding.argument() {
                            GenericArgument::Type(ty) => Some(ty),
                            GenericArgument::Constant(_) => None,
                        }
                    }))?;

                Ok(CleanupShape {
                    graceful: payload.graceful,
                    ..CleanupShape::default()
                })
            }
            Some(_) => Ok(CleanupShape::default()),
            None => self.declared_shape(ty, definition, substitution),
        }
    }

    fn declared_shape(
        &mut self,
        ty: TypeId,
        definition: bray_symbols::NamedTypeSymbolId,
        substitution: bray_symbols::GenericSubstitutionId,
    ) -> Result<CleanupShape, CheckerQueryError<C::UpstreamError>> {
        if let Some(element) = self
            .request
            .context()
            .raw_buffer_element(definition, substitution)?
        {
            let mut shape = self.resolve(element)?;

            shape.lifecycle = true;

            return Ok(shape);
        }

        let representation = self.request.declared_type_representation(definition)?;
        let lifecycle = self.request.declared_type_lifecycle(definition)?;

        let mut shape = CleanupShape {
            lifecycle: *lifecycle.value() != bray_symbols::TypeLifecycle::None,
            graceful: *lifecycle.value() == bray_symbols::TypeLifecycle::Graceful,
            recovered: representation.value().is_recovered()
                || lifecycle.diagnostics().has_errors(),
            ..CleanupShape::default()
        };

        self.diagnostics
            .add_range(representation.diagnostics().clone());

        self.diagnostics.add_range(lifecycle.diagnostics().clone());

        for member in representation.value().storage().member_types() {
            shape.merge(self.member_shape(member, substitution)?);
        }

        if *lifecycle.value() == bray_symbols::TypeLifecycle::Graceful {
            let selected = self
                .request
                .context()
                .lifecycle_callable(ty, bray_symbols::TypeAssociatedLifecycleSlot::Finalizer)?;

            self.diagnostics
                .add_range(selected.diagnostics().iter().cloned());

            if let Some((_, signature)) = selected.value() {
                let data = self
                    .request
                    .semantic_values()
                    .type_data(signature.callable_type());

                let TypeData::Callable(callable) = data.as_ref() else {
                    panic!("selected finalizer has its checked callable type");
                };

                shape.suspending |= callable.execution()
                    == bray_symbols::CallableExecution::Asynchronous
                    || self.resolve(callable.result())?.cancellation;
            }
        }

        Ok(shape)
    }

    fn member_shape(
        &mut self,
        template: &TypeExpressionTemplate,
        substitution: bray_symbols::GenericSubstitutionId,
    ) -> Result<CleanupShape, CheckerQueryError<C::UpstreamError>> {
        let Some(ty) = self.member_type(template, substitution)? else {
            return Ok(CleanupShape {
                recovered: true,
                ..CleanupShape::BOTH
            });
        };

        self.resolve(ty)
    }

    pub(in crate::asynchronous) fn member_type(
        &mut self,
        template: &TypeExpressionTemplate,
        substitution: bray_symbols::GenericSubstitutionId,
    ) -> Result<Option<TypeId>, CheckerQueryError<C::UpstreamError>> {
        let checked =
            crate::constant::checked_substituted_type(self.request, template, substitution)?;

        let (ty, diagnostics) = checked.into_parts();

        self.diagnostics.add_range(diagnostics);

        Ok(ty)
    }

    fn aggregate(
        &mut self,
        types: impl IntoIterator<Item = TypeId>,
    ) -> Result<CleanupShape, CheckerQueryError<C::UpstreamError>> {
        let mut shape = CleanupShape::default();

        for ty in types {
            shape.merge(self.resolve(ty)?);
        }

        Ok(shape)
    }

    pub(in crate::asynchronous) fn report_incomplete_lifecycle(
        &mut self,
        ty: TypeId,
        source: bray_bound_tree::BoundSourceAnchor,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        use bray_diagnostics::{
            Diagnostic, DiagnosticArg, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
            SeverityKind,
        };

        let span = self.request.source(source).span();
        let ty = crate::diagnostic::diagnostic_type(self.request.context(), ty)?;

        self.diagnostics.add(
            Diagnostic::new(
                crate::diagnostic::diagnostic_id(self.diagnostics.len()),
                DiagnosticKind::CheckingIncompleteLifecycleStorage,
                SeverityKind::Error,
            )
            .with_primary_span(span)
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::IncompleteLifecycleStorage,
                span,
            ))
            .with_arg(DiagnosticArg::actual_type(ty)),
        );

        Ok(())
    }
}

pub(in crate::asynchronous) const fn cleanup_requirement(
    shape: CleanupShape,
) -> AsyncStorageCleanupRequirement {
    if shape.recovered {
        return AsyncStorageCleanupRequirement::Recovered(
            AsyncStorageExitRecoveryCause::UnavailableCleanupShape,
        );
    }

    match (shape.cancellation, shape.lifecycle) {
        (true, true) => {
            AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::CancellationThenLifecycle)
        }
        (true, false) => AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::Cancellation),
        (false, true) => AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::Lifecycle),
        (false, false) => AsyncStorageCleanupRequirement::None,
    }
}
