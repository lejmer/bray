use bray_bound_tree::{BoundUnit, BoundUnitKeyData, BoundUnitRoot};
use bray_checker::{
    AnonymousCallableContext, ContractClauseContext, DeclaredUnitContext, SemanticUnitContext,
};
use bray_symbols::{CallableContractClauseKind, SymbolGraph, SymbolKind};
use bray_syntax::SyntaxKind;

/// Constructs the category-specific semantic context for one bound unit.
///
/// The symbol graph must own the unit's declared identities. Broken producer contracts panic.
pub fn semantic_unit_context(symbols: &SymbolGraph, unit: &BoundUnit) -> SemanticUnitContext {
    match (unit.key().data(), unit.root()) {
        (BoundUnitKeyData::CallableBody(_), BoundUnitRoot::CallableBody { .. }) => {
            SemanticUnitContext::CallableBody(declared_entry(symbols, unit))
        }
        (
            BoundUnitKeyData::AnonymousCallable(_),
            BoundUnitRoot::AnonymousCallable {
                callable,
                execution,
                ..
            },
        ) => {
            let Some(callable) = unit.local_symbols().anonymous_callable(callable) else {
                panic!(
                    "anonymous callable {callable:?} must belong to bound unit {:?}",
                    unit.key()
                );
            };

            // The context and bound unit share the same immutable key identity.
            let key = unit.key().clone();

            SemanticUnitContext::AnonymousCallable(AnonymousCallableContext::new(
                key,
                callable.id(),
                execution,
                callable.parameters().iter().copied(),
            ))
        }
        (BoundUnitKeyData::RuntimeDefault(_), BoundUnitRoot::Expression(_)) => {
            SemanticUnitContext::RuntimeDefault(declared_entry(symbols, unit))
        }
        (BoundUnitKeyData::ConstantTemplate(_), BoundUnitRoot::Expression(_)) => {
            SemanticUnitContext::ConstantTemplate(declared_entry(symbols, unit))
        }
        (BoundUnitKeyData::EmbeddedConstant(_), BoundUnitRoot::Expression(_)) => {
            SemanticUnitContext::EmbeddedConstant(declared_entry(symbols, unit))
        }
        (BoundUnitKeyData::PredicateDefinition(_), BoundUnitRoot::Expression(_)) => {
            SemanticUnitContext::PredicateDefinition(declared_entry(symbols, unit))
        }
        (BoundUnitKeyData::Constraint(_), BoundUnitRoot::ExpressionSequence(_)) => {
            SemanticUnitContext::Constraint(declared_entry(symbols, unit))
        }
        (BoundUnitKeyData::ContractClause(_), BoundUnitRoot::ExpressionSequence(_)) => {
            let kind = contract_clause_kind(unit);

            let result = unit
                .local_symbols()
                .scopes()
                .iter()
                .find_map(bray_symbols::LocalScope::postcondition_result);

            SemanticUnitContext::ContractClause(ContractClauseContext::new(
                declared_entry(symbols, unit),
                kind,
                result,
            ))
        }
        (BoundUnitKeyData::TargetGate(_), BoundUnitRoot::Expression(_)) => {
            SemanticUnitContext::TargetGate(declared_entry(symbols, unit))
        }
        _ => panic!(
            "bound root {:?} must match semantic unit {:?}",
            unit.root(),
            unit.key()
        ),
    }
}

fn contract_clause_kind(unit: &BoundUnit) -> CallableContractClauseKind {
    let actual = unit.key().source().syntax().syntax_kind();

    match actual {
        SyntaxKind::RequiresClause | SyntaxKind::WhenClause => CallableContractClauseKind::Requires,
        SyntaxKind::EnsuresClause => CallableContractClauseKind::Ensures,
        SyntaxKind::WithClause => CallableContractClauseKind::Static,
        _ => panic!(
            "contract clause unit {:?} must carry a clause syntax kind, found {actual:?}",
            unit.key()
        ),
    }
}

fn declared_entry(symbols: &SymbolGraph, unit: &BoundUnit) -> DeclaredUnitContext {
    let Some(owner) = symbols.symbol_for_key(unit.key().declared_owner()) else {
        panic!(
            "semantic unit {:?} must have an owner in its symbol graph",
            unit.key()
        );
    };

    let declaration = if is_runtime_default_provider(owner.kind()) {
        let Some(declaration) = symbols.containing_symbol(owner) else {
            panic!(
                "default provider {owner:?} for unit {:?} must have a containing declaration",
                unit.key()
            );
        };

        declaration
    } else {
        owner
    };

    // The context and bound unit share the same immutable key identity.
    DeclaredUnitContext::new(unit.key().clone(), owner, declaration)
}

const fn is_runtime_default_provider(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::CallableParameterDefaultProvider
            | SymbolKind::StructFieldDefaultProvider
            | SymbolKind::UnionPayloadDefaultProvider
    )
}
