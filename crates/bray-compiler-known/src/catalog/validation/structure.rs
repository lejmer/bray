use std::collections::BTreeMap;
use std::sync::Arc;

use crate::RepresentationRole;

use super::super::entry::{ParsedCatalogSource, ParsedEntry, ParsedScope, ParsedScopeLocation};
use super::super::{
    CatalogDiagnostic, CatalogDiagnosticKind, CatalogEntryKind, CatalogKeyDomain, CatalogKind,
    CatalogPath, CatalogRelatedKey, CatalogScopeLocation, CatalogSourceInventory,
};
use super::CatalogFragmentValidator;
use super::field::{declaration_fields, value_fields};
use super::identity::{recognized_identity, validate_recognized_identity_uniqueness};
use super::iteration::validate_iteration_protocol;
use super::metadata::{availability, implementation, iteration, operation, representation};
use super::model::{
    RawDeclaration, RawScope, RawValue, ValidatedCatalog, ValidatedDeclaration, ValidatedScope,
    ValidatedValue,
};
use super::operation::validate_operation_contracts;
use super::owner::resolve_declarations;

pub(crate) fn validate_catalog(
    parsed_sources: &[ParsedCatalogSource],
    inventory: &'static CatalogSourceInventory,
    validator: &mut impl CatalogFragmentValidator,
    diagnostics: &mut Vec<CatalogDiagnostic>,
) -> Option<ValidatedCatalog> {
    let (raw_scopes, mut declarations, mut values) = collect(parsed_sources, diagnostics);

    let (compiler_scopes, recognized_scopes) = split_scopes(raw_scopes);

    declarations.sort_by(|left, right| {
        (left.catalog_kind, left.key.as_ref()).cmp(&(right.catalog_kind, right.key.as_ref()))
    });

    reject_duplicate_declarations(&mut declarations, diagnostics);

    values.sort_by(|left, right| left.key.cmp(&right.key));

    reject_duplicate_values(&mut values, diagnostics);

    let recognized_start = declarations
        .partition_point(|declaration| declaration.catalog_kind == CatalogKind::CompilerKnown);

    let (compiler_declarations, recognized_declarations) =
        declarations.split_at_mut(recognized_start);

    resolve_declarations(
        compiler_declarations,
        &compiler_scopes,
        inventory,
        validator,
        diagnostics,
    );

    resolve_declarations(
        recognized_declarations,
        &recognized_scopes,
        inventory,
        validator,
        diagnostics,
    );

    validate_recognized_identity_uniqueness(
        recognized_declarations,
        &recognized_scopes,
        diagnostics,
    );

    validate_values(&values, inventory, validator, diagnostics);
    validate_representation_uniqueness(compiler_declarations, &values, diagnostics);
    validate_iteration_protocol(compiler_declarations, diagnostics);
    validate_operation_contracts(compiler_declarations, diagnostics);

    if !diagnostics.is_empty() {
        return None;
    }

    let compiler_scope_indexes = compiler_scopes
        .iter()
        .enumerate()
        .map(|(index, scope)| (Arc::clone(&scope.key), index))
        .collect();

    Some(ValidatedCatalog {
        compiler_known_scopes: compiler_scopes,
        compiler_known_declarations: finalize_declarations(compiler_declarations)?,
        compiler_known_values: finalize_values(values, &compiler_scope_indexes)?,
        recognized_scopes,
        recognized_declarations: finalize_declarations(recognized_declarations)?,
    })
}

fn collect(
    parsed_sources: &[ParsedCatalogSource],
    diagnostics: &mut Vec<CatalogDiagnostic>,
) -> (Vec<RawScope>, Vec<RawDeclaration>, Vec<RawValue>) {
    let mut scopes = BTreeMap::<(CatalogKind, Arc<str>), RawScope>::new();
    let mut declarations = Vec::new();
    let mut values = Vec::new();

    for parsed in parsed_sources {
        if parsed.source_kind != parsed.declared_kind.value {
            diagnostics.push(CatalogDiagnostic::new(
                parsed.declared_kind.anchor,
                CatalogDiagnosticKind::CatalogKindMismatch {
                    expected: parsed.source_kind,
                    actual: parsed.declared_kind.value,
                },
            ));
        }

        for scope in &parsed.scopes {
            let location = scope_location(parsed.source_kind, scope, diagnostics);

            collect_entries(
                parsed.source_kind,
                scope,
                &mut declarations,
                &mut values,
                diagnostics,
            );

            let Some(location) = location else {
                continue;
            };

            let map_key = (parsed.source_kind, Arc::clone(&scope.key.value));

            if let Some(existing) = scopes.get(&map_key) {
                if existing.location != location {
                    diagnostics.push(
                        CatalogDiagnostic::new(
                            scope.location.anchor,
                            CatalogDiagnosticKind::ConflictingScopeLocation,
                        )
                        .with_related_keys([CatalogRelatedKey::new(
                            scope_domain(parsed.source_kind),
                            Arc::clone(&scope.key.value),
                        )]),
                    );
                }
            } else {
                scopes.insert(
                    map_key,
                    RawScope {
                        catalog_kind: parsed.source_kind,
                        key: Arc::clone(&scope.key.value),
                        location,
                        anchor: scope.key.anchor,
                    },
                );
            }
        }
    }

    (scopes.into_values().collect(), declarations, values)
}

fn collect_entries(
    catalog_kind: CatalogKind,
    scope: &ParsedScope,
    declarations: &mut Vec<RawDeclaration>,
    values: &mut Vec<RawValue>,
    diagnostics: &mut Vec<CatalogDiagnostic>,
) {
    for entry in &scope.entries {
        match entry {
            ParsedEntry::Declaration(declaration) => {
                let fields = declaration_fields(declaration, diagnostics);

                let representation_role = representation(
                    fields
                        .representation
                        .map(|value| (&value.value, value.anchor)),
                    diagnostics,
                );

                if catalog_kind == CatalogKind::RecognizedStandardLibrary
                    && representation_role.is_some()
                {
                    diagnostics.push(CatalogDiagnostic::new(
                        fields
                            .representation
                            .map_or(declaration.key.anchor, |value| value.anchor),
                        CatalogDiagnosticKind::RecognizedRepresentation,
                    ));
                }

                let operation_role = operation(
                    fields.operation.map(|value| (&value.value, value.anchor)),
                    diagnostics,
                );

                let operation_role = if catalog_kind == CatalogKind::RecognizedStandardLibrary
                    && operation_role.is_some()
                {
                    diagnostics.push(CatalogDiagnostic::new(
                        fields
                            .operation
                            .map_or(declaration.key.anchor, |value| value.anchor),
                        CatalogDiagnosticKind::RecognizedOperationRole,
                    ));

                    None
                } else {
                    operation_role
                };

                let iteration_role = iteration(
                    fields.iteration.map(|value| (&value.value, value.anchor)),
                    diagnostics,
                );

                let iteration_role = if catalog_kind == CatalogKind::RecognizedStandardLibrary
                    && iteration_role.is_some()
                {
                    diagnostics.push(CatalogDiagnostic::new(
                        fields
                            .iteration
                            .map_or(declaration.key.anchor, |value| value.anchor),
                        CatalogDiagnosticKind::RecognizedIterationRole,
                    ));

                    None
                } else {
                    iteration_role
                };

                let recognized_identity = match (catalog_kind, fields.identity) {
                    (CatalogKind::RecognizedStandardLibrary, Some(identity)) => {
                        Some(recognized_identity(&identity.value))
                    }
                    (CatalogKind::RecognizedStandardLibrary, None) => {
                        diagnostics.push(CatalogDiagnostic::new(
                            declaration.key.anchor,
                            CatalogDiagnosticKind::MissingField {
                                field: super::super::CatalogField::Identity,
                            },
                        ));

                        None
                    }
                    (CatalogKind::CompilerKnown, Some(identity)) => {
                        diagnostics.push(CatalogDiagnostic::new(
                            identity.anchor,
                            CatalogDiagnosticKind::UnsupportedDeclarationIdentity {
                                catalog: catalog_kind,
                            },
                        ));

                        None
                    }
                    (CatalogKind::CompilerKnown, None) => None,
                };

                let Some(surface) = fields.surface else {
                    continue;
                };

                declarations.push(RawDeclaration {
                    catalog_kind,
                    scope_key: Arc::clone(&scope.key.value),
                    key: Arc::clone(&declaration.key.value),
                    owner_key: fields.owner.map(|value| Arc::clone(&value.value)),
                    owner: None,
                    recognized_identity,
                    kind: None,
                    surface: surface.value,
                    representation_role,
                    implementation_hook: implementation(
                        fields
                            .implementation
                            .map(|value| (&value.value, value.anchor)),
                        diagnostics,
                    ),
                    iteration_role,
                    operation_role,
                    availability_rule: availability(
                        fields
                            .availability
                            .map(|value| (&value.value, value.anchor)),
                        diagnostics,
                    ),
                    anchor: declaration.key.anchor,
                });
            }
            ParsedEntry::Value(value) => {
                if catalog_kind == CatalogKind::RecognizedStandardLibrary {
                    diagnostics.push(CatalogDiagnostic::new(
                        value.key.anchor,
                        CatalogDiagnosticKind::UnsupportedEntry {
                            catalog: catalog_kind,
                            entry: CatalogEntryKind::Value,
                        },
                    ));

                    continue;
                }

                let fields = value_fields(value, diagnostics);

                let (Some(spelling), Some(type_surface), Some(representation_spelling)) =
                    (fields.spelling, fields.type_surface, fields.representation)
                else {
                    continue;
                };

                let Some(representation_role) = representation(
                    Some((
                        &representation_spelling.value,
                        representation_spelling.anchor,
                    )),
                    diagnostics,
                ) else {
                    continue;
                };

                values.push(RawValue {
                    scope_key: Arc::clone(&scope.key.value),
                    key: Arc::clone(&value.key.value),
                    spelling: spelling.value.to_owned_storage(),
                    type_surface: type_surface.value,
                    representation_role,
                    availability_rule: availability(
                        fields
                            .availability
                            .map(|value| (&value.value, value.anchor)),
                        diagnostics,
                    ),
                    anchor: value.key.anchor,
                });
            }
        }
    }
}

fn scope_location(
    catalog_kind: CatalogKind,
    scope: &ParsedScope,
    diagnostics: &mut Vec<CatalogDiagnostic>,
) -> Option<CatalogScopeLocation> {
    match &scope.location.value {
        ParsedScopeLocation::Ambient if catalog_kind == CatalogKind::CompilerKnown => {
            Some(CatalogScopeLocation::Ambient)
        }
        ParsedScopeLocation::Ambient => {
            diagnostics.push(CatalogDiagnostic::new(
                scope.location.anchor,
                CatalogDiagnosticKind::UnsupportedScopeLocation {
                    catalog: catalog_kind,
                },
            ));

            None
        }
        ParsedScopeLocation::Path(segments) => {
            CatalogPath::try_new(segments.iter().map(Arc::clone)).map(CatalogScopeLocation::Module)
        }
    }
}

fn split_scopes(raw_scopes: Vec<RawScope>) -> (Vec<ValidatedScope>, Vec<ValidatedScope>) {
    let mut compiler = Vec::new();
    let mut recognized = Vec::new();

    for scope in raw_scopes {
        let validated = ValidatedScope {
            key: scope.key,
            location: scope.location,
            anchor: scope.anchor,
        };

        match scope.catalog_kind {
            CatalogKind::CompilerKnown => compiler.push(validated),
            CatalogKind::RecognizedStandardLibrary => recognized.push(validated),
        }
    }

    (compiler, recognized)
}

fn reject_duplicate_declarations(
    declarations: &mut Vec<RawDeclaration>,
    diagnostics: &mut Vec<CatalogDiagnostic>,
) {
    reject_duplicates(
        declarations,
        |declaration| (declaration.catalog_kind, Arc::clone(&declaration.key)),
        |declaration| declaration.anchor,
        |declaration| match declaration.catalog_kind {
            CatalogKind::CompilerKnown => CatalogKeyDomain::CompilerKnownDeclaration,
            CatalogKind::RecognizedStandardLibrary => {
                CatalogKeyDomain::RecognizedStandardLibraryDeclaration
            }
        },
        diagnostics,
    );
}

fn reject_duplicate_values(values: &mut Vec<RawValue>, diagnostics: &mut Vec<CatalogDiagnostic>) {
    reject_duplicates(
        values,
        |value| Arc::clone(&value.key),
        |value| value.anchor,
        |_| CatalogKeyDomain::CompilerKnownValue,
        diagnostics,
    );
}

fn reject_duplicates<T, K: Ord>(
    items: &mut Vec<T>,
    key: impl Fn(&T) -> K,
    anchor: impl Fn(&T) -> super::super::CatalogSourceAnchor,
    domain: impl Fn(&T) -> CatalogKeyDomain,
    diagnostics: &mut Vec<CatalogDiagnostic>,
) {
    let mut previous: Option<K> = None;

    items.retain(|item| {
        let item_key = key(item);
        let duplicate = previous.as_ref() == Some(&item_key);

        if duplicate {
            diagnostics.push(CatalogDiagnostic::new(
                anchor(item),
                CatalogDiagnosticKind::DuplicateKey {
                    domain: domain(item),
                },
            ));

            false
        } else {
            previous = Some(item_key);
            true
        }
    });
}

fn validate_values(
    values: &[RawValue],
    inventory: &'static CatalogSourceInventory,
    validator: &mut impl CatalogFragmentValidator,
    diagnostics: &mut Vec<CatalogDiagnostic>,
) {
    for value in values {
        let value_role = matches!(
            value.representation_role,
            RepresentationRole::BooleanTrue
                | RepresentationRole::BooleanFalse
                | RepresentationRole::UnitValue
                | RepresentationRole::NoneValue
        );

        if !value_role {
            diagnostics.push(CatalogDiagnostic::new(
                value.anchor,
                CatalogDiagnosticKind::IncompatibleRepresentationRole {
                    role: value.representation_role,
                    entry: CatalogEntryKind::Value,
                },
            ));
        }

        let Some(source) = inventory.source(value.type_surface.anchor().source()) else {
            continue;
        };

        if let Err(fragment_diagnostics) =
            validator.validate_type_surface(*source, value.type_surface)
        {
            diagnostics.extend_from_slice(fragment_diagnostics.diagnostics());
        }
    }
}

fn validate_representation_uniqueness(
    declarations: &[RawDeclaration],
    values: &[RawValue],
    diagnostics: &mut Vec<CatalogDiagnostic>,
) {
    let mut roles = BTreeMap::new();

    for (role, anchor) in declarations
        .iter()
        .filter_map(|declaration| {
            declaration
                .representation_role
                .map(|role| (role, declaration.anchor))
        })
        .chain(
            values
                .iter()
                .map(|value| (value.representation_role, value.anchor)),
        )
    {
        if roles.insert(role, anchor).is_some() {
            diagnostics.push(CatalogDiagnostic::new(
                anchor,
                CatalogDiagnosticKind::DuplicateRepresentationRole { role },
            ));
        }
    }
}

fn finalize_declarations(declarations: &[RawDeclaration]) -> Option<Vec<ValidatedDeclaration>> {
    declarations
        .iter()
        .map(|declaration| {
            Some(ValidatedDeclaration {
                key: Arc::clone(&declaration.key),
                owner: declaration.owner?,
                // Finalized descriptors own identity independently of validation scratch data.
                recognized_identity: declaration.recognized_identity.clone(),
                kind: declaration.kind?,
                surface: declaration.surface,
                representation_role: declaration.representation_role,
                implementation_hook: declaration.implementation_hook,
                iteration_role: declaration.iteration_role,
                operation_role: declaration.operation_role,
                availability_rule: declaration.availability_rule,
                anchor: declaration.anchor,
            })
        })
        .collect()
}

fn finalize_values(
    values: Vec<RawValue>,
    scope_indexes: &BTreeMap<Arc<str>, usize>,
) -> Option<Vec<ValidatedValue>> {
    values
        .into_iter()
        .map(|value| {
            Some(ValidatedValue {
                owner_scope: *scope_indexes.get(&value.scope_key)?,
                key: value.key,
                spelling: value.spelling,
                type_surface: value.type_surface,
                representation_role: value.representation_role,
                availability_rule: value.availability_rule,
                anchor: value.anchor,
            })
        })
        .collect()
}

fn scope_domain(kind: CatalogKind) -> CatalogKeyDomain {
    match kind {
        CatalogKind::CompilerKnown => CatalogKeyDomain::CompilerKnownScope,
        CatalogKind::RecognizedStandardLibrary => CatalogKeyDomain::RecognizedStandardLibraryScope,
    }
}
