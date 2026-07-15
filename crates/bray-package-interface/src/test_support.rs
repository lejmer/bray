use std::collections::BTreeSet;

use bray_bound_tree::{CheckedTemplateInputId, CheckedTemplateKind, CheckedTemplateNodeId};
use bray_symbols::{
    ExternalSymbolKey, ImportedSymbolIdentityInput, InterfaceSupportEntityId, InterfaceSymbolId,
    ModulePathKey, PackageIdentity, SymbolKind, SymbolName, SymbolOrdinal,
};

use crate::{
    InterfaceCheckedTemplate, InterfaceCheckedTemplateBehavior, InterfaceCheckedTemplateId,
    InterfaceCheckedTemplateInput, InterfaceCheckedTemplateInputKind, InterfaceCheckedTemplateNode,
    InterfaceCheckedTemplateOperation, InterfaceDeclarationTemplate, InterfaceDependencyContract,
    InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceProductKind,
    InterfaceSemanticFacts, InterfaceSupportEntity, InterfaceSymbolReference, InterfaceType,
    InterfaceTypeId, InterfaceValidationError, PackageInterfaceIdentity, PackageInterfaceSurface,
    SymbolRelationship, SymbolRelationshipKind,
};

/// One valid encoded interface used by cross-crate compilation tests.
pub struct EncodedTemplateTestInterface {
    /// Package identity recorded by the artifact.
    pub package: PackageIdentity,
    /// Product identity recorded by the artifact.
    pub product: InterfaceProductIdentity,
    /// Interface-local owner of the declaration template.
    pub template_owner: InterfaceSymbolId,
    /// Complete encoded artifact bytes.
    pub bytes: Vec<u8>,
}

fn encode_test_interface(
    surface: &PackageInterfaceSurface,
    facts: &InterfaceSemanticFacts,
    language_revision: InterfaceLanguageRevision,
) -> Result<Vec<u8>, InterfaceValidationError> {
    crate::artifact::encode_interface_artifact(surface, facts, language_revision)
}

/// Builds one valid interface containing a declaration-owned checked template.
pub fn encoded_template_test_interface() -> EncodedTemplateTestInterface {
    let package = package("example.dependency");
    let product = product("library");

    let module = module_key(package.clone(), "templates");

    let function = named_key(module, SymbolKind::Function, "run");

    let generic_type = ExternalSymbolKey::ordinal(
        function.clone(),
        SymbolKind::GenericTypeParameter,
        SymbolOrdinal::new(0),
    )
    .unwrap_or_else(|| panic!("test generic type parameter key must be valid"));

    let surface = identity_surface(
        package.clone(),
        product.clone(),
        [function.clone(), generic_type],
    );
    let template_owner = surface
        .symbol_by_external_key(&function)
        .unwrap_or_else(|| panic!("test template owner must be present"));

    let facts = template_facts(&surface, template_owner);

    let bytes = encode_test_interface(&surface, &facts, InterfaceLanguageRevision::new(0))
        .unwrap_or_else(|error| panic!("test interface must encode: {error:?}"));

    EncodedTemplateTestInterface {
        package,
        product,
        template_owner,
        bytes,
    }
}

fn identity_surface(
    package: PackageIdentity,
    product: InterfaceProductIdentity,
    symbols: impl IntoIterator<Item = ExternalSymbolKey>,
) -> PackageInterfaceSurface {
    let mut keys = BTreeSet::new();

    for symbol in symbols {
        insert_key_and_owners(&mut keys, symbol);
    }

    keys.insert(ExternalSymbolKey::package(package.clone()));

    let keys: Vec<_> = keys.into_iter().collect();
    let records = keys.iter().enumerate().map(|(index, key)| {
        let container = key
            .owner()
            .and_then(|owner| keys.binary_search(owner).ok())
            .map(interface_symbol_id);

        ImportedSymbolIdentityInput::new(
            interface_symbol_id(index),
            key.clone(),
            key.kind(),
            container,
        )
    });

    let relationships = keys.iter().enumerate().filter_map(|(index, key)| {
        let owner = key.owner()?;

        let owner_index = keys
            .binary_search(owner)
            .unwrap_or_else(|_| panic!("test symbol owner must be present"));
        let kind = match (owner.kind(), key.kind()) {
            (SymbolKind::Package, SymbolKind::Module) => SymbolRelationshipKind::PackageModule,
            (SymbolKind::Module, SymbolKind::Function) => SymbolRelationshipKind::ModuleMember,
            (SymbolKind::Function, SymbolKind::GenericTypeParameter) => {
                SymbolRelationshipKind::GenericParameter
            }
            pair => panic!("unsupported test symbol relationship: {pair:?}"),
        };

        Some(SymbolRelationship::new(
            kind,
            interface_symbol_id(owner_index),
            interface_symbol_id(index),
            0,
        ))
    });

    let identity = PackageInterfaceIdentity::try_new(
        package,
        product,
        InterfaceProductKind::Library,
        "test-surface",
    )
    .unwrap_or_else(|| panic!("test package interface identity must be valid"));

    PackageInterfaceSurface::try_new(identity, [], records, relationships, [])
        .unwrap_or_else(|error| panic!("test interface surface must be valid: {error:?}"))
}

fn template_facts(
    surface: &PackageInterfaceSurface,
    owner: InterfaceSymbolId,
) -> InterfaceSemanticFacts {
    let generic_type = surface
        .symbols()
        .symbols()
        .iter()
        .find(|symbol| symbol.kind() == SymbolKind::GenericTypeParameter)
        .map(|symbol| InterfaceSymbolReference::Local(symbol.id()))
        .unwrap_or_else(|| panic!("test generic type parameter must be present"));

    let behavior = InterfaceCheckedTemplateBehavior::new(
        [],
        [],
        [],
        [],
        crate::InterfaceDependencyContractId::new(0),
        [],
    );

    let template = InterfaceCheckedTemplate::new(
        CheckedTemplateKind::CallableContract,
        [InterfaceCheckedTemplateInput::new(
            InterfaceCheckedTemplateInputKind::GenericType(generic_type.clone()),
            InterfaceTypeId::new(0),
        )],
        [InterfaceCheckedTemplateNode::new(
            InterfaceCheckedTemplateOperation::Input(CheckedTemplateInputId::new(0)),
            InterfaceTypeId::new(0),
        )],
        [],
        CheckedTemplateNodeId::new(0),
        behavior,
    );

    InterfaceSemanticFacts::new()
        .with_values(
            [InterfaceDependencyContract::new([])],
            [InterfaceType::TypeParameter(generic_type)],
            [],
            [],
        )
        .with_templates(
            [template],
            [InterfaceDeclarationTemplate::new(
                InterfaceSymbolReference::Local(owner),
                CheckedTemplateKind::CallableContract,
                SymbolOrdinal::new(0),
                InterfaceSupportEntityId::new(0),
            )],
            [InterfaceSupportEntity::CheckedTemplate(
                InterfaceCheckedTemplateId::new(0),
            )],
        )
}

fn insert_key_and_owners(keys: &mut BTreeSet<ExternalSymbolKey>, key: ExternalSymbolKey) {
    let mut current = Some(key);

    while let Some(key) = current {
        current = key.owner().cloned();
        keys.insert(key);
    }
}

fn interface_symbol_id(index: usize) -> InterfaceSymbolId {
    let raw = u32::try_from(index)
        .unwrap_or_else(|error| panic!("test symbol count must fit interface IDs: {error:?}"));

    InterfaceSymbolId::new(raw)
}

fn module_key(package: PackageIdentity, segment: &str) -> ExternalSymbolKey {
    let path = ModulePathKey::try_new([segment])
        .unwrap_or_else(|| panic!("test module path must be valid"));

    ExternalSymbolKey::module(ExternalSymbolKey::package(package), path)
        .unwrap_or_else(|| panic!("test module key must be valid"))
}

fn named_key(owner: ExternalSymbolKey, kind: SymbolKind, name: &str) -> ExternalSymbolKey {
    let name =
        SymbolName::try_new(name).unwrap_or_else(|| panic!("test symbol name must be valid"));

    ExternalSymbolKey::named(owner, kind, name)
        .unwrap_or_else(|| panic!("test symbol key must be valid"))
}

fn package(value: &str) -> PackageIdentity {
    PackageIdentity::try_new(value).unwrap_or_else(|| panic!("test package identity must be valid"))
}

fn product(value: &str) -> InterfaceProductIdentity {
    InterfaceProductIdentity::try_new(value)
        .unwrap_or_else(|| panic!("test product identity must be valid"))
}
