use bray_bound_tree::{
    PatternPredicate, Refinement, RefinementKind, StorageAccessId, StoragePlan, StorageProjection,
    StorageRelationship,
};

pub(super) fn storage_is_recovered(storage: &StoragePlan) -> bool {
    storage.is_recovered()
}

pub(super) fn projection_is_available(
    storage: &StoragePlan,
    access: StorageAccessId,
    projection: StorageProjection,
    depth: usize,
    refinements: &[Refinement],
) -> bool {
    if pattern_binding_depth(storage, access).is_some_and(|bound_depth| depth < bound_depth) {
        return true;
    }

    let related = refinements.iter().filter(|refinement| {
        if let Some((subject, _, _)) = refinement.kind().structural_predicate() {
            return storage.access_contains(subject, access)
                && storage
                    .resolved_projections(subject)
                    .is_some_and(|path| path.len() == depth);
        }

        refinement.dependencies().iter().any(|dependency| {
            storage.relationship(*dependency, access) != StorageRelationship::Disjoint
        })
    });

    match projection {
        StorageProjection::NullableValue => related.into_iter().any(|refinement| {
            matches!(
                refinement.kind(),
                RefinementKind::NullablePresence {
                    is_present: true,
                    ..
                } | RefinementKind::Pattern {
                    predicate: PatternPredicate::NullablePresent,
                    value: true,
                    ..
                } | RefinementKind::Pattern {
                    predicate: PatternPredicate::NullableAbsent,
                    value: false,
                    ..
                }
            )
        }),
        StorageProjection::ActiveUnionPayloadField { variant, .. } => {
            related.into_iter().any(|refinement| {
                matches!(
                    refinement.kind().structural_predicate(),
                    Some((_, PatternPredicate::ActiveUnionVariant(active), true)) if active == variant
                )
            })
        }
        StorageProjection::ProductField(_)
        | StorageProjection::TupleElement(_)
        | StorageProjection::ElementFromStart(_)
        | StorageProjection::ElementFromEnd(_)
        | StorageProjection::Element(_)
        | StorageProjection::SliceRange { .. }
        | StorageProjection::OwnedTarget => true,
    }
}

pub(super) fn pattern_binding_depth(
    storage: &StoragePlan,
    access: StorageAccessId,
) -> Option<usize> {
    let source_is_pattern = |access: StorageAccessId| {
        storage.access(access).is_some_and(|access| {
            matches!(
                access.source().syntax().syntax_kind(),
                bray_syntax::SyntaxKind::IrrefutablePattern
                    | bray_syntax::SyntaxKind::IrrefutablePatternEntry
                    | bray_syntax::SyntaxKind::CasePattern
                    | bray_syntax::SyntaxKind::CasePatternEntry
            )
        })
    };

    let direct = source_is_pattern(access).then_some(access);

    direct
        .into_iter()
        .chain(storage.bindings().iter().filter_map(|(_, binding)| {
            let bray_bound_tree::StorageBinding::Access(binding) = binding else {
                return None;
            };

            (source_is_pattern(*binding) && storage.access_contains(*binding, access))
                .then_some(*binding)
        }))
        .filter_map(|binding| storage.resolved_projections(binding).map(|path| path.len()))
        .max()
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundSourceAnchor, BoundUnitId, BoundUnitKind, StorageAccess, StorageAccessRoot,
        StorageBinding, StorageBindingTarget, StorageIdentity, StoragePlanBuilder,
        StorageProjection,
    };
    use bray_declarations::SyntaxAnchor;
    use bray_source::{SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion};
    use bray_symbols::{CallableParameterSymbolId, SymbolId, SymbolOrdinal};
    use bray_syntax::{SyntaxKind, SyntaxWalkControl, SyntaxWalkEvent};

    use super::projection_is_available;

    #[test]
    fn pattern_binding_proves_only_its_own_projection_prefix() {
        let snapshot = SourceSnapshot::new(
            SourceId::new(0),
            SourceIdentity::new(0),
            SourceOrigin::virtual_source("pattern-prefix"),
            SourceVersion::new(1),
            r#"
            module app;

            func main(input: i32?)
            {
                match input
                {
                    case ?value {}
                    case none {}
                }
            }
            "#,
        )
        .unwrap();

        let parsed = bray_parser::parse_source_unit(&snapshot);
        let mut pattern = None;

        bray_syntax::walk_syntax_node(parsed.source_unit(), |event| {
            if let SyntaxWalkEvent::EnterNode(node) = event
                && node.kind() == SyntaxKind::CasePattern
            {
                pattern = Some(BoundSourceAnchor::new(
                    SyntaxAnchor::from_node(&node),
                    snapshot.version(),
                ));

                return SyntaxWalkControl::Stop;
            }

            SyntaxWalkControl::Continue
        });

        let pattern = pattern.unwrap();

        let expression = BoundSourceAnchor::new(
            SyntaxAnchor::from_node(parsed.source_unit()),
            snapshot.version(),
        );

        let mut builder =
            StoragePlanBuilder::new(BoundUnitId::new(85), BoundUnitKind::CallableBody);

        let binding = CallableParameterSymbolId::from_symbol_id(SymbolId::new(7));

        let root = builder
            .push_identity(StorageIdentity::Parameter(binding))
            .unwrap();

        let ty = crate::test_support::error_type();

        let bound = builder
            .push_access(StorageAccess::new(
                StorageAccessRoot::Storage(root),
                [StorageProjection::NullableValue],
                ty,
                pattern,
                false,
            ))
            .unwrap();

        builder
            .bind(
                StorageBindingTarget::Parameter(binding),
                StorageBinding::Access(bound),
            )
            .unwrap();

        let nested = builder
            .push_access(StorageAccess::new(
                StorageAccessRoot::Storage(root),
                [
                    StorageProjection::NullableValue,
                    StorageProjection::NullableValue,
                ],
                ty,
                expression,
                false,
            ))
            .unwrap();

        let sibling = builder
            .push_access(StorageAccess::new(
                StorageAccessRoot::Storage(root),
                [
                    StorageProjection::TupleElement(SymbolOrdinal::new(0)),
                    StorageProjection::NullableValue,
                ],
                ty,
                expression,
                false,
            ))
            .unwrap();

        let storage = builder.finish();

        assert!(projection_is_available(
            &storage,
            nested,
            StorageProjection::NullableValue,
            0,
            &[]
        ));

        assert!(!projection_is_available(
            &storage,
            nested,
            StorageProjection::NullableValue,
            1,
            &[]
        ));

        assert!(!projection_is_available(
            &storage,
            sibling,
            StorageProjection::NullableValue,
            1,
            &[]
        ));
    }
}
