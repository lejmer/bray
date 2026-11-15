use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{
    Diagnostic, DiagnosticId, DiagnosticInterfaceDeclarationIdentity,
    DiagnosticInterfaceSymbolIdentity, DiagnosticInterfaceSymbolReference, DiagnosticKind,
    DiagnosticLabel, DiagnosticLabelKind, DiagnosticProductKind, SeverityKind,
};
use bray_package_interface::InterfaceSymbolReference;
use bray_source::SourceSpan;
use bray_symbols::{
    AnySymbolId, ImportedSymbolSkeleton, ProductKind, SymbolGraph,
    diagnostic_external_symbol_identity, diagnostic_symbol_identity, diagnostic_symbol_kind,
};

use crate::compilation::{
    SemanticDataKind, SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation,
};
use crate::fact::FactQueryError;

pub(in crate::compilation) fn source_diagnostic(
    anchor: SyntaxAnchor,
    kind: DiagnosticKind,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticId::new(anchor.full_range().start().bytes()),
        kind,
        SeverityKind::Error,
    )
    .with_primary_span(SourceSpan::new(anchor.source_id(), anchor.full_range()))
}

pub(in crate::compilation) fn labeled_source_diagnostic(
    anchor: SyntaxAnchor,
    kind: DiagnosticKind,
    label: DiagnosticLabelKind,
) -> Diagnostic {
    let span = SourceSpan::new(anchor.source_id(), anchor.full_range());

    source_diagnostic(anchor, kind).with_label(DiagnosticLabel::primary(label, span))
}

pub(in crate::compilation) const fn diagnostic_product_kind(
    kind: ProductKind,
) -> DiagnosticProductKind {
    match kind {
        ProductKind::Executable => DiagnosticProductKind::Executable,
        ProductKind::Library => DiagnosticProductKind::Library,
        ProductKind::Test => DiagnosticProductKind::Test,
    }
}

pub(in crate::compilation) fn symbol_diagnostic_identity(
    symbols: &SymbolGraph,
    imported: Option<&ImportedSymbolSkeleton>,
    symbol: AnySymbolId,
) -> Result<DiagnosticInterfaceSymbolIdentity, FactQueryError> {
    let key = symbols
        .symbol_key(symbol)
        .or_else(|| imported.and_then(|imported| imported.symbol_key(symbol)))
        .ok_or_else(|| {
            FactQueryError::from(SemanticQueryFailure::contract(
                SemanticQueryContext::Symbol(symbol),
                SemanticQueryViolation::Missing(SemanticDataKind::SymbolKey),
            ))
        })?;

    if let Some(name) = symbols.member_name(symbol)
        && let Some(owner) = symbols.containing_symbol(symbol)
        && let Some(owner_key) = symbols
            .symbol_key(owner)
            .or_else(|| imported.and_then(|imported| imported.symbol_key(owner)))
    {
        return Ok(DiagnosticInterfaceSymbolIdentity::Declaration {
            owner: Box::new(diagnostic_symbol_identity(owner_key)),
            kind: diagnostic_symbol_kind(key.data().kind()),
            identity: DiagnosticInterfaceDeclarationIdentity::Name(name.as_str().to_owned()),
        });
    }

    Ok(diagnostic_symbol_identity(key))
}

pub(in crate::compilation) fn diagnostic_interface_symbol_reference(
    reference: &InterfaceSymbolReference,
) -> DiagnosticInterfaceSymbolReference {
    match reference {
        InterfaceSymbolReference::Local(symbol) => {
            DiagnosticInterfaceSymbolReference::Local(symbol.raw())
        }
        InterfaceSymbolReference::Dependency { dependency, key } => {
            DiagnosticInterfaceSymbolReference::Dependency {
                dependency: dependency.raw(),
                identity: diagnostic_external_symbol_identity(key),
            }
        }
        InterfaceSymbolReference::CompilerKnown(reference) => {
            DiagnosticInterfaceSymbolReference::CompilerKnown(diagnostic_symbol_identity(
                reference.key(),
            ))
        }
    }
}
