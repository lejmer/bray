use super::descriptor::{
    CompilerKnownDeclarationDescriptor, CompilerKnownDeclarationOwner,
    CompilerKnownScopeDescriptor, CompilerKnownValueDescriptor,
    RecognizedStandardLibraryDeclarationDescriptor, RecognizedStandardLibraryDeclarationOwner,
    RecognizedStandardLibraryScopeDescriptor,
};
use super::diagnostic::{
    CatalogDiagnostic, CatalogDiagnosticKind, CatalogDiagnostics, CatalogKeyDomain,
};
use super::parser::parse_catalog_source;
use super::validation::{
    ValidatedCatalog, ValidatedDeclaration, ValidatedDeclarationOwner, ValidatedScope,
    ValidatedValue, validate_catalog,
};
use super::{
    CatalogScopeLocation, CatalogSourceAnchor, CatalogSourceInventory, CompilerKnownCatalog,
    CompilerKnownCatalogRoleRegistry, CompilerKnownDeclarationId, CompilerKnownDeclarationKey,
    CompilerKnownScopeId, CompilerKnownScopeKey, CompilerKnownValueId, CompilerKnownValueKey,
    RecognizedStandardLibraryDeclarationId, RecognizedStandardLibraryDeclarationKey,
    RecognizedStandardLibraryScopeId, RecognizedStandardLibraryScopeKey,
};

pub use super::validation::CatalogFragmentValidator;

/// Result of parsing, validating, and building one immutable catalog graph.
pub type CatalogBuildResult = Result<CompilerKnownCatalog, CatalogDiagnostics>;

/// Parses and validates a complete source inventory, then builds canonical descriptors.
///
/// The caller supplies fragment validation so catalog grammar never duplicates
/// Bray declaration or type-expression grammar.
pub fn build_catalog(
    inventory: &'static CatalogSourceInventory,
    validator: &mut impl CatalogFragmentValidator,
) -> CatalogBuildResult {
    let mut parsed_sources = Vec::new();
    let mut diagnostics = Vec::new();

    for source in inventory.sources() {
        let (parsed, source_diagnostics) = parse_catalog_source(*source);

        diagnostics.extend(source_diagnostics);

        if let Some(parsed) = parsed {
            parsed_sources.push(parsed);
        }
    }

    if !diagnostics.is_empty() {
        return Err(CatalogDiagnostics::from_unsorted(diagnostics));
    }

    let Some(validated) = validate_catalog(&parsed_sources, inventory, validator, &mut diagnostics)
    else {
        return Err(CatalogDiagnostics::from_unsorted(diagnostics));
    };

    build_descriptors(inventory, validated)
}

fn build_descriptors(
    inventory: &'static CatalogSourceInventory,
    validated: ValidatedCatalog,
) -> CatalogBuildResult {
    let mut diagnostics = Vec::new();

    validate_descriptor_count(
        validated.compiler_known_scopes.len(),
        CatalogKeyDomain::CompilerKnownScope,
        first_anchor(inventory, &validated.compiler_known_scopes),
        &mut diagnostics,
    );

    validate_descriptor_count(
        validated.compiler_known_declarations.len(),
        CatalogKeyDomain::CompilerKnownDeclaration,
        first_anchor(inventory, &validated.compiler_known_declarations),
        &mut diagnostics,
    );

    validate_descriptor_count(
        validated.compiler_known_values.len(),
        CatalogKeyDomain::CompilerKnownValue,
        first_anchor(inventory, &validated.compiler_known_values),
        &mut diagnostics,
    );

    validate_descriptor_count(
        validated.recognized_scopes.len(),
        CatalogKeyDomain::RecognizedStandardLibraryScope,
        first_anchor(inventory, &validated.recognized_scopes),
        &mut diagnostics,
    );

    validate_descriptor_count(
        validated.recognized_declarations.len(),
        CatalogKeyDomain::RecognizedStandardLibraryDeclaration,
        first_anchor(inventory, &validated.recognized_declarations),
        &mut diagnostics,
    );

    if !diagnostics.is_empty() {
        return Err(CatalogDiagnostics::from_unsorted(diagnostics));
    }

    let compiler_known_declarations =
        build_compiler_known_declarations(&validated.compiler_known_declarations);

    let compiler_known_values = build_compiler_known_values(&validated.compiler_known_values);

    let compiler_known_scopes = build_compiler_known_scopes(
        &validated.compiler_known_scopes,
        &compiler_known_declarations,
        &compiler_known_values,
    );

    let recognized_declarations = build_recognized_declarations(&validated.recognized_declarations);

    let recognized_scopes =
        build_recognized_scopes(&validated.recognized_scopes, &recognized_declarations);

    let role_registry = CompilerKnownCatalogRoleRegistry::from_descriptors(
        &compiler_known_declarations,
        &compiler_known_values,
        validated
            .compiler_known_declarations
            .iter()
            .enumerate()
            .filter_map(|(index, declaration)| {
                Some((
                    declaration.iteration_role?,
                    CompilerKnownDeclarationId::try_from_index(index)?,
                ))
            }),
        validated
            .compiler_known_declarations
            .iter()
            .enumerate()
            .filter_map(|(index, declaration)| {
                Some((
                    declaration.operation_role?,
                    CompilerKnownDeclarationId::try_from_index(index)?,
                    declaration.kind,
                ))
            }),
    );

    record_construction_mismatch(
        compiler_known_scopes.len(),
        validated.compiler_known_scopes.len(),
        CatalogKeyDomain::CompilerKnownScope,
        inventory,
        &mut diagnostics,
    );

    record_construction_mismatch(
        compiler_known_declarations.len(),
        validated.compiler_known_declarations.len(),
        CatalogKeyDomain::CompilerKnownDeclaration,
        inventory,
        &mut diagnostics,
    );

    record_construction_mismatch(
        compiler_known_values.len(),
        validated.compiler_known_values.len(),
        CatalogKeyDomain::CompilerKnownValue,
        inventory,
        &mut diagnostics,
    );

    record_construction_mismatch(
        recognized_scopes.len(),
        validated.recognized_scopes.len(),
        CatalogKeyDomain::RecognizedStandardLibraryScope,
        inventory,
        &mut diagnostics,
    );

    record_construction_mismatch(
        recognized_declarations.len(),
        validated.recognized_declarations.len(),
        CatalogKeyDomain::RecognizedStandardLibraryDeclaration,
        inventory,
        &mut diagnostics,
    );

    if !diagnostics.is_empty() {
        return Err(CatalogDiagnostics::from_unsorted(diagnostics));
    }

    Ok(CompilerKnownCatalog {
        compiler_known_scopes: compiler_known_scopes.into(),
        compiler_known_declarations: compiler_known_declarations.into(),
        compiler_known_values: compiler_known_values.into(),
        role_registry,
        recognized_scopes: recognized_scopes.into(),
        recognized_declarations: recognized_declarations.into(),
        declaration_surfaces: std::borrow::Cow::Borrowed(&[]),
        type_surfaces: std::borrow::Cow::Borrowed(&[]),
    })
}

fn build_compiler_known_declarations(
    declarations: &[ValidatedDeclaration],
) -> Vec<CompilerKnownDeclarationDescriptor> {
    declarations
        .iter()
        .enumerate()
        .filter_map(|(index, declaration)| {
            Some(CompilerKnownDeclarationDescriptor {
                id: CompilerKnownDeclarationId::try_from_index(index)?,
                key: CompilerKnownDeclarationKey::try_new(&declaration.key)?,
                owner: match declaration.owner {
                    ValidatedDeclarationOwner::Scope(index) => {
                        CompilerKnownDeclarationOwner::Scope(CompilerKnownScopeId::try_from_index(
                            index,
                        )?)
                    }
                    ValidatedDeclarationOwner::Declaration(index) => {
                        CompilerKnownDeclarationOwner::Declaration(
                            CompilerKnownDeclarationId::try_from_index(index)?,
                        )
                    }
                },
                kind: declaration.kind,
                surface: declaration.surface,
                representation_role: declaration.representation_role,
                implementation_hook: declaration.implementation_hook,
                availability_rule: declaration.availability_rule,
            })
        })
        .collect()
}

fn build_compiler_known_values(values: &[ValidatedValue]) -> Vec<CompilerKnownValueDescriptor> {
    values
        .iter()
        .enumerate()
        .filter_map(|(index, value)| {
            Some(CompilerKnownValueDescriptor {
                id: CompilerKnownValueId::try_from_index(index)?,
                key: CompilerKnownValueKey::try_new(&value.key)?,
                owner_scope: CompilerKnownScopeId::try_from_index(value.owner_scope)?,
                spelling: value.spelling.to_owned_storage(),
                type_surface: value.type_surface,
                representation_role: value.representation_role,
                availability_rule: value.availability_rule,
            })
        })
        .collect()
}

fn build_compiler_known_scopes(
    scopes: &[ValidatedScope],
    declarations: &[CompilerKnownDeclarationDescriptor],
    values: &[CompilerKnownValueDescriptor],
) -> Vec<CompilerKnownScopeDescriptor> {
    scopes
        .iter()
        .enumerate()
        .filter_map(|(index, scope)| {
            Some(CompilerKnownScopeDescriptor {
                id: CompilerKnownScopeId::try_from_index(index)?,
                key: CompilerKnownScopeKey::try_new(&scope.key)?,
                // Descriptor assembly borrows validated scopes, so it copies this short path.
                location: scope.location.clone(),
                declaration_ids: direct_compiler_declarations(index, declarations).into(),
                value_ids: values
                    .iter()
                    .filter(|value| value.owner_scope().to_index() == Some(index))
                    .map(CompilerKnownValueDescriptor::id)
                    .collect::<Vec<_>>()
                    .into(),
            })
        })
        .collect()
}

fn build_recognized_declarations(
    declarations: &[ValidatedDeclaration],
) -> Vec<RecognizedStandardLibraryDeclarationDescriptor> {
    declarations
        .iter()
        .enumerate()
        .filter_map(|(index, declaration)| {
            Some(RecognizedStandardLibraryDeclarationDescriptor {
                id: RecognizedStandardLibraryDeclarationId::try_from_index(index)?,
                key: RecognizedStandardLibraryDeclarationKey::try_new(&declaration.key)?,
                owner: match declaration.owner {
                    ValidatedDeclarationOwner::Scope(index) => {
                        RecognizedStandardLibraryDeclarationOwner::Scope(
                            RecognizedStandardLibraryScopeId::try_from_index(index)?,
                        )
                    }
                    ValidatedDeclarationOwner::Declaration(index) => {
                        RecognizedStandardLibraryDeclarationOwner::Declaration(
                            RecognizedStandardLibraryDeclarationId::try_from_index(index)?,
                        )
                    }
                },
                // Generated descriptors own their immutable identity representation.
                identity: declaration.recognized_identity.clone()?,
                kind: declaration.kind,
                surface: declaration.surface,
                implementation_hook: declaration.implementation_hook,
                availability_rule: declaration.availability_rule,
            })
        })
        .collect()
}

fn build_recognized_scopes(
    scopes: &[ValidatedScope],
    declarations: &[RecognizedStandardLibraryDeclarationDescriptor],
) -> Vec<RecognizedStandardLibraryScopeDescriptor> {
    scopes
        .iter()
        .enumerate()
        .filter_map(|(index, scope)| {
            let CatalogScopeLocation::Module(path) = &scope.location else {
                return None;
            };

            Some(RecognizedStandardLibraryScopeDescriptor {
                id: RecognizedStandardLibraryScopeId::try_from_index(index)?,
                key: RecognizedStandardLibraryScopeKey::try_new(&scope.key)?,
                // Descriptor assembly borrows validated scopes, so it copies this short path.
                path: path.clone(),
                declaration_ids: declarations
                    .iter()
                    .filter_map(|declaration| match declaration.owner() {
                        RecognizedStandardLibraryDeclarationOwner::Scope(owner)
                            if owner.to_index() == Some(index) =>
                        {
                            Some(declaration.id())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .into(),
            })
        })
        .collect()
}

fn direct_compiler_declarations(
    scope_index: usize,
    declarations: &[CompilerKnownDeclarationDescriptor],
) -> Vec<CompilerKnownDeclarationId> {
    declarations
        .iter()
        .filter_map(|declaration| match declaration.owner() {
            CompilerKnownDeclarationOwner::Scope(owner)
                if owner.to_index() == Some(scope_index) =>
            {
                Some(declaration.id())
            }
            _ => None,
        })
        .collect()
}

fn validate_descriptor_count(
    count: usize,
    domain: CatalogKeyDomain,
    anchor: CatalogSourceAnchor,
    diagnostics: &mut Vec<CatalogDiagnostic>,
) {
    if u32::try_from(count).is_err() {
        diagnostics.push(CatalogDiagnostic::new(
            anchor,
            CatalogDiagnosticKind::DescriptorLimitExceeded { domain, count },
        ));
    }
}

fn record_construction_mismatch(
    actual: usize,
    expected: usize,
    domain: CatalogKeyDomain,
    inventory: &'static CatalogSourceInventory,
    diagnostics: &mut Vec<CatalogDiagnostic>,
) {
    if actual != expected {
        diagnostics.push(CatalogDiagnostic::new(
            first_inventory_anchor(inventory),
            CatalogDiagnosticKind::DescriptorConstructionInvariant { domain },
        ));
    }
}

trait HasAnchor {
    fn anchor(&self) -> CatalogSourceAnchor;
}

impl HasAnchor for ValidatedScope {
    fn anchor(&self) -> CatalogSourceAnchor {
        self.anchor
    }
}

impl HasAnchor for ValidatedDeclaration {
    fn anchor(&self) -> CatalogSourceAnchor {
        self.anchor
    }
}

impl HasAnchor for ValidatedValue {
    fn anchor(&self) -> CatalogSourceAnchor {
        self.anchor
    }
}

fn first_anchor<T: HasAnchor>(
    inventory: &'static CatalogSourceInventory,
    items: &[T],
) -> CatalogSourceAnchor {
    items
        .first()
        .map_or_else(|| first_inventory_anchor(inventory), HasAnchor::anchor)
}

fn first_inventory_anchor(inventory: &'static CatalogSourceInventory) -> CatalogSourceAnchor {
    let source = inventory
        .sources()
        .first()
        .map_or(super::CatalogSourceId::new(0), |source| source.id());

    CatalogSourceAnchor {
        source,
        range: bray_source::TextRange::EMPTY,
    }
}

#[cfg(test)]
mod tests {
    use bray_parser::{
        DeclarationFragmentContext, parse_declaration_fragment, parse_type_expression_fragment,
    };
    use bray_source::{SourceIdentity, SourceOrigin, SourceStore, SourceVersion};

    use super::{CatalogFragmentValidator, build_catalog};
    use crate::catalog::generation::declaration_kind;
    use crate::catalog::{
        CatalogDeclarationKind, CatalogDeclarationSurface, CatalogDiagnostic,
        CatalogDiagnosticKind, CatalogDiagnostics, CatalogField, CatalogKind, CatalogMetadataKind,
        CatalogScopeLocation, CatalogSource, CatalogSourceAnchor, CatalogSourceId,
        CatalogSourceInventory, CatalogSurfaceContext, CatalogTypeSurface,
        CompilerKnownDeclarationOwner, RecognizedStandardLibraryDeclarationIdentity,
        RecognizedStandardLibraryDeclarationOwner, generator_input_inventory,
    };
    use crate::{AvailabilityRule, ImplementationHook, RepresentationRole};

    const VALID_PRIMARY_TEXT: &str = concat!(
        "catalog compiler_known revision 1;\n",
        "scope Memory at core.memory {\n",
        "  declaration Zed {\n",
        "    representation RawPointer;\n",
        "    surface { struct Zed {} }\n",
        "  }\n",
        "  declaration Read {\n",
        "    owner Zed;\n",
        "    availability RawMemory;\n",
        "    implementation RawPointerRead;\n",
        "    surface { func read(); }\n",
        "  }\n",
        "  value True {\n",
        "    spelling true;\n",
        "    type { bool }\n",
        "    representation BooleanTrue;\n",
        "  }\n",
        "}\n",
    );

    const VALID_CONTRIBUTION_TEXT: &str = concat!(
        "catalog compiler_known revision 1;\n",
        "scope Memory at core.memory {\n",
        "  declaration Alpha {\n",
        "    representation ScalarBool;\n",
        "    surface { struct Alpha {} }\n",
        "  }\n",
        "}\n",
    );

    const VALID_RECOGNIZED_TEXT: &str = concat!(
        "catalog recognized_standard_library revision 1;\n",
        "scope StandardText at std.text {\n",
        "  declaration Length {\n",
        "    identity name length;\n",
        "    implementation MemorySizeOf;\n",
        "    surface { func length(); }\n",
        "  }\n",
        "}\n",
    );

    const VALID_SOURCES: [CatalogSource; 3] = [
        CatalogSource::new(
            CatalogSourceId::new(0),
            CatalogKind::CompilerKnown,
            "catalog/test/primary.braydef",
            VALID_PRIMARY_TEXT,
        ),
        CatalogSource::new(
            CatalogSourceId::new(1),
            CatalogKind::CompilerKnown,
            "catalog/test/contribution.braydef",
            VALID_CONTRIBUTION_TEXT,
        ),
        CatalogSource::new(
            CatalogSourceId::new(2),
            CatalogKind::RecognizedStandardLibrary,
            "catalog/test/recognized.braydef",
            VALID_RECOGNIZED_TEXT,
        ),
    ];

    static VALID_INVENTORY: CatalogSourceInventory = CatalogSourceInventory {
        sources: &VALID_SOURCES,
    };

    #[test]
    fn representative_generator_inputs_build_with_real_bray_fragments() {
        let mut validator = BrayFragmentValidator;

        let catalog = match build_catalog(generator_input_inventory(), &mut validator) {
            Ok(catalog) => catalog,
            Err(diagnostics) => panic!("representative catalog should build: {diagnostics:#?}"),
        };

        assert_eq!(catalog.compiler_known_scopes().len(), 14);
        assert_eq!(catalog.compiler_known_declarations().len(), 356);
        assert_eq!(catalog.compiler_known_values().len(), 4);
        assert_eq!(catalog.recognized_standard_library_scopes().len(), 12);

        assert_eq!(
            catalog.recognized_standard_library_declarations().len(),
            144
        );

        let raw_pointer = declaration(&catalog, "RawPointer");
        let element = declaration(&catalog, "RawPointerElement");
        let storage = declaration(&catalog, "Storage");
        let storage_create = declaration(&catalog, "StorageCreate");
        let callable = declaration(&catalog, "UnaryCallable");
        let implementation = declaration(&catalog, "HeapStorageImplementation");
        let implementation_item = declaration(&catalog, "HeapStorageCreate");
        let memory_copy = declaration(&catalog, "MemoryCopy");
        let future = declaration(&catalog, "Future");
        let future_start = declaration(&catalog, "FutureStart");
        let task = declaration(&catalog, "Task");
        let task_join = declaration(&catalog, "TaskJoin");
        let task_cancel = declaration(&catalog, "TaskCancel");

        assert_eq!(raw_pointer.kind(), CatalogDeclarationKind::Struct);

        assert_eq!(
            element.owner(),
            CompilerKnownDeclarationOwner::Declaration(raw_pointer.id())
        );

        assert_eq!(storage.kind(), CatalogDeclarationKind::Trait);
        assert_eq!(callable.kind(), CatalogDeclarationKind::CallableContract);

        assert_eq!(
            storage_create.owner(),
            CompilerKnownDeclarationOwner::Declaration(storage.id())
        );

        assert_eq!(
            implementation.kind(),
            CatalogDeclarationKind::NamedTraitImplementation
        );

        assert_eq!(
            implementation_item.owner(),
            CompilerKnownDeclarationOwner::Declaration(implementation.id())
        );

        assert_eq!(
            memory_copy.implementation_hook(),
            Some(ImplementationHook::MemoryCopy)
        );

        assert_eq!(memory_copy.availability_rule(), AvailabilityRule::RawMemory);

        let target_scalars = [
            (
                "TargetReal16",
                AvailabilityRule::Real16,
                RepresentationRole::ScalarR16,
            ),
            (
                "TargetReal128",
                AvailabilityRule::Real128,
                RepresentationRole::ScalarR128,
            ),
            (
                "TargetComplex32",
                AvailabilityRule::Complex32,
                RepresentationRole::ScalarC32,
            ),
            (
                "TargetComplex256",
                AvailabilityRule::Complex256,
                RepresentationRole::ScalarC256,
            ),
        ];

        for (key, availability, representation) in target_scalars {
            let target_scalar = declaration(&catalog, key);

            assert_eq!(target_scalar.availability_rule(), availability);
            assert_eq!(target_scalar.representation_role(), Some(representation));
        }

        assert_eq!(
            future.representation_role(),
            Some(RepresentationRole::Future)
        );

        assert_eq!(
            future_start.owner(),
            CompilerKnownDeclarationOwner::Declaration(future.id())
        );

        assert_eq!(
            future_start.implementation_hook(),
            Some(ImplementationHook::FutureStart)
        );

        assert_eq!(task.representation_role(), Some(RepresentationRole::Task));

        assert_eq!(
            task_join.owner(),
            CompilerKnownDeclarationOwner::Declaration(task.id())
        );

        assert_eq!(
            task_join.implementation_hook(),
            Some(ImplementationHook::TaskJoin)
        );

        assert_eq!(
            task_cancel.owner(),
            CompilerKnownDeclarationOwner::Declaration(task.id())
        );

        assert_eq!(
            task_cancel.implementation_hook(),
            Some(ImplementationHook::TaskCancel)
        );

        let Some(core_memory) = catalog
            .compiler_known_scopes()
            .iter()
            .find(|scope| scope.key().as_str() == "CoreMemory")
        else {
            panic!("core memory scope should exist");
        };

        let CatalogScopeLocation::Module(core_memory_path) = core_memory.location() else {
            panic!("core memory should be a module scope");
        };

        assert_eq!(
            core_memory_path.segments().collect::<Vec<_>>(),
            ["core", "memory"]
        );

        assert_eq!(
            catalog
                .compiler_known_values()
                .iter()
                .map(|value| (value.key().as_str(), value.representation_role()))
                .collect::<Vec<_>>(),
            [
                ("False", RepresentationRole::BooleanFalse),
                ("None", RepresentationRole::NoneValue),
                ("True", RepresentationRole::BooleanTrue),
                ("UnitValue", RepresentationRole::UnitValue),
            ]
        );
    }

    #[test]
    fn builder_merges_scopes_and_assigns_canonical_typed_ids() {
        let mut validator = TestFragmentValidator;

        let catalog = match build_catalog(&VALID_INVENTORY, &mut validator) {
            Ok(catalog) => catalog,
            Err(diagnostics) => panic!("test catalog should build: {diagnostics:?}"),
        };

        assert_eq!(catalog.compiler_known_scopes().len(), 1);
        assert_eq!(catalog.compiler_known_declarations().len(), 3);
        assert_eq!(catalog.compiler_known_values().len(), 1);
        assert_eq!(catalog.recognized_standard_library_scopes().len(), 1);

        assert_eq!(
            catalog
                .compiler_known_declarations()
                .iter()
                .map(|declaration| (declaration.id().raw(), declaration.key().as_str()))
                .collect::<Vec<_>>(),
            [(0, "Alpha"), (1, "Read"), (2, "Zed")]
        );

        let read = &catalog.compiler_known_declarations()[1];

        assert_eq!(
            read.owner(),
            CompilerKnownDeclarationOwner::Declaration(crate::CompilerKnownDeclarationId::new(2))
        );

        assert_eq!(read.kind(), CatalogDeclarationKind::Function);
        assert_eq!(read.availability_rule(), AvailabilityRule::RawMemory);

        assert_eq!(
            read.implementation_hook(),
            Some(ImplementationHook::RawPointerRead)
        );

        let scope = &catalog.compiler_known_scopes()[0];

        assert_eq!(
            scope
                .declaration_ids()
                .iter()
                .map(|id| id.raw())
                .collect::<Vec<_>>(),
            [0, 2]
        );

        let value = &catalog.compiler_known_values()[0];

        assert_eq!(value.key().as_str(), "True");
        assert_eq!(value.spelling().as_str(), "true");
        assert_eq!(value.representation_role(), RepresentationRole::BooleanTrue);

        let recognized = &catalog.recognized_standard_library_declarations()[0];

        assert_eq!(recognized.key().as_str(), "Length");

        assert_eq!(
            recognized.owner(),
            RecognizedStandardLibraryDeclarationOwner::Scope(
                crate::RecognizedStandardLibraryScopeId::new(0)
            )
        );

        assert_eq!(recognized.identity().name(), Some("length"));
    }

    #[test]
    fn recognized_declaration_identity_is_explicit_and_typed() {
        let inventory = inventory(
            CatalogKind::RecognizedStandardLibrary,
            concat!(
                "catalog recognized_standard_library revision 1;\n",
                "scope Standard at std {\n",
                "  declaration Indexed {\n",
                "    identity ordinal 7;\n",
                "    surface { func indexed(); }\n",
                "  }\n",
                "}\n",
            ),
        );

        let mut validator = TestFragmentValidator;

        let catalog = match build_catalog(inventory, &mut validator) {
            Ok(catalog) => catalog,
            Err(diagnostics) => panic!("ordinal identity should build: {diagnostics:?}"),
        };

        let declaration = &catalog.recognized_standard_library_declarations()[0];

        assert_eq!(
            declaration.identity(),
            &RecognizedStandardLibraryDeclarationIdentity::Ordinal(7)
        );
    }

    #[test]
    fn declaration_identity_is_required_only_for_recognized_catalogs() {
        let recognized = inventory(
            CatalogKind::RecognizedStandardLibrary,
            concat!(
                "catalog recognized_standard_library revision 1;\n",
                "scope Standard at std {\n",
                "  declaration Missing { surface { func missing(); } }\n",
                "}\n",
            ),
        );

        let mut validator = TestFragmentValidator;

        let recognized_diagnostics = match build_catalog(recognized, &mut validator) {
            Ok(_) => panic!("recognized identity must be required"),
            Err(diagnostics) => diagnostics,
        };

        assert!(contains_kind(&recognized_diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::MissingField {
                field: CatalogField::Identity
            }
        )));

        let compiler_known = inventory(
            CatalogKind::CompilerKnown,
            concat!(
                "catalog compiler_known revision 1;\n",
                "scope Core at ambient {\n",
                "  declaration Invalid {\n",
                "    identity name invalid;\n",
                "    surface { func invalid(); }\n",
                "  }\n",
                "}\n",
            ),
        );

        let mut validator = TestFragmentValidator;

        let compiler_diagnostics = match build_catalog(compiler_known, &mut validator) {
            Ok(_) => panic!("compiler-known identity field must be rejected"),
            Err(diagnostics) => diagnostics,
        };

        assert!(contains_kind(&compiler_diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::UnsupportedDeclarationIdentity { .. }
        )));
    }

    #[test]
    fn duplicate_recognized_external_identities_are_rejected() {
        let inventory = inventory(
            CatalogKind::RecognizedStandardLibrary,
            concat!(
                "catalog recognized_standard_library revision 1;\n",
                "scope Standard at std {\n",
                "  declaration First {\n",
                "    identity name convert;\n",
                "    surface { func first(); }\n",
                "  }\n",
                "}\n",
                "scope StandardExtension at std {\n",
                "  declaration Second {\n",
                "    identity name convert;\n",
                "    surface { func second(); }\n",
                "  }\n",
                "}\n",
            ),
        );

        let mut validator = TestFragmentValidator;

        let diagnostics = match build_catalog(inventory, &mut validator) {
            Ok(_) => panic!("duplicate external identities must be rejected"),
            Err(diagnostics) => diagnostics,
        };

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::DuplicateRecognizedDeclarationIdentity
        )));
    }

    #[test]
    fn descriptor_ids_do_not_depend_on_source_inventory_order() {
        let mut validator = TestFragmentValidator;

        let forward = match build_catalog(&ORDERED_INVENTORY, &mut validator) {
            Ok(catalog) => catalog,
            Err(diagnostics) => panic!("ordered catalog should build: {diagnostics:?}"),
        };

        let mut validator = TestFragmentValidator;

        let reversed = match build_catalog(&REVERSED_INVENTORY, &mut validator) {
            Ok(catalog) => catalog,
            Err(diagnostics) => panic!("reversed catalog should build: {diagnostics:?}"),
        };

        assert_eq!(
            declaration_identity(&forward),
            declaration_identity(&reversed)
        );
    }

    #[test]
    fn builder_reports_field_key_and_metadata_defects_together() {
        let inventory = inventory(
            CatalogKind::CompilerKnown,
            concat!(
                "catalog compiler_known revision 1;\n",
                "scope Ambient at ambient {\n",
                "  declaration Duplicate {\n",
                "    availability Always;\n",
                "    availability NeverKnown;\n",
                "    surface { struct Duplicate {} }\n",
                "  }\n",
                "  declaration Duplicate { surface { struct Duplicate {} } }\n",
                "  declaration DuplicateOperation {\n",
                "    operation PlainConversion;\n",
                "    operation Equality;\n",
                "    surface { trait DuplicateOperation<Target> {} }\n",
                "  }\n",
                "  declaration Missing { representation UnknownRole; }\n",
                "}\n",
            ),
        );

        let mut validator = TestFragmentValidator;

        let diagnostics = match build_catalog(inventory, &mut validator) {
            Ok(_) => panic!("invalid catalog should not build"),
            Err(diagnostics) => diagnostics,
        };

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::DuplicateField {
                field: CatalogField::Availability
            }
        )));

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::DuplicateField {
                field: CatalogField::Operation
            }
        )));

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::DuplicateKey { .. }
        )));

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::UnknownMetadata { .. }
        )));

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::MissingField {
                field: CatalogField::Surface
            }
        )));
    }

    #[test]
    fn builder_rejects_unknown_and_incompatible_typed_metadata() {
        let inventory = inventory(
            CatalogKind::CompilerKnown,
            concat!(
                "catalog compiler_known revision 1;\n",
                "scope Ambient at ambient {\n",
                "  declaration UnknownAvailability {\n",
                "    availability NeverKnown;\n",
                "    surface { struct UnknownAvailability {} }\n",
                "  }\n",
                "  declaration UnknownHook {\n",
                "    implementation MissingHook;\n",
                "    surface { func unknown_hook(); }\n",
                "  }\n",
                "  declaration UnknownOperation {\n",
                "    operation MissingOperation;\n",
                "    surface { trait UnknownOperation {} }\n",
                "  }\n",
                "  declaration BadHook {\n",
                "    implementation RawPointerRead;\n",
                "    surface { struct BadHook {} }\n",
                "  }\n",
                "  declaration FirstRole {\n",
                "    representation ScalarBool;\n",
                "    surface { struct FirstRole {} }\n",
                "  }\n",
                "  declaration SecondRole {\n",
                "    representation ScalarBool;\n",
                "    surface { struct SecondRole {} }\n",
                "  }\n",
                "}\n",
            ),
        );

        let mut validator = TestFragmentValidator;

        let diagnostics = match build_catalog(inventory, &mut validator) {
            Ok(_) => panic!("invalid metadata should not build"),
            Err(diagnostics) => diagnostics,
        };

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::UnknownMetadata {
                metadata: CatalogMetadataKind::Availability,
                ..
            }
        )));

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::UnknownMetadata {
                metadata: CatalogMetadataKind::Implementation,
                ..
            }
        )));

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::UnknownMetadata {
                metadata: CatalogMetadataKind::Operation,
                ..
            }
        )));

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::IncompatibleImplementationHook {
                hook: ImplementationHook::RawPointerRead,
                declaration: CatalogDeclarationKind::Struct
            }
        )));

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::DuplicateRepresentationRole {
                role: RepresentationRole::ScalarBool
            }
        )));
    }

    #[test]
    fn builder_rejects_malformed_operation_contracts() {
        let inventory = inventory(
            CatalogKind::CompilerKnown,
            concat!(
                "catalog compiler_known revision 1;\n",
                "scope Ambient at ambient {\n",
                "  declaration WrongKind {\n",
                "    operation BinaryMultiply;\n",
                "    surface { struct WrongKind {} }\n",
                "  }\n",
                "  declaration Incomplete {\n",
                "    operation PlainConversion;\n",
                "    surface { trait Incomplete<Target> {} }\n",
                "  }\n",
                "  declaration Comparison {\n",
                "    operation Comparison;\n",
                "    surface { trait Comparison<Rhs> {} }\n",
                "  }\n",
                "  declaration ComparisonCall {\n",
                "    owner Comparison;\n",
                "    operation Comparison;\n",
                "    surface { func compare(pos rhs: &Rhs) -> bool; }\n",
                "  }\n",
                "  declaration Add {\n",
                "    operation BinaryAdd;\n",
                "    surface { trait Add<Rhs> {} }\n",
                "  }\n",
                "  declaration AddOutput {\n",
                "    owner Add;\n",
                "    operation BinaryAdd;\n",
                "    surface { type Output; }\n",
                "  }\n",
                "  declaration AddCall {\n",
                "    owner Add;\n",
                "    operation BinaryAdd;\n",
                "    surface { func add(pos rhs: &Rhs) -> Output; }\n",
                "  }\n",
                "  declaration DuplicateAddCall {\n",
                "    owner Add;\n",
                "    operation BinaryAdd;\n",
                "    surface { func add_again(pos rhs: &Rhs) -> Output; }\n",
                "  }\n",
                "  declaration Divide {\n",
                "    operation BinaryDivide;\n",
                "    surface { trait Divide<Rhs> {} }\n",
                "  }\n",
                "  declaration Other { surface { trait Other {} } }\n",
                "  declaration DivideOutput {\n",
                "    owner Other;\n",
                "    operation BinaryDivide;\n",
                "    surface { type Output; }\n",
                "  }\n",
                "  declaration DivideCall {\n",
                "    owner Divide;\n",
                "    operation BinaryDivide;\n",
                "    surface { func divide(pos rhs: &Rhs) -> Output; }\n",
                "  }\n",
                "}\n",
            ),
        );

        let mut validator = BrayFragmentValidator;

        let diagnostics = match build_catalog(inventory, &mut validator) {
            Ok(_) => panic!("malformed operation contracts must be rejected"),
            Err(diagnostics) => diagnostics,
        };

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::IncompatibleOperationRole {
                role: crate::CompilerKnownOperationRole::BinaryMultiply,
                declaration: CatalogDeclarationKind::Struct
            }
        )));

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::IncompleteOperationContract {
                role: crate::CompilerKnownOperationRole::PlainConversion
            }
        )));

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::IncompleteOperationContract {
                role: crate::CompilerKnownOperationRole::Comparison
            }
        )));

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::DuplicateOperationComponent {
                role: crate::CompilerKnownOperationRole::BinaryAdd
            }
        )));

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::InvalidOperationComponentOwner {
                role: crate::CompilerKnownOperationRole::BinaryDivide
            }
        )));
    }

    #[test]
    fn builder_rejects_unknown_owners_cycles_and_invalid_contexts() {
        let inventory = inventory(
            CatalogKind::CompilerKnown,
            concat!(
                "catalog compiler_known revision 1;\n",
                "scope Ambient at ambient {\n",
                "  declaration A { owner B; surface { func a(); } }\n",
                "  declaration B { owner A; surface { func b(); } }\n",
                "  declaration Lost { owner Missing; surface { func lost(); } }\n",
                "  declaration Field { surface { field item; } }\n",
                "}\n",
            ),
        );

        let mut validator = TestFragmentValidator;

        let diagnostics = match build_catalog(inventory, &mut validator) {
            Ok(_) => panic!("invalid ownership should not build"),
            Err(diagnostics) => diagnostics,
        };

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::UnknownOwner
        )));

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::OwnershipCycle
        )));

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::InvalidDeclarationContext {
                owner: None,
                child: CatalogDeclarationKind::StructField
            }
        )));
    }

    #[test]
    fn recognized_catalog_rejects_ambient_scopes_values_and_representation() {
        let inventory = inventory(
            CatalogKind::RecognizedStandardLibrary,
            concat!(
                "catalog recognized_standard_library revision 1;\n",
                "scope Standard at ambient {\n",
                "  declaration Item {\n",
                "    identity name Item;\n",
                "    representation ScalarBool;\n",
                "    operation PlainConversion;\n",
                "    surface { struct Item {} }\n",
                "  }\n",
                "  value True { spelling true; type { bool } representation BooleanTrue; }\n",
                "}\n",
            ),
        );

        let mut validator = TestFragmentValidator;

        let diagnostics = match build_catalog(inventory, &mut validator) {
            Ok(_) => panic!("invalid recognized catalog should not build"),
            Err(diagnostics) => diagnostics,
        };

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::UnsupportedScopeLocation { .. }
        )));

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::UnsupportedEntry { .. }
        )));

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::RecognizedRepresentation
        )));

        assert!(contains_kind(&diagnostics, |kind| matches!(
            kind,
            CatalogDiagnosticKind::RecognizedOperationRole
        )));
    }

    struct BrayFragmentValidator;

    impl CatalogFragmentValidator for BrayFragmentValidator {
        fn validate_declaration_surface(
            &mut self,
            source: CatalogSource,
            surface: CatalogDeclarationSurface,
            context: CatalogSurfaceContext,
        ) -> Result<CatalogDeclarationKind, CatalogDiagnostics> {
            let text = fragment_text(source, surface.anchor());
            let sources = fragment_sources(source.relative_path(), &text);

            let Some(snapshot) = sources.iter().next() else {
                panic!("fragment source should exist");
            };

            let result = parse_declaration_fragment(snapshot, parser_context(context));

            assert!(
                result.diagnostics().is_empty(),
                "{} has parser diagnostics: {:?}",
                source.relative_path(),
                result.diagnostics()
            );

            assert!(
                !result.is_recovered(),
                "{} contains recovered declaration syntax",
                source.relative_path()
            );

            let Some(declaration) = result.declaration() else {
                panic!("catalog surface should contain one declaration");
            };

            Ok(declaration_kind(declaration))
        }

        fn validate_type_surface(
            &mut self,
            source: CatalogSource,
            surface: CatalogTypeSurface,
        ) -> Result<(), CatalogDiagnostics> {
            let text = fragment_text(source, surface.anchor());
            let sources = fragment_sources(source.relative_path(), &text);

            let Some(snapshot) = sources.iter().next() else {
                panic!("fragment source should exist");
            };

            let result = parse_type_expression_fragment(snapshot);

            assert!(
                result.diagnostics().is_empty(),
                "{} has type parser diagnostics: {:?}",
                source.relative_path(),
                result.diagnostics()
            );

            assert!(
                !result.is_recovered(),
                "{} contains recovered type syntax in {text:?}",
                source.relative_path(),
            );

            Ok(())
        }
    }

    fn fragment_text(source: CatalogSource, anchor: CatalogSourceAnchor) -> String {
        let Some(text) = anchor.range().slice_str(source.text()) else {
            panic!("catalog parser should retain a valid fragment range");
        };

        text.to_owned()
    }

    fn fragment_sources(name: &str, text: &str) -> SourceStore {
        let mut sources = SourceStore::new();

        let loaded = sources.insert(
            SourceIdentity::new(0),
            SourceOrigin::test_fixture(name),
            SourceVersion::new(0),
            text.to_owned(),
        );

        if let Err(error) = loaded {
            panic!("catalog fragment should load as UTF-8: {error:?}");
        }

        sources
    }

    fn parser_context(context: CatalogSurfaceContext) -> DeclarationFragmentContext {
        match context {
            CatalogSurfaceContext::Scope => DeclarationFragmentContext::Module,
            CatalogSurfaceContext::Declaration(CatalogDeclarationKind::Struct) => {
                DeclarationFragmentContext::Struct
            }
            CatalogSurfaceContext::Declaration(CatalogDeclarationKind::Union) => {
                DeclarationFragmentContext::Union
            }
            CatalogSurfaceContext::Declaration(CatalogDeclarationKind::UnionVariant) => {
                DeclarationFragmentContext::UnionVariant
            }
            CatalogSurfaceContext::Declaration(CatalogDeclarationKind::Trait) => {
                DeclarationFragmentContext::Trait
            }
            CatalogSurfaceContext::Declaration(
                CatalogDeclarationKind::InherentImplementation
                | CatalogDeclarationKind::UnnamedTraitImplementation
                | CatalogDeclarationKind::NamedTraitImplementation,
            ) => DeclarationFragmentContext::Implementation,
            CatalogSurfaceContext::Declaration(owner) => {
                panic!("unsupported representative declaration owner: {owner:?}")
            }
        }
    }

    fn declaration<'catalog>(
        catalog: &'catalog crate::CompilerKnownCatalog,
        key: &str,
    ) -> &'catalog crate::CompilerKnownDeclarationDescriptor {
        let Some(declaration) = catalog
            .compiler_known_declarations()
            .iter()
            .find(|declaration| declaration.key().as_str() == key)
        else {
            panic!("representative declaration should exist: {key}");
        };

        declaration
    }

    struct TestFragmentValidator;

    impl CatalogFragmentValidator for TestFragmentValidator {
        fn validate_declaration_surface(
            &mut self,
            source: CatalogSource,
            surface: CatalogDeclarationSurface,
            _context: CatalogSurfaceContext,
        ) -> Result<CatalogDeclarationKind, CatalogDiagnostics> {
            let text = surface
                .anchor()
                .range()
                .slice_str(source.text())
                .unwrap_or_default()
                .trim_start();

            if text.starts_with("struct") {
                Ok(CatalogDeclarationKind::Struct)
            } else if text.starts_with("field") {
                Ok(CatalogDeclarationKind::StructField)
            } else {
                Ok(CatalogDeclarationKind::Function)
            }
        }

        fn validate_type_surface(
            &mut self,
            _source: CatalogSource,
            _surface: CatalogTypeSurface,
        ) -> Result<(), CatalogDiagnostics> {
            Ok(())
        }
    }

    fn contains_kind(
        diagnostics: &CatalogDiagnostics,
        predicate: impl Fn(&CatalogDiagnosticKind) -> bool,
    ) -> bool {
        diagnostics
            .diagnostics()
            .iter()
            .any(|diagnostic| predicate(diagnostic.kind()))
    }

    fn inventory(kind: CatalogKind, text: &'static str) -> &'static CatalogSourceInventory {
        let sources = Box::leak(Box::new([CatalogSource::new(
            CatalogSourceId::new(0),
            kind,
            "catalog/test/invalid.braydef",
            text,
        )]));

        Box::leak(Box::new(CatalogSourceInventory { sources }))
    }

    fn declaration_identity(
        catalog: &crate::CompilerKnownCatalog,
    ) -> Vec<(u32, &str, CompilerKnownDeclarationOwner)> {
        catalog
            .compiler_known_declarations()
            .iter()
            .map(|declaration| {
                (
                    declaration.id().raw(),
                    declaration.key().as_str(),
                    declaration.owner(),
                )
            })
            .collect()
    }

    const ORDER_A: &str = concat!(
        "catalog compiler_known revision 1;\n",
        "scope Ambient at ambient {\n",
        "  declaration Zed { surface { struct Zed {} } }\n",
        "}\n",
    );

    const ORDER_B: &str = concat!(
        "catalog compiler_known revision 1;\n",
        "scope Ambient at ambient {\n",
        "  declaration Alpha { surface { struct Alpha {} } }\n",
        "}\n",
    );

    const ORDERED_SOURCES: [CatalogSource; 2] = [
        CatalogSource::new(
            CatalogSourceId::new(0),
            CatalogKind::CompilerKnown,
            "catalog/test/a.braydef",
            ORDER_A,
        ),
        CatalogSource::new(
            CatalogSourceId::new(1),
            CatalogKind::CompilerKnown,
            "catalog/test/b.braydef",
            ORDER_B,
        ),
    ];

    const REVERSED_SOURCES: [CatalogSource; 2] = [
        CatalogSource::new(
            CatalogSourceId::new(0),
            CatalogKind::CompilerKnown,
            "catalog/test/b.braydef",
            ORDER_B,
        ),
        CatalogSource::new(
            CatalogSourceId::new(1),
            CatalogKind::CompilerKnown,
            "catalog/test/a.braydef",
            ORDER_A,
        ),
    ];

    static ORDERED_INVENTORY: CatalogSourceInventory = CatalogSourceInventory {
        sources: &ORDERED_SOURCES,
    };

    static REVERSED_INVENTORY: CatalogSourceInventory = CatalogSourceInventory {
        sources: &REVERSED_SOURCES,
    };

    #[test]
    fn fragment_validator_diagnostics_are_preserved() {
        struct RejectingValidator;

        impl CatalogFragmentValidator for RejectingValidator {
            fn validate_declaration_surface(
                &mut self,
                _source: CatalogSource,
                surface: CatalogDeclarationSurface,
                _context: CatalogSurfaceContext,
            ) -> Result<CatalogDeclarationKind, CatalogDiagnostics> {
                Err(CatalogDiagnostic::new(
                    surface.anchor(),
                    CatalogDiagnosticKind::UnexpectedEndOfFile {
                        expected: crate::CatalogExpectation::EndOfFile,
                    },
                )
                .into())
            }

            fn validate_type_surface(
                &mut self,
                _source: CatalogSource,
                _surface: CatalogTypeSurface,
            ) -> Result<(), CatalogDiagnostics> {
                Ok(())
            }
        }

        let mut validator = RejectingValidator;

        let diagnostics = match build_catalog(&ORDERED_INVENTORY, &mut validator) {
            Ok(_) => panic!("rejecting validator should fail the build"),
            Err(diagnostics) => diagnostics,
        };

        assert_eq!(diagnostics.diagnostics().len(), 2);

        assert!(
            diagnostics
                .diagnostics()
                .windows(2)
                .all(|pair| pair[0] <= pair[1])
        );
    }

    #[test]
    fn catalog_diagnostics_keep_exact_source_anchors() {
        let anchor = CatalogSourceAnchor {
            source: CatalogSourceId::new(4),
            range: bray_source::TextRange::EMPTY,
        };

        let diagnostic = CatalogDiagnostic::new(anchor, CatalogDiagnosticKind::UnknownOwner);

        assert_eq!(diagnostic.anchor(), anchor);
    }
}
