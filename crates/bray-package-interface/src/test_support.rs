use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{CheckedTemplateInputId, CheckedTemplateKind, CheckedTemplateNodeId};
use bray_symbols::{
    ExternalSymbolKey, InterfaceSupportEntityId, InterfaceSymbolId, ModulePathKey, PackageIdentity,
    SymbolKind, SymbolName, SymbolOrdinal,
};

use crate::{
    ExportLookupInput, ExportRelationshipInput, ExportSymbolInput, ExportSymbolReferenceInput,
    ExportedLookupKind, InterfaceCheckedTemplate, InterfaceCheckedTemplateBehavior,
    InterfaceCheckedTemplateId, InterfaceCheckedTemplateInput, InterfaceCheckedTemplateInputKind,
    InterfaceCheckedTemplateNode, InterfaceCheckedTemplateOperation, InterfaceCoherenceRecord,
    InterfaceConstantValue, InterfaceConstantValueId, InterfaceConstantValueKind,
    InterfaceDeclarationTemplate, InterfaceDependencyContract, InterfaceGenericSubstitution,
    InterfaceGenericSubstitutionId, InterfaceImplementationRecord, InterfaceLanguageRevision,
    InterfacePredicateSummary, InterfaceProductIdentity, InterfaceProductKind,
    InterfaceSemanticFacts, InterfaceSupportEntity, InterfaceSymbolReference,
    InterfaceTargetFactDependency, InterfaceTraitApplication, InterfaceTraitApplicationId,
    InterfaceType, InterfaceTypeId, PackageInterfaceExportBundle, PackageInterfaceIdentity,
    PackageInterfaceSurface, SymbolRelationshipKind, build_package_interface_surface,
    encode_package_interface,
};

/// One valid encoded interface used by cross-crate compilation tests.
pub struct EncodedSemanticTestInterface {
    /// Package identity recorded by the artifact.
    pub package: PackageIdentity,
    /// Product identity recorded by the artifact.
    pub product: InterfaceProductIdentity,
    /// Interface-local owner of the declaration template.
    pub template_owner: InterfaceSymbolId,
    /// Interface-local owner of the imported implementation header.
    pub implementation_owner: InterfaceSymbolId,
    /// Complete encoded artifact bytes.
    pub bytes: Vec<u8>,
}

/// Builds one valid interface containing representative exported semantic facts.
pub fn encoded_semantic_test_interface() -> EncodedSemanticTestInterface {
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

    let implementation_owner = bundle
        .surface()
        .symbols()
        .symbols()
        .iter()
        .find(|symbol| symbol.kind() == SymbolKind::NamedTraitImplementation)
        .map(|symbol| symbol.id())
        .unwrap_or_else(|| panic!("test implementation owner must be present"));

    let encoded = encode_package_interface(&bundle)
        .unwrap_or_else(|error| panic!("test interface must encode: {error:?}"));

    EncodedSemanticTestInterface {
        package,
        product,
        template_owner,
        implementation_owner,
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

    let structure = named_key(module.clone(), SymbolKind::Struct, "Record");
    let trait_definition = named_key(module.clone(), SymbolKind::Trait, "Contract");

    let implementation = named_key(
        module.clone(),
        SymbolKind::NamedTraitImplementation,
        "RecordContract",
    );

    let target_fact = named_key(module.clone(), SymbolKind::Constant, "pointer_width");

    let generic_type = ExternalSymbolKey::ordinal(
        function.clone(),
        SymbolKind::GenericTypeParameter,
        SymbolOrdinal::new(0),
    )
    .unwrap_or_else(|| panic!("test generic type parameter key must be valid"));

    let surface = identity_surface(
        package.clone(),
        product.clone(),
        [
            function.clone(),
            generic_type,
            structure,
            trait_definition,
            implementation,
            target_fact,
        ],
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

    let mut relationship_ordinals = BTreeMap::new();

    let relationships = keys.iter().filter_map(move |key| {
        let owner = key.owner()?;

        let kind = match (owner.kind(), key.kind()) {
            (SymbolKind::Package, SymbolKind::Module) => SymbolRelationshipKind::PackageModule,
            (
                SymbolKind::Module,
                SymbolKind::Function
                | SymbolKind::Struct
                | SymbolKind::Trait
                | SymbolKind::NamedTraitImplementation
                | SymbolKind::Constant,
            ) => SymbolRelationshipKind::ModuleMember,
            (SymbolKind::Function, SymbolKind::GenericTypeParameter) => {
                SymbolRelationshipKind::GenericParameter
            }
            pair => panic!("unsupported test symbol relationship: {pair:?}"),
        };

        let next_ordinal = relationship_ordinals
            .entry((owner.clone(), kind))
            .or_insert(0_u32);

        let ordinal = *next_ordinal;

        *next_ordinal = next_ordinal
            .checked_add(1)
            .unwrap_or_else(|| panic!("test relationship ordinal must fit the wire format"));

        Some(ExportRelationshipInput::new(
            kind,
            owner.clone(),
            key.clone(),
            ordinal,
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

    let generic_type = local_by_kind(surface, SymbolKind::GenericTypeParameter);
    let structure = local_by_kind(surface, SymbolKind::Struct);
    let trait_definition = local_by_kind(surface, SymbolKind::Trait);
    let implementation = local_by_kind(surface, SymbolKind::NamedTraitImplementation);
    let target_fact = local_by_kind(surface, SymbolKind::Constant);

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
        .with_applications(
            [
                InterfaceGenericSubstitution::new(structure.clone(), []),
                InterfaceGenericSubstitution::new(trait_definition.clone(), []),
            ],
            [InterfaceTraitApplication::new(
                trait_definition.clone(),
                InterfaceGenericSubstitutionId::new(1),
            )],
            [],
            [],
        )
        .with_values(
            [InterfaceDependencyContract::new([])],
            [
                InterfaceType::TypeParameter(generic_type),
                InterfaceType::Named {
                    definition: structure.clone(),
                    substitution: InterfaceGenericSubstitutionId::new(0),
                },
            ],
            [InterfaceConstantValue::new(
                InterfaceTypeId::new(1),
                InterfaceConstantValueKind::Boolean(true),
            )],
            [],
        )
        .with_contracts(
            [
                crate::InterfaceConstraint::new(owner.clone(), SymbolOrdinal::new(0), predicate),
                crate::InterfaceConstraint::new(
                    implementation.clone(),
                    SymbolOrdinal::new(0),
                    predicate,
                ),
            ],
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
        .with_implementations(
            [InterfaceImplementationRecord::new(
                implementation.clone(),
                InterfaceTypeId::new(1),
                Some(InterfaceTraitApplicationId::new(0)),
            )],
            [InterfaceCoherenceRecord::new(
                InterfaceTypeId::new(1),
                InterfaceTraitApplicationId::new(0),
                [implementation.clone()],
            )],
        )
        .with_target_dependencies(
            [
                InterfaceTargetFactDependency::new(
                    structure,
                    target_fact.clone(),
                    InterfaceConstantValueId::new(0),
                ),
                InterfaceTargetFactDependency::new(
                    trait_definition,
                    target_fact.clone(),
                    InterfaceConstantValueId::new(0),
                ),
                InterfaceTargetFactDependency::new(
                    implementation,
                    target_fact.clone(),
                    InterfaceConstantValueId::new(0),
                ),
                InterfaceTargetFactDependency::new(
                    target_fact.clone(),
                    target_fact,
                    InterfaceConstantValueId::new(0),
                ),
            ],
            [],
        )
}

pub(crate) fn local_by_kind(
    surface: &PackageInterfaceSurface,
    kind: SymbolKind,
) -> InterfaceSymbolReference {
    surface
        .symbols()
        .symbols()
        .iter()
        .find(|symbol| symbol.kind() == kind)
        .map(|symbol| InterfaceSymbolReference::Local(symbol.id()))
        .unwrap_or_else(|| panic!("test symbol kind must be present"))
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
