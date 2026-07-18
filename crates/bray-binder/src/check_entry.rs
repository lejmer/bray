use bray_bound_tree::{BoundUnit, BoundUnitKeyData, BoundUnitRoot};
use bray_checker::{
    AnonymousCallableCheckEntry, ContractClauseCheckEntry, DeclaredUnitCheckEntry,
    UnitCheckEntryContext,
};
use bray_symbols::{CallableContractClauseKind, SymbolGraph, SymbolKind};
use bray_syntax::SyntaxKind;

/// A bound-unit invariant that prevents checker entry-context construction.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UnitCheckEntryContextError {
    /// The anonymous callable root does not resolve through its local snapshot.
    MissingAnonymousCallable,
    /// The bound-unit root does not match its stable unit key.
    RootKindMismatch,
    /// The stable unit owner does not resolve in the symbol graph.
    MissingOwner,
    /// A synthesized unit owner does not resolve to its containing declaration.
    MissingDeclaration,
    /// A contract-clause unit does not retain a recognized callable clause kind.
    InvalidContractClauseKind,
}

/// Constructs the category-specific semantic inputs for one bound unit.
pub fn unit_check_entry_context(
    symbols: &SymbolGraph,
    unit: &BoundUnit,
) -> Result<UnitCheckEntryContext, UnitCheckEntryContextError> {
    match (unit.key().data(), unit.root()) {
        (BoundUnitKeyData::CallableBody(_), BoundUnitRoot::CallableBody(_)) => Ok(
            UnitCheckEntryContext::CallableBody(declared_entry(symbols, unit)?),
        ),
        (
            BoundUnitKeyData::AnonymousCallable(_),
            BoundUnitRoot::AnonymousCallable { callable, .. },
        ) => {
            let Some(callable) = unit.local_symbols().anonymous_callable(callable) else {
                return Err(UnitCheckEntryContextError::MissingAnonymousCallable);
            };

            // Bound-unit keys are Arc-backed immutable identities shared with the checker request.
            let key = unit.key().clone();

            Ok(UnitCheckEntryContext::AnonymousCallable(
                AnonymousCallableCheckEntry::new(
                    key,
                    callable.id(),
                    callable.parameters().iter().copied(),
                ),
            ))
        }
        (BoundUnitKeyData::RuntimeDefault(_), BoundUnitRoot::Expression(_)) => Ok(
            UnitCheckEntryContext::RuntimeDefault(declared_entry(symbols, unit)?),
        ),
        (BoundUnitKeyData::ConstantTemplate(_), BoundUnitRoot::Expression(_)) => Ok(
            UnitCheckEntryContext::ConstantTemplate(declared_entry(symbols, unit)?),
        ),
        (BoundUnitKeyData::PredicateDefinition(_), BoundUnitRoot::Expression(_)) => Ok(
            UnitCheckEntryContext::PredicateDefinition(declared_entry(symbols, unit)?),
        ),
        (BoundUnitKeyData::Constraint(_), BoundUnitRoot::ExpressionSequence(_)) => Ok(
            UnitCheckEntryContext::Constraint(declared_entry(symbols, unit)?),
        ),
        (BoundUnitKeyData::ContractClause(_), BoundUnitRoot::ExpressionSequence(_)) => {
            let kind = contract_clause_kind(unit)?;
            let result = unit
                .local_symbols()
                .scopes()
                .iter()
                .find_map(bray_symbols::LocalScope::postcondition_result);

            Ok(UnitCheckEntryContext::ContractClause(
                ContractClauseCheckEntry::new(declared_entry(symbols, unit)?, kind, result),
            ))
        }
        _ => Err(UnitCheckEntryContextError::RootKindMismatch),
    }
}

fn contract_clause_kind(
    unit: &BoundUnit,
) -> Result<CallableContractClauseKind, UnitCheckEntryContextError> {
    match unit.key().source().syntax().syntax_kind() {
        SyntaxKind::RequiresClause => Ok(CallableContractClauseKind::Requires),
        SyntaxKind::EnsuresClause => Ok(CallableContractClauseKind::Ensures),
        SyntaxKind::WithClause => Ok(CallableContractClauseKind::Static),
        _ => Err(UnitCheckEntryContextError::InvalidContractClauseKind),
    }
}

fn declared_entry(
    symbols: &SymbolGraph,
    unit: &BoundUnit,
) -> Result<DeclaredUnitCheckEntry, UnitCheckEntryContextError> {
    let Some(owner) = symbols.symbol_for_key(unit.key().declared_owner()) else {
        return Err(UnitCheckEntryContextError::MissingOwner);
    };

    let declaration = if is_runtime_default_provider(owner.kind()) {
        symbols
            .containing_symbol(owner)
            .ok_or(UnitCheckEntryContextError::MissingDeclaration)?
    } else {
        owner
    };

    // Bound-unit keys are Arc-backed immutable identities shared with the checker request.
    Ok(DeclaredUnitCheckEntry::new(
        unit.key().clone(),
        owner,
        declaration,
    ))
}

const fn is_runtime_default_provider(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::CallableParameterDefaultProvider
            | SymbolKind::StructFieldDefaultProvider
            | SymbolKind::UnionPayloadDefaultProvider
    )
}
