use std::sync::Arc;

use super::super::entry::{
    Anchored, ParsedDeclaration, ParsedDeclarationField, ParsedValue, ParsedValueField,
};
use super::super::{
    CatalogDeclarationSurface, CatalogDiagnostic, CatalogDiagnosticKind, CatalogField,
    CatalogSourceAnchor, CatalogTokenSpelling, CatalogTypeSurface,
};

pub(super) struct DeclarationFields<'entry> {
    pub(super) owner: Option<&'entry Anchored<Arc<str>>>,
    pub(super) availability: Option<&'entry Anchored<Arc<str>>>,
    pub(super) representation: Option<&'entry Anchored<Arc<str>>>,
    pub(super) implementation: Option<&'entry Anchored<Arc<str>>>,
    pub(super) surface: Option<&'entry Anchored<CatalogDeclarationSurface>>,
}

pub(super) struct ValueFields<'entry> {
    pub(super) spelling: Option<&'entry Anchored<CatalogTokenSpelling>>,
    pub(super) type_surface: Option<&'entry Anchored<CatalogTypeSurface>>,
    pub(super) availability: Option<&'entry Anchored<Arc<str>>>,
    pub(super) representation: Option<&'entry Anchored<Arc<str>>>,
}

pub(super) fn declaration_fields<'entry>(
    declaration: &'entry ParsedDeclaration,
    diagnostics: &mut Vec<CatalogDiagnostic>,
) -> DeclarationFields<'entry> {
    let mut result = DeclarationFields {
        owner: None,
        availability: None,
        representation: None,
        implementation: None,
        surface: None,
    };

    for field in &declaration.fields {
        match field {
            ParsedDeclarationField::Owner(value) => {
                set_once(&mut result.owner, value, CatalogField::Owner, diagnostics)
            }
            ParsedDeclarationField::Availability(value) => set_once(
                &mut result.availability,
                value,
                CatalogField::Availability,
                diagnostics,
            ),
            ParsedDeclarationField::Representation(value) => set_once(
                &mut result.representation,
                value,
                CatalogField::Representation,
                diagnostics,
            ),
            ParsedDeclarationField::Implementation(value) => set_once(
                &mut result.implementation,
                value,
                CatalogField::Implementation,
                diagnostics,
            ),
            ParsedDeclarationField::Surface(value) => set_once(
                &mut result.surface,
                value,
                CatalogField::Surface,
                diagnostics,
            ),
        }
    }

    require(
        result.surface.is_some(),
        CatalogField::Surface,
        declaration.key.anchor,
        diagnostics,
    );

    result
}

pub(super) fn value_fields<'entry>(
    value: &'entry ParsedValue,
    diagnostics: &mut Vec<CatalogDiagnostic>,
) -> ValueFields<'entry> {
    let mut result = ValueFields {
        spelling: None,
        type_surface: None,
        availability: None,
        representation: None,
    };

    for field in &value.fields {
        match field {
            ParsedValueField::Spelling(value) => set_once(
                &mut result.spelling,
                value,
                CatalogField::Spelling,
                diagnostics,
            ),
            ParsedValueField::Type(value) => set_once(
                &mut result.type_surface,
                value,
                CatalogField::Type,
                diagnostics,
            ),
            ParsedValueField::Availability(value) => set_once(
                &mut result.availability,
                value,
                CatalogField::Availability,
                diagnostics,
            ),
            ParsedValueField::Representation(value) => set_once(
                &mut result.representation,
                value,
                CatalogField::Representation,
                diagnostics,
            ),
        }
    }

    require(
        result.spelling.is_some(),
        CatalogField::Spelling,
        value.key.anchor,
        diagnostics,
    );

    require(
        result.type_surface.is_some(),
        CatalogField::Type,
        value.key.anchor,
        diagnostics,
    );

    require(
        result.representation.is_some(),
        CatalogField::Representation,
        value.key.anchor,
        diagnostics,
    );

    result
}

fn set_once<'entry, T>(
    slot: &mut Option<&'entry Anchored<T>>,
    value: &'entry Anchored<T>,
    field: CatalogField,
    diagnostics: &mut Vec<CatalogDiagnostic>,
) {
    if slot.is_some() {
        diagnostics.push(CatalogDiagnostic::new(
            value.anchor,
            CatalogDiagnosticKind::DuplicateField { field },
        ));

        return;
    }

    *slot = Some(value);
}

fn require(
    present: bool,
    field: CatalogField,
    anchor: CatalogSourceAnchor,
    diagnostics: &mut Vec<CatalogDiagnostic>,
) {
    if !present {
        diagnostics.push(CatalogDiagnostic::new(
            anchor,
            CatalogDiagnosticKind::MissingField { field },
        ));
    }
}
