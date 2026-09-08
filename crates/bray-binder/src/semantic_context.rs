use bray_bound_tree::{BoundUnit, BoundUnitKey, BoundUnitKeyData, BoundUnitRoot};
use bray_checker::{
    AnonymousCallableContext, ContractClauseContext, DeclaredUnitContext, SemanticUnitContext,
};
use bray_symbols::{
    AnonymousCallableSymbolId, AnySymbolId, CallableContractClauseKind, SymbolGraph, SymbolKind,
};
use bray_syntax::SyntaxKind;

/// A bound-unit invariant that prevents semantic-context construction.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum SemanticUnitContextError {
    /// The anonymous callable root does not resolve through its local snapshot.
    MissingAnonymousCallable {
        /// The exact semantic unit whose local snapshot was incomplete.
        unit: BoundUnitKey,
        /// The unresolved local callable identity.
        callable: AnonymousCallableSymbolId,
    },
    /// The bound-unit root does not match its stable unit key.
    RootKindMismatch {
        /// The stable key whose category defines the expected root shape.
        unit: BoundUnitKey,
        /// The incompatible root retained by the bound unit.
        root: BoundUnitRoot,
    },
    /// The stable unit owner does not resolve in the symbol graph.
    MissingOwner {
        /// The exact semantic unit whose declared owner was unavailable.
        unit: BoundUnitKey,
    },
    /// A synthesized unit owner does not resolve to its containing declaration.
    MissingDeclaration {
        /// The exact semantic unit requiring a containing declaration.
        unit: BoundUnitKey,
        /// The resolved synthesized owner missing its declaration relationship.
        owner: AnySymbolId,
    },
    /// A contract-clause unit does not retain a recognized callable clause kind.
    InvalidContractClauseKind {
        /// The exact contract-clause unit with incompatible syntax.
        unit: BoundUnitKey,
        /// The incompatible source syntax category.
        actual: SyntaxKind,
    },
}

impl SemanticUnitContextError {
    /// Returns the exact source-correlated semantic unit affected by this failure.
    pub const fn unit(&self) -> &BoundUnitKey {
        match self {
            Self::MissingAnonymousCallable { unit, .. }
            | Self::RootKindMismatch { unit, .. }
            | Self::MissingOwner { unit }
            | Self::MissingDeclaration { unit, .. }
            | Self::InvalidContractClauseKind { unit, .. } => unit,
        }
    }
}

/// Constructs the category-specific semantic context for one bound unit.
pub fn semantic_unit_context(
    symbols: &SymbolGraph,
    unit: &BoundUnit,
) -> Result<SemanticUnitContext, SemanticUnitContextError> {
    match (unit.key().data(), unit.root()) {
        (BoundUnitKeyData::CallableBody(_), BoundUnitRoot::CallableBody { .. }) => Ok(
            SemanticUnitContext::CallableBody(declared_entry(symbols, unit)?),
        ),
        (
            BoundUnitKeyData::AnonymousCallable(_),
            BoundUnitRoot::AnonymousCallable {
                callable,
                execution,
                ..
            },
        ) => {
            let Some(callable) = unit.local_symbols().anonymous_callable(callable) else {
                return Err(SemanticUnitContextError::MissingAnonymousCallable {
                    unit: error_unit(unit),
                    callable,
                });
            };

            // The context and bound unit share the same immutable key identity.
            let key = unit.key().clone();

            Ok(SemanticUnitContext::AnonymousCallable(
                AnonymousCallableContext::new(
                    key,
                    callable.id(),
                    execution,
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
        (BoundUnitKeyData::TargetGate(_), BoundUnitRoot::Expression(_)) => Ok(
            SemanticUnitContext::TargetGate(declared_entry(symbols, unit)?),
        ),
        _ => Err(SemanticUnitContextError::RootKindMismatch {
            unit: error_unit(unit),
            root: unit.root(),
        }),
    }
}

fn contract_clause_kind(
    unit: &BoundUnit,
) -> Result<CallableContractClauseKind, SemanticUnitContextError> {
    let actual = unit.key().source().syntax().syntax_kind();

    match actual {
        SyntaxKind::RequiresClause => Ok(CallableContractClauseKind::Requires),
        SyntaxKind::EnsuresClause => Ok(CallableContractClauseKind::Ensures),
        SyntaxKind::WhenClause => Ok(CallableContractClauseKind::Guard),
        SyntaxKind::WithClause => Ok(CallableContractClauseKind::Static),
        _ => Err(SemanticUnitContextError::InvalidContractClauseKind {
            unit: error_unit(unit),
            actual,
        }),
    }
}

fn declared_entry(
    symbols: &SymbolGraph,
    unit: &BoundUnit,
) -> Result<DeclaredUnitContext, SemanticUnitContextError> {
    let Some(owner) = symbols.symbol_for_key(unit.key().declared_owner()) else {
        return Err(SemanticUnitContextError::MissingOwner {
            unit: error_unit(unit),
        });
    };

    let declaration = if is_runtime_default_provider(owner.kind()) {
        let Some(declaration) = symbols.containing_symbol(owner) else {
            return Err(SemanticUnitContextError::MissingDeclaration {
                unit: error_unit(unit),
                owner,
            });
        };

        declaration
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

fn error_unit(unit: &BoundUnit) -> BoundUnitKey {
    // Context failures outlive the unit borrow, and bound-unit keys are Arc-backed identities.
    unit.key().clone()
}

const fn is_runtime_default_provider(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::CallableParameterDefaultProvider
            | SymbolKind::StructFieldDefaultProvider
            | SymbolKind::UnionPayloadDefaultProvider
    )
}
