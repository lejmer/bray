use std::collections::BTreeSet;

use bray_bound_tree::{CheckedTemplateInputId, CheckedTemplateKind, CheckedTemplateNodeId};
use bray_symbols::{
    ExternalSymbolKey, InterfaceSupportEntityId, InterfaceSymbolId, ModulePathKey, PackageIdentity,
    SymbolKind, SymbolName, SymbolOrdinal,
};

use crate::{
    ExportLookupInput, ExportRelationshipInput, ExportSymbolInput, ExportSymbolReferenceInput,
    ExportedLookupKind, InterfaceCheckedTemplate, InterfaceCheckedTemplateBehavior,
    InterfaceCheckedTemplateId, InterfaceCheckedTemplateInput, InterfaceCheckedTemplateInputKind,
    InterfaceCheckedTemplateNode, InterfaceCheckedTemplateOperation, InterfaceDeclarationTemplate,
    InterfaceDependencyContract, InterfaceLanguageRevision, InterfacePredicateSummary,
    InterfaceProductIdentity, InterfaceProductKind, InterfaceSemanticFacts, InterfaceSupportEntity,
    InterfaceSymbolReference, InterfaceType, InterfaceTypeId, PackageInterfaceExportBundle,
    PackageInterfaceIdentity, PackageInterfaceSurface, SymbolRelationshipKind,
    build_package_interface_surface, encode_package_interface,
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

/// Builds one valid interface containing a declaration-owned checked template.
pub fn encoded_template_test_interface() -> EncodedTemplateTestInterface {
    let bundle = package_interface_export_bundle();
    let package = bundle.surface().identity().package().clone();
    let product = bundle.surface().identity().product().clone();

    let template_owner = bundle
        .surface()
        .symbols()
        .symbols()
        .iter()
        .find(|symbol| symbol.kind() == SymbolKind::Function)
        .map(|symbol| symbol.id())
        .unwrap_or_else(|| panic!("test template owner must be present"));

    let encoded = encode_package_interface(&bundle)
        .unwrap_or_else(|error| panic!("test interface must encode: {error:?}"));

    EncodedTemplateTestInterface {
        package,
        product,
        template_owner,
        bytes: encoded.bytes().to_vec(),
    }
}

/// Builds one complete immutable package-interface artifact for emitter tests.
pub fn interface_artifact() -> crate::InterfaceArtifact {
    encode_package_interface(&package_interface_export_bundle())
        .unwrap_or_else(|error| panic!("test interface must encode: {error:?}"))
}

/// Builds one complete test artifact for the supplied package-local product identity.
pub fn interface_artifact_for(
    package: PackageIdentity,
    product_name: &str,
) -> crate::InterfaceArtifact {
    let product = product(product_name);

    encode_package_interface(&package_interface_export_bundle_for(package, product))
        .unwrap_or_else(|error| panic!("test interface must encode: {error:?}"))
}

pub(crate) fn package_interface_export_bundle() -> PackageInterfaceExportBundle {
    let package = package("example.dependency");
    let product = product("library");

    package_interface_export_bundle_for(package, product)
}

fn package_interface_export_bundle_for(
    package: PackageIdentity,
    product: InterfaceProductIdentity,
) -> PackageInterfaceExportBundle {
    let module = module_key(package.clone(), "templates");

    let function = named_key(module.clone(), SymbolKind::Function, "run");

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
        [
            ExportLookupInput::new(
                ExternalSymbolKey::package(package.clone()),
                symbol_name("templates"),
                ExportedLookupKind::Direct,
                ExportSymbolReferenceInput::Local(module.clone()),
            ),
            ExportLookupInput::new(
                module,
                symbol_name("run"),
                ExportedLookupKind::Direct,
                ExportSymbolReferenceInput::Local(function.clone()),
            ),
        ],
    );

    let template_owner = surface
        .symbol_by_external_key(&function)
        .unwrap_or_else(|| panic!("test template owner must be present"));

    let facts = template_facts(&surface, template_owner);

    PackageInterfaceExportBundle::try_new(surface, facts, InterfaceLanguageRevision::new(0))
        .unwrap_or_else(|error| panic!("test export bundle must be valid: {error:?}"))
}

fn identity_surface(
    package: PackageIdentity,
    product: InterfaceProductIdentity,
    symbols: impl IntoIterator<Item = ExternalSymbolKey>,
    exports: impl IntoIterator<Item = ExportLookupInput>,
) -> PackageInterfaceSurface {
    let mut keys = BTreeSet::new();

    for symbol in symbols {
        insert_key_and_owners(&mut keys, symbol);
    }

    keys.insert(ExternalSymbolKey::package(package.clone()));

    let records = keys
        .iter()
        .map(|key| ExportSymbolInput::new(key.clone(), key.owner().cloned()));

    let relationships = keys.iter().filter_map(|key| {
        let owner = key.owner()?;

        let kind = match (owner.kind(), key.kind()) {
            (SymbolKind::Package, SymbolKind::Module) => SymbolRelationshipKind::PackageModule,
            (SymbolKind::Module, SymbolKind::Function) => SymbolRelationshipKind::ModuleMember,
            (SymbolKind::Function, SymbolKind::GenericTypeParameter) => {
                SymbolRelationshipKind::GenericParameter
            }
            pair => panic!("unsupported test symbol relationship: {pair:?}"),
        };

        Some(ExportRelationshipInput::new(
            kind,
            owner.clone(),
            key.clone(),
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

    build_package_interface_surface(identity, [], records, relationships, exports)
        .unwrap_or_else(|error| panic!("test interface surface must be valid: {error:?}"))
}

fn template_facts(
    surface: &PackageInterfaceSurface,
    owner: InterfaceSymbolId,
) -> InterfaceSemanticFacts {
    let owner = InterfaceSymbolReference::Local(owner);

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
        crate::InterfaceCheckedTemplateExecution::new(
            [],
            bray_symbols::CurrentRunCancellation::NotEntered,
        ),
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

    let predicate = InterfacePredicateSummary::new(crate::InterfaceDependencyContractId::new(0));

    let invocation_behavior = crate::InterfaceCallablePhaseBehavior::new(
        [],
        [],
        [],
        [],
        [],
        crate::InterfaceDependencyContractId::new(0),
        bray_symbols::CurrentRunCancellation::NotEntered,
    );

    InterfaceSemanticFacts::new()
        .with_values(
            [InterfaceDependencyContract::new([])],
            [InterfaceType::TypeParameter(generic_type)],
            [],
            [],
        )
        .with_contracts(
            [crate::InterfaceConstraint::new(
                owner.clone(),
                SymbolOrdinal::new(0),
                predicate,
            )],
            [crate::InterfaceCallableContract::new(
                owner.clone(),
                [crate::InterfaceCallableContractClause::new(
                    SymbolOrdinal::new(0),
                    bray_symbols::CallableContractClauseKind::Requires,
                    predicate,
                )],
                invocation_behavior,
                None,
            )],
        )
        .with_templates(
            [template],
            [InterfaceDeclarationTemplate::new(
                owner,
                CheckedTemplateKind::CallableContract,
                SymbolOrdinal::new(0),
                InterfaceSupportEntityId::new(0),
            )],
            [InterfaceSupportEntity::CheckedTemplate(
                InterfaceCheckedTemplateId::new(0),
            )],
        )
}

pub(crate) fn insert_key_and_owners(
    keys: &mut BTreeSet<ExternalSymbolKey>,
    key: ExternalSymbolKey,
) {
    let mut current = Some(key);

    while let Some(key) = current {
        current = key.owner().cloned();
        keys.insert(key);
    }
}

pub(crate) fn module_key(package: PackageIdentity, segment: &str) -> ExternalSymbolKey {
    let path = ModulePathKey::try_new([segment])
        .unwrap_or_else(|| panic!("test module path must be valid"));

    ExternalSymbolKey::module(ExternalSymbolKey::package(package), path)
        .unwrap_or_else(|| panic!("test module key must be valid"))
}

pub(crate) fn named_key(
    owner: ExternalSymbolKey,
    kind: SymbolKind,
    name: &str,
) -> ExternalSymbolKey {
    let name = symbol_name(name);

    ExternalSymbolKey::named(owner, kind, name)
        .unwrap_or_else(|| panic!("test symbol key must be valid"))
}

fn symbol_name(value: &str) -> SymbolName {
    SymbolName::try_new(value).unwrap_or_else(|| panic!("test symbol name must be valid"))
}

fn package(value: &str) -> PackageIdentity {
    PackageIdentity::try_new(value).unwrap_or_else(|| panic!("test package identity must be valid"))
}

fn product(value: &str) -> InterfaceProductIdentity {
    InterfaceProductIdentity::try_new(value)
        .unwrap_or_else(|| panic!("test product identity must be valid"))
}
