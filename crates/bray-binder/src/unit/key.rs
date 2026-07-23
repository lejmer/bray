use bray_bound_tree::{BoundUnitKey, BoundUnitKeyData};
use bray_declarations::SyntaxAnchor;
use bray_symbols::{
    LocalSymbolRegionKey, LocalSymbolRegionRole, SymbolFactKind, SymbolKey, SymbolKind,
};

pub(crate) fn local_region_key(key: &BoundUnitKey) -> LocalSymbolRegionKey {
    let (owner, role, anchors) = local_region_key_parts(key);

    match LocalSymbolRegionKey::try_new(owner, role, anchors, None) {
        Some(key) => key,
        None => unreachable!("bound unit keys always contain one source anchor"),
    }
}

fn local_region_key_parts(
    key: &BoundUnitKey,
) -> (SymbolKey, LocalSymbolRegionRole, Vec<SyntaxAnchor>) {
    match key.data() {
        BoundUnitKeyData::CallableBody(unit) => (
            shared_symbol_key(unit.owner()),
            LocalSymbolRegionRole::CallableBody,
            vec![unit.source().syntax()],
        ),
        BoundUnitKeyData::AnonymousCallable(unit) => {
            let (owner, _, mut anchors) = local_region_key_parts(unit.enclosing());

            if !matches!(
                unit.enclosing().data(),
                BoundUnitKeyData::AnonymousCallable(_)
            ) {
                anchors.clear();
            }

            anchors.push(unit.source().syntax());

            (owner, LocalSymbolRegionRole::AnonymousCallable, anchors)
        }
        BoundUnitKeyData::RuntimeDefault(unit) => (
            shared_symbol_key(unit.owner()),
            LocalSymbolRegionRole::DeclarationFact(runtime_default_fact(unit.owner().kind())),
            vec![unit.source().syntax()],
        ),
        BoundUnitKeyData::ConstantTemplate(unit) => (
            shared_symbol_key(unit.owner()),
            LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::ConstantDefinition),
            vec![unit.source().syntax()],
        ),
        BoundUnitKeyData::EmbeddedConstant(unit) => (
            shared_symbol_key(unit.owner()),
            LocalSymbolRegionRole::EmbeddedConstant,
            vec![unit.source().syntax()],
        ),
        BoundUnitKeyData::PredicateDefinition(unit) => (
            shared_symbol_key(unit.owner()),
            LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::PredicateDefinition),
            vec![unit.source().syntax()],
        ),
        BoundUnitKeyData::Constraint(unit) => (
            shared_symbol_key(unit.owner()),
            LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::GenericConstraints),
            vec![unit.source().syntax()],
        ),
        BoundUnitKeyData::ContractClause(unit) => (
            shared_symbol_key(unit.owner()),
            LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::CallableContracts),
            vec![unit.source().syntax()],
        ),
        BoundUnitKeyData::TargetGate(unit) => (
            shared_symbol_key(unit.owner()),
            LocalSymbolRegionRole::TargetGate,
            vec![unit.source().syntax()],
        ),
    }
}

fn runtime_default_fact(owner: SymbolKind) -> SymbolFactKind {
    match owner {
        SymbolKind::CallableParameterDefaultProvider => SymbolFactKind::CallableParameterDefault,
        SymbolKind::StructFieldDefaultProvider => SymbolFactKind::StructFieldDefault,
        SymbolKind::UnionPayloadDefaultProvider => SymbolFactKind::UnionPayloadFieldDefault,
        _ => unreachable!("bound runtime-default keys validate their owner kind"),
    }
}

fn shared_symbol_key(key: &SymbolKey) -> SymbolKey {
    // Symbol keys are immutable trees backed by shared identity storage.
    key.clone()
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundSourceAnchor, BoundUnitId, BoundUnitKey};
    use bray_source::TextSize;
    use bray_symbols::{LocalSymbolRegionId, LocalSymbolRegionRole};

    use crate::unit::builder::BoundUnitLocalBuilder;
    use crate::unit::test_support::{finish, fixture};

    #[test]
    fn declared_units_derive_their_exact_region_owner_role_and_anchor() {
        let fixture = fixture();

        let builder = match BoundUnitLocalBuilder::new(
            BoundUnitId::new(9),
            fixture.key.clone(),
            LocalSymbolRegionId::new(9),
            TextSize::ZERO,
        ) {
            Ok(builder) => builder,
            Err(error) => panic!("declared region must build: {error:?}"),
        };

        let snapshot = finish(builder);

        assert_eq!(
            snapshot.local_symbols().key().role(),
            LocalSymbolRegionRole::CallableBody
        );

        assert_eq!(snapshot.local_symbols().key().anchors(), &[fixture.first]);
    }

    #[test]
    fn nested_anonymous_regions_use_canonical_lambda_anchor_paths() {
        let fixture = fixture();

        let outer = BoundUnitKey::anonymous_callable(
            fixture.key.clone(),
            BoundSourceAnchor::new(fixture.first, fixture.version),
        );

        let inner = BoundUnitKey::anonymous_callable(
            outer,
            BoundSourceAnchor::new(fixture.second, fixture.version),
        );

        let builder = match BoundUnitLocalBuilder::new(
            BoundUnitId::new(10),
            inner,
            LocalSymbolRegionId::new(10),
            TextSize::ZERO,
        ) {
            Ok(builder) => builder,
            Err(error) => panic!("nested anonymous region must build: {error:?}"),
        };

        let snapshot = finish(builder);

        assert_eq!(
            snapshot.local_symbols().key().role(),
            LocalSymbolRegionRole::AnonymousCallable
        );

        assert_eq!(
            snapshot.local_symbols().key().anchors(),
            &[fixture.first, fixture.second]
        );
    }
}
