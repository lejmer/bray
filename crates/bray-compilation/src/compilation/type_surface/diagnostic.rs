use std::collections::BTreeMap;

use bray_binder::BinderFactContext;
use bray_declarations::duplicate_lifecycle_slot_diagnostic;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{AnySymbolId, TypeAssociatedSurface};

use crate::compilation::binder::CompilationBindingContext;
use crate::fact::FactQueryError;

pub(super) fn lifecycle_slot_diagnostics(
    binding_context: &CompilationBindingContext<'_>,
    surface: &TypeAssociatedSurface,
) -> Result<DiagnosticBag, FactQueryError> {
    let declarations = binding_context.compilation().product_source_graph()?.declarations();

    let mut first_by_slot: BTreeMap<_, &bray_declarations::DeclarationRecord> = BTreeMap::new();
    let mut diagnostics = Vec::new();

    for member in surface.lifecycle_members() {
        let Some(declaration) = source_declaration(binding_context, member.id()) else {
            continue;
        };

        let Some(record) = declarations.declaration(declaration) else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        if record.is_recovered() {
            continue;
        }

        if let Some(first) = first_by_slot.get(&member.slot()) {
            diagnostics.push(duplicate_lifecycle_slot_diagnostic(first, record));
        } else {
            first_by_slot.insert(member.slot(), record);
        }
    }

    Ok(DiagnosticBag::from(diagnostics))
}

fn source_declaration(
    binding_context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
) -> Option<bray_declarations::DeclarationId> {
    binding_context.symbols().symbol_key(symbol)?.source_declaration_id()
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_symbols::{NamedTypeSymbolId, SymbolOrigin};

    use crate::test_support::compilation;

    #[test]
    fn named_type_surface_reports_lifecycle_slots_repeated_by_inherent_implementations() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Resource\n",
            "{\n",
            "    finalize()\n",
            "    {\n",
            "    }\n",
            "}\n",
            "impl Resource\n",
            "{\n",
            "    finalize()\n",
            "    {\n",
            "    }\n",
            "}\n",
        ));

        let symbols = match compilation.symbol_graph() {
            Ok(symbols) => symbols,
            Err(error) => panic!("symbol graph must be available: {error:?}"),
        };

        let Some(subject) = symbols
            .structures()
            .iter()
            .find(|symbol| symbol.origin() == SymbolOrigin::Source)
            .map(|symbol| NamedTypeSymbolId::from(symbol.id()))
        else {
            panic!("test source must declare one structure");
        };

        let result = match compilation.type_associated_surface_result(subject) {
            Ok(result) => result,
            Err(error) => panic!("type-associated surface must be available: {error:?}"),
        };

        assert_eq!(
            result
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.kind())
                .collect::<Vec<_>>(),
            [DiagnosticKind::DeclarationDuplicateLifecycleSlot]
        );
    }
}
