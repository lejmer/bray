use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_diagnostics::DiagnosticBag;

use bray_symbols::{
    AnySymbolId, CheckedConstraint, GenericOwnerId, GenericTypeConstraintsQuery, SymbolQueryRequest,
};

use crate::compilation::binder::{CompilationBindingContext, binding_query_error};
use crate::fact::FactQueryError;

pub(super) fn enclosing_generic_constraints(
    binding_context: &CompilationBindingContext<'_>,
    mut owner: AnySymbolId,
    diagnostics: &mut DiagnosticBag,
) -> Result<Vec<(GenericOwnerId, CheckedConstraint)>, FactQueryError> {
    let mut result = Vec::new();

    loop {
        if let Some(generic_owner) = GenericOwnerId::try_new(owner) {
            let constraints = binding_context
                .resolve_symbol_query(SymbolQueryRequest::<GenericTypeConstraintsQuery>::new(
                    generic_owner,
                ))
                .map_err(binding_query_error)?;

            *diagnostics = diagnostics.merged(constraints.diagnostics());

            result.extend(
                constraints
                    .value()
                    .constraints()
                    .iter()
                    .copied()
                    .map(|constraint| (generic_owner, constraint)),
            );
        }

        let Some(container) = binding_context.symbols().containing_symbol(owner) else {
            break;
        };

        owner = container;
    }

    let checker = crate::compilation::checker::CompilationCheckerContext::new(*binding_context)
        .with_type_constraints(&result);

    let mut normalizer = bray_checker::TypeNormalizer::new(&checker, None);

    for (_, constraint) in &mut result {
        *constraint = normalizer.normalize_constraint(*constraint)?;
    }

    diagnostics.extend(normalizer.take_diagnostics());

    Ok(result)
}
