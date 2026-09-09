use std::collections::{BTreeMap, HashMap};

use bray_bound_tree::{BoundUnitKey, BoundUnitKind};
use bray_declarations::{DeclarationKind, DeclarationRecord, SyntaxAnchor};
use bray_symbols::{AnySymbolId, SymbolGraph, SymbolKey};
use bray_syntax::{
    ExpressionSyntax, SyntaxKind, SyntaxTree, SyntaxWalkControl, SyntaxWalkEvent, walk_syntax_tree,
};

use crate::compilation::{
    Compilation, SemanticDataKind, SemanticQueryContext, SemanticQueryFailure,
    SemanticQueryViolation,
};
use crate::fact::{CompilationFactKey, FactQueryError};

#[derive(Hash)]
pub(in crate::compilation) struct DeclaredUnitIndex {
    keys: Box<[BoundUnitKey]>,
    primary: BTreeMap<(SymbolKey, BoundUnitKind), BoundUnitKey>,
}

impl DeclaredUnitIndex {
    fn new(keys: Vec<BoundUnitKey>) -> Result<Self, FactQueryError> {
        let mut primary = BTreeMap::new();

        for key in &keys {
            if !matches!(
                key.kind(),
                BoundUnitKind::CallableBody
                    | BoundUnitKind::ConstantTemplate
                    | BoundUnitKind::PredicateDefinition
                    | BoundUnitKind::RuntimeDefault
            ) {
                continue;
            }

            // The lookup index shares stable identities with the ordered unit inventory.
            let identity = (key.declared_owner().clone(), key.kind());

            if primary.insert(identity, key.clone()).is_some() {
                return Err(SemanticQueryFailure::contract(
                    SemanticQueryContext::Unit(key.clone()),
                    SemanticQueryViolation::CountMismatch {
                        data: SemanticDataKind::BoundUnit,
                        expected: 1,
                        actual: 2,
                    },
                )
                .into());
            }
        }

        Ok(Self {
            keys: keys.into_boxed_slice(),
            primary,
        })
    }
}

impl Compilation {
    fn declared_units(&self) -> Result<&DeclaredUnitIndex, FactQueryError> {
        self.evaluate_query(
            CompilationFactKey::DeclaredUnits,
            &self.state.declared_units,
            || DeclaredUnitIndex::new(self.compute_declared_unit_keys()?),
        )
        .as_ref()
        .map_err(Clone::clone)
    }

    pub(in crate::compilation) fn declared_unit_keys(
        &self,
    ) -> Result<&[BoundUnitKey], FactQueryError> {
        Ok(&self.declared_units()?.keys)
    }

    pub(in crate::compilation) fn declared_unit_key(
        &self,
        owner: AnySymbolId,
        kind: BoundUnitKind,
    ) -> Result<Option<BoundUnitKey>, FactQueryError> {
        let symbols = self.symbol_graph()?;

        let Some(owner) = symbols.symbol_key(owner) else {
            return Ok(None);
        };

        // Lookup and the returned request share Arc-backed identities with the immutable index.
        Ok(self
            .declared_units()?
            .primary
            .get(&(owner.clone(), kind))
            .cloned())
    }

    #[cfg(test)]
    pub(in crate::compilation) fn declared_unit_keys_for_test(
        &self,
    ) -> Result<Vec<BoundUnitKey>, FactQueryError> {
        // Test callers reorder or retain their own unit inventory.
        Ok(self.declared_unit_keys()?.to_vec())
    }

    fn compute_declared_unit_keys(&self) -> Result<Vec<BoundUnitKey>, FactQueryError> {
        let symbols = self.symbol_graph()?;
        let syntax = self.syntax_tree();
        let declarations = self.product_source_graph()?.declarations();
        let syntax_index = SemanticSyntaxIndex::new(syntax, declarations.declarations());

        let mut keys = Vec::new();

        for declaration in declarations.declarations() {
            let Some(symbol) = symbols.symbol_for_declaration(declaration.id()) else {
                continue;
            };

            // Stable symbol keys are Arc-backed and retained by each bound-unit key.
            let owner = symbols.symbol_key(symbol).cloned().ok_or_else(|| {
                FactQueryError::from(SemanticQueryFailure::contract(
                    SemanticQueryContext::Symbol(symbol),
                    SemanticQueryViolation::Missing(SemanticDataKind::SymbolKey),
                ))
            })?;

            self.push_primary_unit_key(&mut keys, declaration, owner.clone(), &syntax_index)?;

            self.push_surface_unit_keys(
                &mut keys,
                declaration,
                symbol,
                owner,
                symbols,
                &syntax_index,
            )?;
        }

        Ok(keys)
    }

    fn push_primary_unit_key(
        &self,
        keys: &mut Vec<BoundUnitKey>,
        declaration: &DeclarationRecord,
        owner: SymbolKey,
        syntax: &SemanticSyntaxIndex,
    ) -> Result<(), FactQueryError> {
        let anchor = declaration.syntax_anchor();

        if callable_body_kind(declaration.kind()) && syntax.has_callable_body(anchor) {
            let context = SemanticQueryContext::SymbolKey(owner.clone());

            push_key(
                keys,
                BoundUnitKey::callable_body(owner, self.bound_source(anchor)?),
                context,
            )?;

            return Ok(());
        }

        let constructor = match declaration.kind() {
            DeclarationKind::Constant
            | DeclarationKind::Static
            | DeclarationKind::TraitConstantMember => BoundUnitKey::constant_template,
            DeclarationKind::Predicate | DeclarationKind::TraitPredicateMember => {
                BoundUnitKey::predicate_definition
            }
            _ => return Ok(()),
        };

        let Some(expression) = syntax.surface_expression(anchor) else {
            return Ok(());
        };

        let context = SemanticQueryContext::SymbolKey(owner.clone());

        push_key(
            keys,
            constructor(owner, self.bound_source(expression)?),
            context,
        )
    }

    fn push_surface_unit_keys(
        &self,
        keys: &mut Vec<BoundUnitKey>,
        declaration: &DeclarationRecord,
        symbol: AnySymbolId,
        owner: SymbolKey,
        symbols: &SymbolGraph,
        syntax: &SemanticSyntaxIndex,
    ) -> Result<(), FactQueryError> {
        for anchor in declaration.surface().constraints() {
            if syntax.has_bound_constraint_expression(*anchor) {
                let context = SemanticQueryContext::SymbolKey(owner.clone());

                push_key(
                    keys,
                    BoundUnitKey::constraint(owner.clone(), self.bound_source(*anchor)?),
                    context,
                )?;
            }
        }

        for anchor in declaration.surface().contract_clauses() {
            if syntax.has_bound_constraint_expression(*anchor) {
                let context = SemanticQueryContext::SymbolKey(owner.clone());

                push_key(
                    keys,
                    BoundUnitKey::contract_clause(owner.clone(), self.bound_source(*anchor)?),
                    context,
                )?;
            }
        }

        let Some(default) = declaration.surface().runtime_default() else {
            return Ok(());
        };

        let Some(expression) = syntax.surface_expression(default) else {
            return Ok(());
        };

        let provider = symbols.runtime_default_provider(symbol).ok_or_else(|| {
            FactQueryError::from(SemanticQueryFailure::contract(
                SemanticQueryContext::Symbol(symbol),
                SemanticQueryViolation::Missing(SemanticDataKind::RuntimeDefault),
            ))
        })?;

        // Synthesized provider keys are Arc-backed and retained by the runtime-default unit key.
        let provider = symbols.symbol_key(provider).cloned().ok_or_else(|| {
            FactQueryError::from(SemanticQueryFailure::contract(
                SemanticQueryContext::Symbol(provider),
                SemanticQueryViolation::Missing(SemanticDataKind::SymbolKey),
            ))
        })?;

        let context = SemanticQueryContext::SymbolKey(provider.clone());

        push_key(
            keys,
            BoundUnitKey::runtime_default(provider, self.bound_source(expression)?),
            context,
        )
    }
}

struct SemanticSyntaxIndex {
    entries: HashMap<SyntaxAnchor, SemanticSyntaxEntry>,
}

impl SemanticSyntaxIndex {
    fn new<'declaration>(
        syntax: &SyntaxTree,
        declarations: impl IntoIterator<Item = &'declaration DeclarationRecord>,
    ) -> Self {
        let mut entries: HashMap<SyntaxAnchor, SemanticSyntaxEntry> = HashMap::new();

        for declaration in declarations {
            entries.entry(declaration.syntax_anchor()).or_default();

            for anchor in declaration
                .surface()
                .directives()
                .iter()
                .chain(declaration.surface().constraints())
                .chain(declaration.surface().contract_clauses())
                .copied()
                .chain(declaration.surface().runtime_default())
            {
                entries.entry(anchor).or_default();
            }
        }

        let mut active = Vec::new();
        let mut depth = 0_usize;

        walk_syntax_tree(syntax, |event| {
            let node = match event {
                SyntaxWalkEvent::EnterNode(node) => {
                    depth += 1;

                    node
                }
                SyntaxWalkEvent::ExitNode(node) => {
                    let anchor = SyntaxAnchor::from_node(&node);

                    if active.last().is_some_and(|(active, _)| *active == anchor) {
                        active.pop();
                    }

                    depth -= 1;

                    return SyntaxWalkControl::Continue;
                }
                SyntaxWalkEvent::Token(_) => return SyntaxWalkControl::Continue,
            };

            let anchor = SyntaxAnchor::from_node(&node);

            if entries.contains_key(&anchor) {
                active.push((anchor, depth));
            }

            if node.kind() == SyntaxKind::CallableBodyBlockExpression
                && let Some((active_anchor, _)) = active.last()
                && let Some(entry) = entries.get_mut(active_anchor)
            {
                entry.has_callable_body = true;
            }

            if let Some((active_anchor, active_depth)) = active.last()
                && let Some(entry) = entries.get_mut(active_anchor)
            {
                let expression_depth = depth - active_depth;

                if node.kind() == SyntaxKind::Expression
                    && entry
                        .surface_expression_depth
                        .is_none_or(|current| expression_depth < current)
                {
                    entry.surface_expression = Some(anchor);
                    entry.surface_expression_depth = Some(expression_depth);

                    entry.surface_expression_is_trait_satisfaction =
                        node.cast::<ExpressionSyntax>().is_some_and(|expression| {
                            expression.trait_satisfaction_constraint().is_some()
                        });
                }
            }

            SyntaxWalkControl::Continue
        });

        Self { entries }
    }

    fn has_callable_body(&self, anchor: SyntaxAnchor) -> bool {
        self.entries
            .get(&anchor)
            .is_some_and(|entry| entry.has_callable_body)
    }

    fn has_bound_constraint_expression(&self, anchor: SyntaxAnchor) -> bool {
        self.entries.get(&anchor).is_some_and(|entry| {
            entry.surface_expression.is_some() && !entry.surface_expression_is_trait_satisfaction
        })
    }

    fn surface_expression(&self, anchor: SyntaxAnchor) -> Option<SyntaxAnchor> {
        self.entries.get(&anchor)?.surface_expression
    }
}

#[derive(Default)]
struct SemanticSyntaxEntry {
    surface_expression: Option<SyntaxAnchor>,
    surface_expression_depth: Option<usize>,
    surface_expression_is_trait_satisfaction: bool,
    has_callable_body: bool,
}

fn callable_body_kind(kind: DeclarationKind) -> bool {
    matches!(
        kind,
        DeclarationKind::Function
            | DeclarationKind::TraitCallableMember
            | DeclarationKind::TypeConstructorMember
            | DeclarationKind::FinalizerMember
            | DeclarationKind::DestructorMember
            | DeclarationKind::ScopeEnterMember
            | DeclarationKind::ScopeExitMember
            | DeclarationKind::TypeCallableMember
    )
}

fn push_key(
    keys: &mut Vec<BoundUnitKey>,
    key: Option<BoundUnitKey>,
    context: SemanticQueryContext,
) -> Result<(), FactQueryError> {
    let key = key.ok_or_else(|| {
        FactQueryError::from(SemanticQueryFailure::contract(
            context,
            SemanticQueryViolation::Missing(SemanticDataKind::BoundUnit),
        ))
    })?;

    keys.push(key);

    Ok(())
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundUnitKind;

    use super::DeclaredUnitIndex;
    use crate::compilation::{
        SemanticDataKind, SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation,
    };
    use crate::fact::FactQueryError;
    use crate::test_support::{compilation, source_input};

    #[test]
    fn primary_declaration_lookups_share_one_inventory() {
        let compilation = compilation(
            r#"
        module app;

        static STORED: i32 = 1;

        const FIXED: i32 = 2;

        predicate valid() = true;

        func check(value: i32 = 3)
            requires(true)
            ensures(true) {}
        "#,
        );

        let index = compilation.declared_units().unwrap();
        let symbols = compilation.symbol_graph().unwrap();

        assert_eq!(index.primary.len(), 5);
        assert_eq!(index.keys.len(), 7);

        for key in index.primary.values() {
            let owner = symbols.symbol_for_key(key.declared_owner()).unwrap();

            assert_eq!(
                compilation.declared_unit_key(owner, key.kind()).unwrap(),
                Some(key.clone()),
            );
        }

        assert!(std::ptr::eq(index, compilation.declared_units().unwrap()));

        assert!(std::ptr::eq(
            index.keys.as_ref(),
            compilation.declared_unit_keys().unwrap(),
        ));
    }

    #[test]
    fn declared_inventory_is_reused_only_for_unchanged_sources() {
        const SOURCE: &str = "module app; static STORED: i32 = 1;";
        let previous = compilation(SOURCE);
        let index = previous.declared_units().unwrap();

        let unchanged = previous
            .updated_sources(vec![source_input(SOURCE, 0)])
            .unwrap();

        assert!(std::ptr::eq(index, unchanged.declared_units().unwrap()));

        let changed = previous
            .updated_sources(vec![source_input(
                "module app; static REPLACED: i32 = 2; func check() {}",
                1,
            )])
            .unwrap();

        assert!(changed.state.declared_units.get().is_none());
        assert_eq!(changed.declared_unit_keys().unwrap().len(), 2);
        assert_eq!(previous.declared_unit_keys().unwrap().len(), 1);
        assert!(!std::ptr::eq(index, changed.declared_units().unwrap()));
    }

    #[test]
    fn duplicate_primary_units_retain_the_conflicting_identity() {
        let compilation = compilation("module app; static STORED: i32 = 1;");
        let key = compilation.declared_unit_keys().unwrap().first().unwrap();

        assert_eq!(key.kind(), BoundUnitKind::ConstantTemplate);

        let error = DeclaredUnitIndex::new(vec![key.clone(), key.clone()])
            .err()
            .unwrap();

        assert_eq!(
            error,
            FactQueryError::from(SemanticQueryFailure::contract(
                SemanticQueryContext::Unit(key.clone()),
                SemanticQueryViolation::CountMismatch {
                    data: SemanticDataKind::BoundUnit,
                    expected: 1,
                    actual: 2,
                },
            ))
        );
    }
}
