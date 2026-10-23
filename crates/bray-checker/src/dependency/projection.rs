use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundStructuredExpressionKind,
    BoundUnit, CheckedPatterns, CheckedSemanticSelections, ConstructionInputId,
    SelectedConstructionInput, SelectedOperation, SemanticSelection,
};
use bray_symbols::{AnyLocalSymbolId, DependencyProjection, LocalBindingSymbolId, SymbolOrdinal};

use super::ValueInputs;

impl ValueInputs {
    pub(super) fn collect_projections(
        &mut self,
        unit: &BoundUnit,
        selections: &CheckedSemanticSelections,
        patterns: &CheckedPatterns,
    ) {
        self.collect_aggregate_projections(unit, selections);

        let initializers = self.collect_binding_initializers(unit, patterns);

        self.collect_initialized_fields(unit, selections, &initializers);
    }

    fn collect_aggregate_projections(
        &mut self,
        unit: &BoundUnit,
        selections: &CheckedSemanticSelections,
    ) {
        for (id, expression) in unit.tree().expressions() {
            if let Some(SemanticSelection::Operation(SelectedOperation::Construction(
                construction,
            ))) = selections.expression(id)
            {
                for input in construction.inputs() {
                    if let SelectedConstructionInput::Explicit {
                        expression,
                        input,
                        conversion,
                        ..
                    } = input
                    {
                        let projection = match input {
                            ConstructionInputId::StructField(field) => {
                                DependencyProjection::ProductField(*field)
                            }
                            ConstructionInputId::UnionPayloadField(field) => {
                                DependencyProjection::UnionPayloadField(*field)
                            }
                            ConstructionInputId::CallableParameter(_) => continue,
                        };

                        let suffix = match conversion.target() {
                            bray_bound_tree::ConversionTarget::NullablePresent => {
                                Some(DependencyProjection::NullableValue)
                            }
                            bray_bound_tree::ConversionTarget::Identity
                            | bray_bound_tree::ConversionTarget::BuiltInScalar => None,
                            target => panic!(
                                "implicit construction input has unexpected conversion {target:?}"
                            ),
                        };

                        self.projections
                            .entry(id)
                            .or_default()
                            .insert(projection, (*expression, suffix));
                    }
                }
            }

            if let BoundExpression::Structured(value) = expression
                && value.kind() == BoundStructuredExpressionKind::Tuple
            {
                for (ordinal, child) in value.operands().iter().enumerate() {
                    if let Ok(ordinal) = u32::try_from(ordinal) {
                        self.projections.entry(id).or_default().insert(
                            DependencyProjection::TupleElement(SymbolOrdinal::new(ordinal)),
                            (*child, None),
                        );
                    }
                }
            }
        }
    }

    fn collect_binding_initializers(
        &mut self,
        unit: &BoundUnit,
        patterns: &CheckedPatterns,
    ) -> BTreeMap<LocalBindingSymbolId, BoundExpressionId> {
        let mut initializers = BTreeMap::new();
        let subjects = &self.pattern_subjects;

        loop {
            let mut changed = false;

            for (pattern, initializer) in subjects {
                // Implicit invocation values have no source initializer to substitute.
                let bray_bound_tree::BoundExecutionSite::Node(
                    bray_bound_tree::AnyBoundNodeId::Expression(initializer),
                ) = initializer
                else {
                    continue;
                };

                let mut pending = vec![(*pattern, *initializer)];

                while let Some((id, initializer)) = pending.pop() {
                    let Some(pattern) = unit.tree().pattern(id) else {
                        continue;
                    };

                    let Some(checked) = patterns.pattern(id) else {
                        continue;
                    };

                    let Some(initializer) =
                        self.pattern_initializer(unit, initializer, checked.projection())
                    else {
                        continue;
                    };

                    for local in checked.bindings(pattern) {
                        let Some(ty) = patterns.binding_type(local) else {
                            continue;
                        };

                        let Some(value) = self.pattern_initializer(
                            unit,
                            initializer,
                            binding_projection(pattern, ty),
                        ) else {
                            continue;
                        };

                        initializers.insert(local, value);
                    }

                    pending.extend(pattern.children().iter().map(|child| (*child, initializer)));
                }
            }

            for (id, expression) in unit.tree().expressions() {
                let local = match expression {
                    BoundExpression::Name(name) => match name.target() {
                        BoundReferenceTarget::Local(AnyLocalSymbolId::Binding(binding)) => {
                            Some(binding)
                        }
                        _ => None,
                    },
                    BoundExpression::PatternReference(reference) => Some(reference.binding()),
                    _ => None,
                };

                if let Some(initializer) = local.and_then(|binding| initializers.get(&binding)) {
                    changed |= self.initializers.insert(id, *initializer) != Some(*initializer);
                }
            }

            if !changed {
                break;
            }
        }

        initializers
    }

    fn pattern_initializer(
        &self,
        unit: &BoundUnit,
        mut initializer: BoundExpressionId,
        projection: Option<bray_bound_tree::PatternProjection>,
    ) -> Option<BoundExpressionId> {
        let Some(projection) = projection else {
            return Some(initializer);
        };

        if let bray_bound_tree::PatternProjection::ElementFromStart(index)
        | bray_bound_tree::PatternProjection::ElementFromEnd(index) = projection
        {
            let mut visited = BTreeSet::new();

            while let Some(source) = self.initializers.get(&initializer) {
                if !visited.insert(initializer) {
                    return None;
                }

                initializer = *source;
            }

            let BoundExpression::Structured(value) = unit.tree().expression(initializer)? else {
                return None;
            };

            if value.kind() == BoundStructuredExpressionKind::RepeatedArray {
                return value.operands().first().copied();
            }

            if value.kind() != BoundStructuredExpressionKind::Array {
                return None;
            }

            let index = index.to_index()?;

            let index = if matches!(
                projection,
                bray_bound_tree::PatternProjection::ElementFromEnd(_)
            ) {
                value.operands().len().checked_sub(index.checked_add(1)?)?
            } else {
                index
            };

            return value.operands().get(index).copied();
        }

        let path = [pattern_projection(projection)?];

        let (value, remaining) = self.project(initializer, &path);

        remaining.is_empty().then_some(value)
    }

    fn collect_initialized_fields(
        &mut self,
        unit: &BoundUnit,
        selections: &CheckedSemanticSelections,
        initializers: &BTreeMap<LocalBindingSymbolId, BoundExpressionId>,
    ) {
        for (id, _) in unit.tree().expressions() {
            let places = super::assignment::value_places(unit, selections, self, id);
            let mut places = places.into_iter();

            let Some((BoundReferenceTarget::Local(AnyLocalSymbolId::Binding(binding)), path)) =
                places.next()
            else {
                continue;
            };

            if places.next().is_some() || path.is_empty() {
                continue;
            }

            let Some(path) = path.into_iter().collect::<Option<Vec<_>>>() else {
                continue;
            };

            let Some(initializer) = initializers.get(&binding) else {
                continue;
            };

            let (value, remaining) = self.project(*initializer, &path);

            if remaining.is_empty() {
                self.projected_initializers.insert(id, value);
            }
        }
    }

    /// Resolves initialized aggregate fields without treating carried sources as aggregate storage.
    pub(crate) fn project<'a>(
        &self,
        mut expression: BoundExpressionId,
        mut path: &'a [DependencyProjection],
    ) -> (BoundExpressionId, &'a [DependencyProjection]) {
        let mut visited = BTreeSet::new();

        while let Some((projection, rest)) = path.split_first() {
            if !visited.insert((expression, path.len())) {
                break;
            }

            if let Some((child, suffix)) = self
                .projections
                .get(&expression)
                .and_then(|fields| fields.get(projection))
            {
                let Some(remaining) = rest.strip_prefix(suffix.as_slice()) else {
                    break;
                };

                expression = *child;
                path = remaining;
            } else if let Some(initializer) = self.initializers.get(&expression) {
                if self
                    .writes
                    .values(expression)
                    .any(|written| written.matches(path))
                {
                    break;
                }

                expression = *initializer;
            } else {
                break;
            }
        }

        (expression, path)
    }
}

pub(super) fn pattern_projection(
    projection: bray_bound_tree::PatternProjection,
) -> Option<DependencyProjection> {
    use bray_bound_tree::PatternProjection;

    match projection {
        PatternProjection::ProductField(field) => Some(DependencyProjection::ProductField(field)),
        PatternProjection::TupleElement(index) => Some(DependencyProjection::TupleElement(index)),
        PatternProjection::ActiveUnionPayloadField { field, .. } => {
            Some(DependencyProjection::UnionPayloadField(field))
        }
        PatternProjection::NullableValue => Some(DependencyProjection::NullableValue),
        PatternProjection::OwnedTarget => Some(DependencyProjection::OwnedTarget),
        PatternProjection::ElementFromStart(_) | PatternProjection::ElementFromEnd(_) => None,
    }
}

/// Direct bindings already use the pattern projection; shorthand entries add their own.
pub(super) fn binding_projection(
    pattern: &bray_bound_tree::BoundPattern,
    binding: bray_bound_tree::PatternBindingTypeEntry,
) -> Option<bray_bound_tree::PatternProjection> {
    if pattern.bindings().contains(&binding.binding()) {
        None
    } else {
        binding.projection()
    }
}

/// Resolve source and imported declaration relationships once, before fixed-point flow.
pub(super) fn prepare_variants<C: crate::CheckerRequestContext + ?Sized>(
    request: crate::CheckerUnitView<'_, C>,
    contracts: &bray_bound_tree::CheckedDependencyContracts,
    storage: &bray_bound_tree::StoragePlan,
    values: &ValueInputs,
    selections: &CheckedSemanticSelections,
) -> crate::CheckerQueryResult<
    BTreeMap<bray_symbols::UnionPayloadFieldSymbolId, bray_symbols::UnionVariantSymbolId>,
    C::UpstreamError,
> {
    let mut fields = BTreeSet::new();

    let mut retain = |projections: &[DependencyProjection]| {
        fields.extend(
            projections
                .iter()
                .filter_map(|projection| match projection {
                    DependencyProjection::UnionPayloadField(field) => Some(*field),
                    _ => None,
                }),
        );
    };

    for call in contracts.calls().values() {
        for subject in call.reference_subjects() {
            retain(subject.projections());
        }

        for reference in call.references() {
            let relation = reference.relation();

            if let Some(entry) = relation.entry() {
                retain(entry.projections());
            }

            retain(relation.exit().projections());
            retain(relation.target_projections());
        }

        for carrier in call.carriers() {
            retain(carrier.path());
        }

        for effect in call
            .effects()
            .iter()
            .chain(call.deferred_effects().into_iter().flatten())
        {
            if let bray_symbols::DependencyStorageEffect::Indirect { projections, .. } = effect {
                retain(projections);
            }
        }
    }

    for contract in contracts.lifecycle_dependencies().values() {
        for subject in contract.reference_subjects() {
            retain(subject.projections());
        }

        for relation in contract.borrows() {
            retain(relation.target_projections());
        }

        for effect in contract.effects() {
            if let bray_symbols::DependencyStorageEffect::Indirect { projections, .. } = effect {
                retain(projections);
            }
        }
    }

    for carrier in contracts
        .static_carriers()
        .values()
        .flat_map(|carriers| carriers.iter())
    {
        retain(carrier.path());
    }

    for projections in values.projections.values() {
        for projection in projections.keys() {
            retain(std::slice::from_ref(projection));
        }
    }

    for inputs in values.projected.values().chain(values.borrowed.values()) {
        for (_, projections) in inputs {
            retain(projections);
        }
    }

    for (_, projection) in values.error_exits.values() {
        retain(std::slice::from_ref(projection));
    }

    let mut variants = BTreeMap::new();

    for entry in selections.entries() {
        if let SemanticSelection::Operation(SelectedOperation::Construction(construction)) =
            entry.selection()
            && let bray_bound_tree::ConstructionTarget::UnionVariant(variant) =
                construction.target()
        {
            for input in construction.inputs() {
                let (SelectedConstructionInput::Explicit { input, .. }
                | SelectedConstructionInput::Default { input, .. }) = input;

                if let ConstructionInputId::UnionPayloadField(field) = input {
                    variants.insert(*field, variant);
                }
            }
        }

        if let SemanticSelection::Propagation(selection) = entry.selection() {
            if let Some((variant, field)) = selection.success() {
                variants.insert(field, variant);
            }

            if let bray_bound_tree::SelectedPropagation::Result {
                error: (variant, field),
                ..
            } = selection
            {
                variants.insert(*field, *variant);
            }
        }
    }

    for (access, _) in storage.access_entries() {
        for projection in storage.resolved_projections(access).into_iter().flatten() {
            if let bray_bound_tree::StorageProjection::ActiveUnionPayloadField { variant, field } =
                projection
            {
                variants.insert(*field, *variant);
            }
        }
    }

    for field in fields {
        if let std::collections::btree_map::Entry::Vacant(entry) = variants.entry(field) {
            entry.insert(
                request
                    .union_payload_field(field)?
                    .expect("checked dependency field retains its declaration")
                    .variant(),
            );
        }
    }

    Ok(variants)
}
