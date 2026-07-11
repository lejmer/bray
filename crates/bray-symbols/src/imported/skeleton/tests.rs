use crate::{
    AnySymbolId, FunctionSymbol, FunctionSymbolId, ImportedInterfaceId, ImportedSymbolRelationship,
    ImportedSymbolSkeleton, ImportedSymbolSkeletonBuildError, ImportedSymbolSkeletonInput,
    InterfaceSymbolId, MemberLookupResult, SymbolId, SymbolOrigin, SymbolProvider,
    SymbolRelationshipKind,
};

use super::test_support::interface_fixture;

#[test]
fn external_key_remapping_is_deterministic_under_interface_permutations() {
    let first = interface_fixture(7, "z.package", "zeta");
    let second = interface_fixture(4, "a.package", "alpha");

    let forward = build([first.input.clone(), second.input.clone()]);
    let reverse = build([second.input, first.input]);

    assert_eq!(forward, reverse);
    assert_eq!(
        forward.symbol_by_external_key(&first.package_key),
        reverse.symbol_by_external_key(&first.package_key)
    );
    assert_eq!(
        forward.symbol_by_external_key(&second.function_key),
        reverse.symbol_by_external_key(&second.function_key)
    );
}

#[test]
fn containment_lookup_and_origin_neutral_provider_access_are_exact() {
    let fixture = interface_fixture(3, "example.package", "run");
    let skeleton = build([fixture.input]);
    let Some(AnySymbolId::Package(package_id)) =
        skeleton.symbol_by_external_key(&fixture.package_key)
    else {
        panic!("package key must remap to a package symbol");
    };

    let Some(AnySymbolId::Module(module_id)) = skeleton.symbol_by_external_key(&fixture.module_key)
    else {
        panic!("module key must remap to a module symbol");
    };

    let Some(AnySymbolId::Function(function_id)) =
        skeleton.symbol_by_external_key(&fixture.function_key)
    else {
        panic!("function key must remap to a function symbol");
    };

    let Some(package) = skeleton.package(package_id) else {
        panic!("remapped package ID must resolve");
    };

    let Some(module) = skeleton.module(module_id) else {
        panic!("remapped module ID must resolve");
    };

    let Some(function) = provided_function(&skeleton, function_id) else {
        panic!("ordinary function provider contract must resolve imported records");
    };

    assert_eq!(package.modules(), [module_id]);
    assert_eq!(module.owner(), package_id.into());
    assert_eq!(module.functions(), [function_id]);
    assert_eq!(function.containing_symbol(), module_id.into());
    assert_eq!(function.origin(), SymbolOrigin::Imported);
    assert_eq!(function.source_declaration(), None);
    assert_eq!(
        function.imported_fact_key().map(|key| key.interface()),
        Some(ImportedInterfaceId::new(3))
    );
    assert_eq!(
        skeleton.lookup(module_id.into(), "run"),
        MemberLookupResult::Found(function_id.into())
    );
    assert_eq!(
        skeleton.lookup(package_id.into(), "run"),
        MemberLookupResult::NotFound
    );
}

#[test]
fn malformed_external_relationships_are_rejected_with_typed_errors() {
    let fixture = interface_fixture(2, "example.package", "run");
    let malformed = ImportedSymbolSkeletonInput::new(
        ImportedInterfaceId::new(2),
        fixture.input.symbols().clone(),
        [ImportedSymbolRelationship::new(
            SymbolRelationshipKind::ModuleMember,
            InterfaceSymbolId::new(1),
            InterfaceSymbolId::new(99),
            0,
        )],
        [],
    );

    assert_eq!(
        ImportedSymbolSkeleton::try_new(SymbolId::new(0), [malformed]),
        Err(
            ImportedSymbolSkeletonBuildError::RelationshipSymbolOutOfBounds {
                interface: ImportedInterfaceId::new(2),
                symbol: InterfaceSymbolId::new(99),
            }
        )
    );

    let invalid_kinds = ImportedSymbolSkeletonInput::new(
        ImportedInterfaceId::new(2),
        fixture.input.symbols().clone(),
        [ImportedSymbolRelationship::new(
            SymbolRelationshipKind::PackageModule,
            InterfaceSymbolId::new(0),
            InterfaceSymbolId::new(2),
            0,
        )],
        [],
    );
    let error = ImportedSymbolSkeleton::try_new(SymbolId::new(0), [invalid_kinds]);

    assert!(matches!(
        error,
        Err(ImportedSymbolSkeletonBuildError::InvalidRelationshipKinds {
            relationship: SymbolRelationshipKind::PackageModule,
            ..
        })
    ));
}

#[test]
fn imported_skeletons_and_records_are_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<ImportedSymbolSkeleton>();
    assert_send_sync::<FunctionSymbol>();
}

fn build(inputs: impl IntoIterator<Item = ImportedSymbolSkeletonInput>) -> ImportedSymbolSkeleton {
    match ImportedSymbolSkeleton::try_new(SymbolId::new(10), inputs) {
        Ok(skeleton) => skeleton,
        Err(error) => panic!("test imported skeleton must build: {error:?}"),
    }
}

fn provided_function(
    provider: &impl SymbolProvider<FunctionSymbolId>,
    id: FunctionSymbolId,
) -> Option<&FunctionSymbol> {
    provider.symbol(id)
}
