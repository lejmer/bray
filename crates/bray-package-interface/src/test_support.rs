use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{CheckedTemplateInputId, CheckedTemplateKind, CheckedTemplateNodeId};
use bray_runtime_interface::{PanicAbiIdentity, RuntimeAbiVersion};
use bray_symbols::{
    ExternalSymbolKey, InterfaceSupportEntityId, InterfaceSymbolId, ModulePathKey, PackageIdentity,
    PackageVersion, SymbolKind, SymbolName, SymbolOrdinal, SynthesizedSymbolRole,
};

use crate::{
    ExportLookupInput, ExportRelationshipInput, ExportSymbolInput, ExportSymbolReferenceInput,
    ExportedLookupKind, InterfaceCallableParameter, InterfaceCallableParameterDefault,
    InterfaceCallablePhaseBehavior, InterfaceCallableSignature, InterfaceCheckedTemplate,
    InterfaceCheckedTemplateBehavior, InterfaceCheckedTemplateId, InterfaceCheckedTemplateInput,
    InterfaceCheckedTemplateInputKind, InterfaceCheckedTemplateNode,
    InterfaceCheckedTemplateOperation, InterfaceCoherenceRecord, InterfaceConstantTerm,
    InterfaceConstantTermId, InterfaceConstantValue, InterfaceConstantValueId,
    InterfaceConstantValueKind, InterfaceDeclarationTemplate, InterfaceDependencyContract,
    InterfaceGenericSubstitution, InterfaceGenericSubstitutionId, InterfaceImplementationRecord,
    InterfaceLanguageRevision, InterfacePredicateDefinition, InterfacePredicateDefinitionState,
    InterfacePredicateSummary, InterfaceProductIdentity, InterfaceProductKind, InterfaceSemantics,
    InterfaceStorageMember, InterfaceStorageShape, InterfaceSupportEntity,
    InterfaceSymbolReference, InterfaceTargetPropertyDependency, InterfaceTraitApplication,
    InterfaceTraitApplicationId, InterfaceType, InterfaceTypeId, InterfaceTypeRepresentation,
    PackageInterfaceExportBundle, PackageInterfaceIdentity, PackageInterfaceSurface,
    SymbolRelationshipKind, build_package_interface_surface, encode_package_interface,
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
    /// Interface-local owner of an opaque trusted predicate.
    pub opaque_predicate_owner: InterfaceSymbolId,
    /// Interface-local owner of a defined trait predicate fulfillment.
    pub defined_predicate_owner: InterfaceSymbolId,
    /// Complete encoded artifact bytes.
    pub bytes: Vec<u8>,
}

/// Builds one valid interface containing representative exported semantics.
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

    let opaque_predicate_owner = bundle
        .surface()
        .symbols()
        .symbols()
        .iter()
        .find(|symbol| symbol.kind() == SymbolKind::Predicate)
        .map(|symbol| symbol.id())
        .unwrap_or_else(|| panic!("test opaque predicate owner must be present"));

    let defined_predicate_owner = bundle
        .surface()
        .symbols()
        .symbols()
        .iter()
        .find(|symbol| symbol.kind() == SymbolKind::TraitPredicateFulfillment)
        .map(|symbol| symbol.id())
        .unwrap_or_else(|| panic!("test defined predicate owner must be present"));

    let encoded = encode_package_interface(&bundle)
        .unwrap_or_else(|error| panic!("test interface must encode: {error:?}"));

    EncodedSemanticTestInterface {
        package,
        product,
        template_owner,
        implementation_owner,
        opaque_predicate_owner,
        defined_predicate_owner,
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

/// Builds the representative semantic export bundle used by cross-crate tests.
pub fn package_interface_export_bundle() -> PackageInterfaceExportBundle {
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

    let direct_callable = named_key(structure.clone(), SymbolKind::TypeCallableMember, "direct");

    let trait_definition = named_key(module.clone(), SymbolKind::Trait, "Contract");

    let implementation = named_key(
        module.clone(),
        SymbolKind::NamedTraitImplementation,
        "RecordContract",
    );

    let inherent_implementation = ExternalSymbolKey::ordinal(
        module.clone(),
        SymbolKind::InherentImplementation,
        SymbolOrdinal::new(0),
    )
    .unwrap_or_else(|| panic!("test inherent implementation key must be valid"));

    let inherent_callable = named_key(
        inherent_implementation.clone(),
        SymbolKind::TypeCallableMember,
        "extension",
    );

    let target_record = named_key(module.clone(), SymbolKind::Constant, "pointer_width");

    let opaque_predicate = named_key(module.clone(), SymbolKind::Predicate, "trusted_boundary");

    let required_predicate = named_key(
        trait_definition.clone(),
        SymbolKind::TraitPredicateMember,
        "valid",
    );

    let defined_predicate = named_key(
        implementation.clone(),
        SymbolKind::TraitPredicateFulfillment,
        "valid",
    );

    let generic_type = ExternalSymbolKey::ordinal(
        function.clone(),
        SymbolKind::GenericTypeParameter,
        SymbolOrdinal::new(0),
    )
    .unwrap_or_else(|| panic!("test generic type parameter key must be valid"));

    let generic_constant = ExternalSymbolKey::ordinal(
        function.clone(),
        SymbolKind::GenericConstParameter,
        SymbolOrdinal::new(1),
    )
    .unwrap_or_else(|| panic!("test generic constant parameter key must be valid"));

    let parameter = ExternalSymbolKey::ordinal(
        function.clone(),
        SymbolKind::CallableParameter,
        SymbolOrdinal::new(0),
    )
    .unwrap_or_else(|| panic!("test callable parameter key must be valid"));

    let default_provider = ExternalSymbolKey::synthesized(
        parameter.clone(),
        SynthesizedSymbolRole::CallableParameterDefaultProvider,
        None,
    )
    .unwrap_or_else(|| panic!("test callable default provider key must be valid"));

    let surface = identity_surface(
        package.clone(),
        product.clone(),
        [
            function.clone(),
            generic_type,
            generic_constant,
            parameter,
            default_provider,
            structure.clone(),
            direct_callable.clone(),
            trait_definition,
            implementation.clone(),
            inherent_implementation.clone(),
            inherent_callable.clone(),
            target_record,
            opaque_predicate,
            required_predicate,
            defined_predicate,
        ],
        [
            ExportLookupInput::new(
                ExternalSymbolKey::package(package.clone()),
                symbol_name("templates"),
                ExportedLookupKind::Direct,
                ExportSymbolReferenceInput::Local(module.clone()),
            ),
            ExportLookupInput::new(
                module.clone(),
                symbol_name("Record"),
                ExportedLookupKind::Direct,
                ExportSymbolReferenceInput::Local(structure.clone()),
            ),
            ExportLookupInput::new(
                module.clone(),
                symbol_name("run"),
                ExportedLookupKind::Direct,
                ExportSymbolReferenceInput::Local(function.clone()),
            ),
            ExportLookupInput::new(
                module,
                symbol_name("RecordContract"),
                ExportedLookupKind::Direct,
                ExportSymbolReferenceInput::Local(implementation.clone()),
            ),
        ],
    );

    let template_owner = surface
        .symbol_by_external_key(&function)
        .unwrap_or_else(|| panic!("test template owner must be present"));

    let direct_callable = local_by_key(&surface, &direct_callable);
    let inherent_implementation = local_by_key(&surface, &inherent_implementation);
    let inherent_callable = local_by_key(&surface, &inherent_callable);

    let semantics = template_semantics(
        &surface,
        template_owner,
        direct_callable,
        inherent_implementation,
        inherent_callable,
    );

    PackageInterfaceExportBundle::try_new(
        surface,
        semantics,
        InterfaceLanguageRevision::new(0),
        implementation_configuration(),
    )
    .unwrap_or_else(|error| panic!("test export bundle must be valid: {error:?}"))
}

/// Builds the representative implementation target configuration used by artifact tests.
pub fn implementation_configuration() -> crate::PackageImplementationConfiguration {
    let target = bray_target::NativeTarget::X86_64LinuxGnu.profile();
    let target = bray_ir::MirTargetContract::new(target, RuntimeAbiVersion::new(1, 0));

    let panic_abi = PanicAbiIdentity::try_new("bray.panic.unwind")
        .unwrap_or_else(|| panic!("test panic ABI identity must be valid"));

    crate::PackageImplementationConfiguration::for_mir_target(&target, None, panic_abi)
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
                | SymbolKind::InherentImplementation
                | SymbolKind::NamedTraitImplementation
                | SymbolKind::Constant
                | SymbolKind::Predicate,
            ) => SymbolRelationshipKind::ModuleMember,
            (SymbolKind::Struct | SymbolKind::Union, member)
                if SymbolRelationshipKind::TypeMember.supports(owner.kind(), member) =>
            {
                SymbolRelationshipKind::TypeMember
            }
            (SymbolKind::Trait, SymbolKind::TraitPredicateMember) => {
                SymbolRelationshipKind::TraitMember
            }
            (SymbolKind::InherentImplementation, member)
                if SymbolRelationshipKind::ImplementationMember.supports(owner.kind(), member) =>
            {
                SymbolRelationshipKind::ImplementationMember
            }
            (SymbolKind::NamedTraitImplementation, SymbolKind::TraitPredicateFulfillment) => {
                SymbolRelationshipKind::ImplementationFulfillment
            }
            (
                SymbolKind::Function,
                SymbolKind::GenericTypeParameter | SymbolKind::GenericConstParameter,
            ) => SymbolRelationshipKind::GenericParameter,
            (SymbolKind::Function, SymbolKind::CallableParameter) => {
                SymbolRelationshipKind::CallableParameter
            }
            (SymbolKind::CallableParameter, SymbolKind::CallableParameterDefaultProvider) => {
                SymbolRelationshipKind::DefaultProvider
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
        package_version(),
        product,
        InterfaceProductKind::Library,
        "test-surface",
    )
    .unwrap_or_else(|| panic!("test package interface identity must be valid"));

    build_package_interface_surface(identity, [], records, relationships, exports)
        .unwrap_or_else(|error| panic!("test interface surface must be valid: {error:?}"))
}

pub fn package_version() -> PackageVersion {
    PackageVersion::try_new("1.0.0").unwrap_or_else(|| panic!("test package version must be valid"))
}

fn template_semantics(
    surface: &PackageInterfaceSurface,
    owner: InterfaceSymbolId,
    direct_callable: InterfaceSymbolReference,
    inherent_implementation: InterfaceSymbolReference,
    inherent_callable: InterfaceSymbolReference,
) -> InterfaceSemantics {
    let owner = InterfaceSymbolReference::Local(owner);

    let generic_type = local_by_kind(surface, SymbolKind::GenericTypeParameter);
    let generic_constant = local_by_kind(surface, SymbolKind::GenericConstParameter);
    let parameter = local_by_kind(surface, SymbolKind::CallableParameter);
    let structure = local_by_kind(surface, SymbolKind::Struct);
    let trait_definition = local_by_kind(surface, SymbolKind::Trait);
    let implementation = local_by_kind(surface, SymbolKind::NamedTraitImplementation);
    let target_record = local_by_kind(surface, SymbolKind::Constant);
    let opaque_predicate = local_by_kind(surface, SymbolKind::Predicate);
    let required_predicate = local_by_kind(surface, SymbolKind::TraitPredicateMember);
    let defined_predicate = local_by_kind(surface, SymbolKind::TraitPredicateFulfillment);

    let template = checked_template(CheckedTemplateKind::CallableContract, generic_type.clone());

    let predicate_template = checked_template(
        CheckedTemplateKind::PredicateDefinition,
        generic_type.clone(),
    );

    let constraint_template =
        checked_template(CheckedTemplateKind::GenericConstraint, generic_type.clone());

    let predicate = InterfacePredicateSummary::new(crate::InterfaceDependencyContractId::new(0));

    let invocation_behavior = callable_phase_behavior();

    let mut declaration_templates = vec![
        InterfaceDeclarationTemplate::new(
            owner.clone(),
            CheckedTemplateKind::CallableContract,
            SymbolOrdinal::new(0),
            InterfaceSupportEntityId::new(0),
        ),
        InterfaceDeclarationTemplate::new(
            defined_predicate.clone(),
            CheckedTemplateKind::PredicateDefinition,
            SymbolOrdinal::new(0),
            InterfaceSupportEntityId::new(1),
        ),
        InterfaceDeclarationTemplate::new(
            implementation.clone(),
            CheckedTemplateKind::GenericConstraint,
            SymbolOrdinal::new(0),
            InterfaceSupportEntityId::new(2),
        ),
    ];

    declaration_templates.sort_by(|left, right| {
        (left.owner(), left.kind(), left.ordinal()).cmp(&(
            right.owner(),
            right.kind(),
            right.ordinal(),
        ))
    });

    InterfaceSemantics::new()
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
                InterfaceType::TypeParameter(generic_type.clone()),
                InterfaceType::Named {
                    definition: structure.clone(),
                    substitution: InterfaceGenericSubstitutionId::new(0),
                },
                InterfaceType::Array {
                    element: InterfaceTypeId::new(0),
                    length: InterfaceConstantTermId::new(0),
                },
                InterfaceType::Callable {
                    parameters: [InterfaceCallableParameter::new(
                        "value",
                        bray_symbols::CallablePosition::PositionalOrNamed,
                        bray_symbols::CallableParameterMode::Immutable,
                        InterfaceTypeId::new(2),
                    )]
                    .into(),
                    result: InterfaceTypeId::new(0),
                    constness: bray_symbols::CallableConstness::Runtime,
                    trust: bray_symbols::CallableTrust::Safe,
                    abi: bray_symbols::CallableAbi::Bray,
                    invocation_behavior: invocation_behavior.clone(),
                    deferred_execution_behavior: None,
                },
                InterfaceType::Callable {
                    parameters: [].into(),
                    result: InterfaceTypeId::new(1),
                    constness: bray_symbols::CallableConstness::Runtime,
                    trust: bray_symbols::CallableTrust::Safe,
                    abi: bray_symbols::CallableAbi::Bray,
                    invocation_behavior: invocation_behavior.clone(),
                    deferred_execution_behavior: None,
                },
            ],
            [InterfaceConstantValue::new(
                InterfaceTypeId::new(1),
                InterfaceConstantValueKind::Boolean(true),
            )],
            [InterfaceConstantTerm::Parameter(generic_constant.clone())],
        )
        .with_contracts(
            [
                crate::InterfaceConstraint::trait_satisfaction(
                    owner.clone(),
                    SymbolOrdinal::new(0),
                    InterfaceTypeId::new(0),
                    crate::InterfaceTraitApplicationId::new(0),
                ),
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
        .with_declarations(
            [
                InterfaceCallableSignature::new(
                    owner.clone(),
                    InterfaceTypeId::new(3),
                    None,
                    [parameter.clone()],
                    InterfaceTypeId::new(0),
                )
                .with_body(true),
                InterfaceCallableSignature::new(
                    direct_callable,
                    InterfaceTypeId::new(4),
                    None,
                    [],
                    InterfaceTypeId::new(1),
                )
                .with_body(true),
                InterfaceCallableSignature::new(
                    inherent_callable,
                    InterfaceTypeId::new(4),
                    None,
                    [],
                    InterfaceTypeId::new(1),
                )
                .with_body(true),
            ],
            [crate::InterfaceGenericDeclaration::new(
                owner.clone(),
                [generic_type, generic_constant],
            )],
            [InterfaceCallableParameterDefault::new(parameter, true)],
            [
                InterfacePredicateDefinition::new(
                    opaque_predicate,
                    InterfacePredicateDefinitionState::OpaqueTrusted,
                ),
                InterfacePredicateDefinition::new(
                    required_predicate,
                    InterfacePredicateDefinitionState::Required,
                ),
                InterfacePredicateDefinition::new(
                    defined_predicate.clone(),
                    InterfacePredicateDefinitionState::Defined,
                ),
            ],
        )
        .with_type_representations(type_representations(structure.clone()))
        .with_templates(
            [template, predicate_template, constraint_template],
            declaration_templates,
            [
                InterfaceSupportEntity::CheckedTemplate(InterfaceCheckedTemplateId::new(0)),
                InterfaceSupportEntity::CheckedTemplate(InterfaceCheckedTemplateId::new(1)),
                InterfaceSupportEntity::CheckedTemplate(InterfaceCheckedTemplateId::new(2)),
            ],
        )
        .with_implementations(
            [
                InterfaceImplementationRecord::new(
                    implementation.clone(),
                    InterfaceTypeId::new(1),
                    Some(InterfaceTraitApplicationId::new(0)),
                ),
                InterfaceImplementationRecord::new(
                    inherent_implementation,
                    InterfaceTypeId::new(1),
                    None,
                ),
            ],
            [InterfaceCoherenceRecord::new(
                InterfaceTypeId::new(1),
                InterfaceTraitApplicationId::new(0),
                [implementation.clone()],
            )],
        )
        .with_target_dependencies(
            [
                InterfaceTargetPropertyDependency::new(
                    structure,
                    target_record.clone(),
                    InterfaceConstantValueId::new(0),
                ),
                InterfaceTargetPropertyDependency::new(
                    trait_definition,
                    target_record.clone(),
                    InterfaceConstantValueId::new(0),
                ),
                InterfaceTargetPropertyDependency::new(
                    implementation,
                    target_record.clone(),
                    InterfaceConstantValueId::new(0),
                ),
                InterfaceTargetPropertyDependency::new(
                    target_record.clone(),
                    target_record,
                    InterfaceConstantValueId::new(0),
                ),
            ],
            [],
        )
}

fn type_representations(structure: InterfaceSymbolReference) -> [InterfaceTypeRepresentation; 1] {
    [InterfaceTypeRepresentation::new(structure)
        .with_storage(InterfaceStorageShape::Structure(
            [InterfaceStorageMember::new(None, InterfaceTypeId::new(0))].into(),
        ))
        .with_properties(true, true)]
}

pub(crate) fn callable_phase_behavior() -> InterfaceCallablePhaseBehavior {
    InterfaceCallablePhaseBehavior::new(
        [],
        [],
        [],
        [],
        [],
        crate::InterfaceDependencyContractId::new(0),
        bray_symbols::CurrentRunCancellation::NotEntered,
    )
}

fn checked_template(
    kind: CheckedTemplateKind,
    generic_type: InterfaceSymbolReference,
) -> InterfaceCheckedTemplate {
    InterfaceCheckedTemplate::new(
        kind,
        [InterfaceCheckedTemplateInput::new(
            InterfaceCheckedTemplateInputKind::GenericType(generic_type),
            InterfaceTypeId::new(0),
        )],
        [InterfaceCheckedTemplateNode::new(
            InterfaceCheckedTemplateOperation::Input(CheckedTemplateInputId::new(0)),
            InterfaceTypeId::new(0),
        )],
        [],
        CheckedTemplateNodeId::new(0),
        InterfaceCheckedTemplateBehavior::new(
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
        ),
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

fn local_by_key(
    surface: &PackageInterfaceSurface,
    key: &ExternalSymbolKey,
) -> InterfaceSymbolReference {
    surface
        .symbol_by_external_key(key)
        .map(InterfaceSymbolReference::Local)
        .unwrap_or_else(|| panic!("test symbol key must be present: {key:?}"))
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
