use bray_binder::{BinderFactContext, SymbolFactProvider};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnySymbolId, CheckedConstraint, GenericConstraintsFact, GenericOwnerId, SymbolFactRequest,
};

use crate::compilation::binder::{CompilationBinderFacts, binder_fact_error};
use crate::fact::FactQueryError;

pub(super) fn enclosing_generic_constraints(
    facts: &CompilationBinderFacts<'_>,
    mut owner: AnySymbolId,
    diagnostics: &mut DiagnosticBag,
) -> Result<Vec<(GenericOwnerId, CheckedConstraint)>, FactQueryError> {
    let mut result = Vec::new();

    loop {
        if let Some(generic_owner) = GenericOwnerId::try_new(owner) {
            let constraints = facts
                .symbol_fact(SymbolFactRequest::<GenericConstraintsFact>::new(
                    generic_owner,
                ))
                .map_err(binder_fact_error)?;

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

        let Some(container) = facts.symbols().containing_symbol(owner) else {
            break;
        };

        owner = container;
    }

    Ok(result)
}
