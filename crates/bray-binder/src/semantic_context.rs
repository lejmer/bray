use bray_bound_tree::{BoundUnit, BoundUnitKeyData, BoundUnitRoot};
use bray_checker::{
    AnonymousCallableContext, ContractClauseContext, DeclaredUnitContext, SemanticUnitContext,
};
use bray_symbols::{CallableContractClauseKind, SymbolGraph, SymbolKind};
use bray_syntax::SyntaxKind;

/// A bound-unit invariant that prevents semantic-context construction.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SemanticUnitContextError {
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

/// Constructs the category-specific semantic context for one bound unit.
pub fn semantic_unit_context(
    symbols: &SymbolGraph,
    unit: &BoundUnit,
) -> Result<SemanticUnitContext, SemanticUnitContextError> {
    match (unit.key().data(), unit.root()) {
        (BoundUnitKeyData::CallableBody(_), BoundUnitRoot::CallableBody(_)) => Ok(
            SemanticUnitContext::CallableBody(declared_entry(symbols, unit)?),
        ),
        (
            BoundUnitKeyData::AnonymousCallable(_),
            BoundUnitRoot::AnonymousCallable { callable, .. },
        ) => {
            let Some(callable) = unit.local_symbols().anonymous_callable(callable) else {
                return Err(SemanticUnitContextError::MissingAnonymousCallable);
            };

            // The context and bound unit share the same immutable key identity.
            let key = unit.key().clone();

            Ok(SemanticUnitContext::AnonymousCallable(
                AnonymousCallableContext::new(
                    key,
                    callable.id(),
                    callable.parameters().iter().copied(),
                ),
            ))
        }
        (BoundUnitKeyData::RuntimeDefault(_), BoundUnitRoot::Expression(_)) => Ok(
            SemanticUnitContext::RuntimeDefault(declared_entry(symbols, unit)?),
        ),
        (BoundUnitKeyData::ConstantTemplate(_), BoundUnitRoot::Expression(_)) => Ok(
            SemanticUnitContext::ConstantTemplate(declared_entry(symbols, unit)?),
        ),
        (BoundUnitKeyData::EmbeddedConstant(_), BoundUnitRoot::Expression(_)) => Ok(
            SemanticUnitContext::EmbeddedConstant(declared_entry(symbols, unit)?),
        ),
        (BoundUnitKeyData::PredicateDefinition(_), BoundUnitRoot::Expression(_)) => Ok(
            SemanticUnitContext::PredicateDefinition(declared_entry(symbols, unit)?),
        ),
        (BoundUnitKeyData::Constraint(_), BoundUnitRoot::ExpressionSequence(_)) => Ok(
            SemanticUnitContext::Constraint(declared_entry(symbols, unit)?),
        ),
        (BoundUnitKeyData::ContractClause(_), BoundUnitRoot::ExpressionSequence(_)) => {
            let kind = contract_clause_kind(unit)?;

            let result = unit
                .local_symbols()
                .scopes()
                .iter()
                .find_map(bray_symbols::LocalScope::postcondition_result);

            Ok(SemanticUnitContext::ContractClause(
                ContractClauseContext::new(declared_entry(symbols, unit)?, kind, result),
            ))
        }
        _ => Err(SemanticUnitContextError::RootKindMismatch),
    }
}

fn contract_clause_kind(
    unit: &BoundUnit,
) -> Result<CallableContractClauseKind, SemanticUnitContextError> {
    match unit.key().source().syntax().syntax_kind() {
        SyntaxKind::RequiresClause => Ok(CallableContractClauseKind::Requires),
        SyntaxKind::EnsuresClause => Ok(CallableContractClauseKind::Ensures),
        SyntaxKind::WithClause => Ok(CallableContractClauseKind::Static),
        _ => Err(SemanticUnitContextError::InvalidContractClauseKind),
    }
}

fn declared_entry(
    symbols: &SymbolGraph,
    unit: &BoundUnit,
) -> Result<DeclaredUnitContext, SemanticUnitContextError> {
    let Some(owner) = symbols.symbol_for_key(unit.key().declared_owner()) else {
        return Err(SemanticUnitContextError::MissingOwner);
    };

    let declaration = if is_runtime_default_provider(owner.kind()) {
        symbols
            .containing_symbol(owner)
            .ok_or(SemanticUnitContextError::MissingDeclaration)?
    } else {
        owner
    };

    // The context and bound unit share the same immutable key identity.
    Ok(DeclaredUnitContext::new(
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
