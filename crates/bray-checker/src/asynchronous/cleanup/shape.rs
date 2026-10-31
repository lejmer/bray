use std::collections::{BTreeMap, BTreeSet, VecDeque};

use bray_bound_tree::{
    AsyncCleanupPhases, AsyncStorageCleanupRequirement, AsyncStorageExitRecoveryCause,
    AsyncStorageRequirement, StorageCleanupType,
};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{GenericArgument, TypeData, TypeExpressionTemplate, TypeId};

use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

/// A concrete cleanup subject with its optional represented subpart.
pub(crate) type ResolvedCleanupPart = (TypeId, Option<bray_bound_tree::StorageCleanupPart>);

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

    const RECOVERED: Self = Self {
        recovered: true,
        ..Self::BOTH
    };

    pub(in crate::asynchronous) fn opaque(role: RepresentationRole) -> Option<Self> {
        match role {
            RepresentationRole::Future | RepresentationRole::Task => Some(Self::BOTH),
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

#[derive(Clone, Copy)]
enum CleanupShapeInput {
    Contents(TypeId),
    Uninitialized(TypeId),
    FinalizerResult(TypeId),
}

impl CleanupShapeInput {
    const fn ty(self) -> TypeId {
        match self {
            Self::Contents(ty) | Self::Uninitialized(ty) | Self::FinalizerResult(ty) => ty,
        }
    }

    fn merge(self, target: &mut CleanupShape, shape: CleanupShape) {
        match self {
            Self::Contents(_) => target.merge(shape),
            Self::Uninitialized(_) => {
                target.graceful |= shape.graceful;
                target.recovered |= shape.recovered;
            }
            Self::FinalizerResult(_) => {
                target.suspending |= shape.cancellation;
                target.recovered |= shape.recovered;
            }
        }
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
        BTreeMap<TypeId, std::sync::Arc<[crate::asynchronous::ResolvedCleanupPart]>>,
    pub(super) requirements: Option<std::sync::Arc<[AsyncStorageRequirement]>>,
    limit_exceeded: bool,
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
            limit_exceeded: false,
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
                Kind::ArrayGenerator | Kind::GeneralGenerator
            ) {
                let ty = types
                    .expression(id)
                    .expect("checked generator retains its result type")
                    .ty();

                self.requirement(ty)?;
            }

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
        if self.request.is_cancelled() {
            return Err(CheckerQueryError::Cancelled);
        }

        if let Some(shape) = self.completed.get(&ty) {
            return Ok(*shape);
        }

        if self.limit_exceeded {
            return Ok(CleanupShape::RECOVERED);
        }

        let maximum = self.request.context().maximum_recursion_depth();
        let mut pending = vec![(ty, 1_usize)];
        let mut shapes = BTreeMap::new();
        let mut dependents = BTreeMap::<TypeId, Vec<(TypeId, CleanupShapeInput)>>::new();

        while let Some((subject, depth)) = pending.pop() {
            if self.request.is_cancelled() {
                return Err(CheckerQueryError::Cancelled);
            }

            if self.completed.contains_key(&subject) || shapes.contains_key(&subject) {
                continue;
            }

            if depth > maximum {
                self.report_recursion_limit(depth);

                return Ok(CleanupShape::RECOVERED);
            }

            let (mut shape, inputs) = self.shape_inputs(subject)?;

            for input in inputs.into_iter().rev() {
                let child = input.ty();

                if let Some(completed) = self.completed.get(&child) {
                    input.merge(&mut shape, *completed);
                } else {
                    dependents.entry(child).or_default().push((subject, input));
                    pending.push((child, depth.saturating_add(1)));
                }
            }

            shapes.insert(subject, shape);
        }

        // Close the exact-type graph before publishing any summaries. Each flag only grows,
        // so recursive components converge without assuming equal behavior across substitutions.
        let mut changed = shapes.keys().copied().collect::<VecDeque<_>>();

        while let Some(child) = changed.pop_front() {
            if self.request.is_cancelled() {
                return Err(CheckerQueryError::Cancelled);
            }

            let shape = shapes[&child];

            for (parent, input) in dependents.get(&child).into_iter().flatten() {
                let target = shapes
                    .get_mut(parent)
                    .expect("cleanup graph retains every dependent type");

                let previous = *target;

                input.merge(target, shape);

                if *target != previous {
                    changed.push_back(*parent);
                }
            }
        }

        let result = shapes[&ty];

        // Prepared consumers share the published cache. New demand extends only this resolver.
        std::sync::Arc::make_mut(&mut self.completed).extend(shapes);

        Ok(result)
    }

    fn shape_inputs(
        &mut self,
        ty: TypeId,
    ) -> Result<(CleanupShape, Vec<CleanupShapeInput>), CheckerQueryError<C::UpstreamError>> {
        let data = self.request.semantic_values().type_data(ty);
        let mut inputs = Vec::new();

        let shape = match data.as_ref() {
            TypeData::Error => CleanupShape::RECOVERED,
            TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_)
            | TypeData::TypeValuedMemberProjection { .. } => CleanupShape::BOTH,
            TypeData::Named {
                definition,
                substitution,
            } => self.named_shape(ty, *definition, *substitution, &mut inputs)?,
            TypeData::Tuple(elements) => {
                inputs.extend(elements.iter().copied().map(CleanupShapeInput::Contents));

                CleanupShape::default()
            }
            TypeData::Array { element, .. } | TypeData::Nullable(element) => {
                inputs.push(CleanupShapeInput::Contents(*element));

                CleanupShape::default()
            }
            TypeData::OwnedIndirection { target, .. } | TypeData::Generator(target) => {
                inputs.push(CleanupShapeInput::Contents(*target));

                CleanupShape {
                    lifecycle: true,
                    ..CleanupShape::default()
                }
            }
            TypeData::ExactInteger { .. }
            | TypeData::FlexibleArray(_)
            | TypeData::Slice(_)
            | TypeData::Borrow { .. }
            | TypeData::Callable(_)
            | TypeData::TraitView(_) => CleanupShape::default(),
        };

        Ok((shape, inputs))
    }

    pub(in crate::asynchronous) fn report_recursion_limit(&mut self, actual: usize) {
        if self.limit_exceeded {
            return;
        }

        self.limit_exceeded = true;

        let owner = self
            .request
            .symbols()
            .symbol_for_key(self.request.unit().key().declared_owner());

        crate::diagnostic::report_cleanup_recursion_limit(
            self.request,
            owner,
            actual,
            self.request.context().maximum_recursion_depth(),
            &mut self.diagnostics,
        );
    }

    fn named_shape(
        &mut self,
        ty: TypeId,
        definition: bray_symbols::NamedTypeSymbolId,
        substitution: bray_symbols::GenericSubstitutionId,
        inputs: &mut Vec<CleanupShapeInput>,
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

                inputs.extend(substitution.bindings().iter().filter_map(|binding| {
                    match binding.argument() {
                        GenericArgument::Type(ty) => Some(CleanupShapeInput::Contents(ty)),
                        GenericArgument::Constant(_) => None,
                    }
                }));

                // RunResult also owns its implicit Panicked(PanicReport)
                // alternative, even when the completed value needs no cleanup.
                Ok(if role == Some(RepresentationRole::RunResult) {
                    CleanupShape::LIFECYCLE
                } else {
                    CleanupShape::default()
                })
            }
            Some(RepresentationRole::Uninit) => {
                let substitution = self
                    .request
                    .semantic_values()
                    .generic_substitution_data(substitution);

                inputs.extend(substitution.bindings().iter().filter_map(|binding| {
                    match binding.argument() {
                        GenericArgument::Type(ty) => Some(CleanupShapeInput::Uninitialized(ty)),
                        GenericArgument::Constant(_) => None,
                    }
                }));

                Ok(CleanupShape::default())
            }
            Some(_) => Ok(CleanupShape::default()),
            None => self.declared_shape(ty, definition, substitution, inputs),
        }
    }

    fn declared_shape(
        &mut self,
        ty: TypeId,
        definition: bray_symbols::NamedTypeSymbolId,
        substitution: bray_symbols::GenericSubstitutionId,
        inputs: &mut Vec<CleanupShapeInput>,
    ) -> Result<CleanupShape, CheckerQueryError<C::UpstreamError>> {
        if let Some(element) = self
            .request
            .context()
            .raw_buffer_element(definition, substitution)?
        {
            inputs.push(CleanupShapeInput::Contents(element));

            return Ok(CleanupShape {
                lifecycle: true,
                ..CleanupShape::default()
            });
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
            if let Some(ty) = self.member_type(member, substitution)? {
                inputs.push(CleanupShapeInput::Contents(ty));
            } else {
                shape.merge(CleanupShape::RECOVERED);
            }
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

                shape.suspending |=
                    callable.execution() == bray_symbols::CallableExecution::Asynchronous;

                if callable.execution() == bray_symbols::CallableExecution::Synchronous {
                    inputs.push(CleanupShapeInput::FinalizerResult(callable.result()));
                }
            }
        }

        Ok(shape)
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_bound_tree::BoundUnitId;
    use bray_diagnostics::DiagnosticKind;
    use bray_symbols::{TypeData, TypeId};

    use super::CleanupShapeResolver;
    use crate::test_support::{
        TestCheckerContext, callable_entry, expression_unit, semantic_values,
    };
    use crate::{CheckerQueryError, CheckerUnitView};

    fn nested_type(depth: usize) -> TypeId {
        let values = semantic_values();

        let mut ty = values
            .intern_type(TypeData::Tuple(Arc::from([])))
            .expect("empty structural type must intern");

        for _ in 0..depth {
            ty = values
                .intern_type(TypeData::Nullable(ty))
                .expect("nested structural type must intern");
        }

        ty
    }

    #[test]
    fn cleanup_shape_limits_do_not_publish_partial_summaries() {
        let (unit, _) = expression_unit(BoundUnitId::new(70), |_, _| Vec::new());

        let context = TestCheckerContext::new(false);
        let semantic_context = callable_entry(unit.key());
        let request = CheckerUnitView::new(&unit, &semantic_context, &context);
        let mut resolver = CleanupShapeResolver::new(request);

        let shape = resolver
            .resolve(nested_type(1_024))
            .expect("bounded shape analysis must finish");

        assert!(shape.recovered);
        assert!(resolver.completed.is_empty());

        bray_testing::assert_goal_state_diagnostic_kind(
            &resolver.diagnostics,
            DiagnosticKind::CheckingCleanupRecursionLimitExceeded,
        );
    }

    #[test]
    fn cleanup_shape_cancellation_does_not_publish_partial_summaries() {
        let (unit, _) = expression_unit(BoundUnitId::new(71), |_, _| Vec::new());

        let context = TestCheckerContext::cancelling_after(4);
        let semantic_context = callable_entry(unit.key());
        let request = CheckerUnitView::new(&unit, &semantic_context, &context);
        let mut resolver = CleanupShapeResolver::new(request);

        assert!(matches!(
            resolver.resolve(nested_type(32)),
            Err(CheckerQueryError::Cancelled)
        ));

        assert!(resolver.completed.is_empty());
        assert!(resolver.diagnostics.is_empty());
    }
}
