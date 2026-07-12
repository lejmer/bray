use std::collections::BTreeSet;

use bray_bound_tree::{BoundPattern, BoundPatternId, BoundPatternKind, BoundPatternMode};
use bray_declarations::SyntaxAnchor;
use bray_symbols::{LocalBindingSymbolId, LocalScopeId, SymbolName, SymbolOrdinal, TypeId};
use bray_syntax::{CasePatternSyntax, IrrefutablePatternSyntax, SourceSyntaxNode, SyntaxToken};

use super::{BindingError, BindingResult};
use crate::BinderFactContext;
use crate::request::{BinderRequestContext, PatternBindingMode};

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct BoundPatternBinding {
    pattern: BoundPatternId,
    bindings: Box<[LocalBindingSymbolId]>,
}

impl BoundPatternBinding {
    pub(crate) const fn pattern(&self) -> BoundPatternId {
        self.pattern
    }

    pub(crate) fn bindings(&self) -> &[LocalBindingSymbolId] {
        &self.bindings
    }
}

macro_rules! define_pattern_binder {
    (
        $method:ident,
        $inner:ident,
        $syntax:ty,
        $children:ident,
        $entries:ident,
        $entry_children:ident,
        $has_alternatives:expr
    ) => {
        pub(crate) fn $method(
            &mut self,
            scope: LocalScopeId,
            syntax: &$syntax,
            input_type: TypeId,
            mode: PatternBindingMode,
        ) -> BindingResult<BoundPatternBinding> {
            let coherent =
                self.push_coherent_bindings(scope, syntax.coherent_binding_occurrences(), mode)?;

            self.$inner(scope, syntax, input_type, mode, &coherent)
        }

        fn $inner(
            &mut self,
            scope: LocalScopeId,
            syntax: &$syntax,
            input_type: TypeId,
            mode: PatternBindingMode,
            coherent: &[(SymbolName, LocalBindingSymbolId)],
        ) -> BindingResult<BoundPatternBinding> {
            self.check_cancellation()?;

            let mut children = Vec::new();
            let mut introduced = Vec::new();
            let mut ordinal = 0_u32;

            for child in syntax.$children() {
                let bound = self.$inner(scope, &child, input_type, mode, coherent)?;

                children.push(bound.pattern());
                introduced.extend_from_slice(bound.bindings());
            }

            for entry in syntax.$entries() {
                let nested = entry.$entry_children().collect::<Vec<_>>();

                for child in &nested {
                    let bound = self.$inner(scope, child, input_type, mode, coherent)?;

                    children.push(bound.pattern());
                    introduced.extend_from_slice(bound.bindings());
                }

                if nested.is_empty()
                    && entry.dot_dot_token().is_none()
                    && let Some(token) = entry.identifier_token()
                    && let Some(binding) = self.push_pattern_binding(
                        scope,
                        &entry,
                        token,
                        mode,
                        &mut ordinal,
                        coherent,
                    )?
                {
                    introduced.push(binding);
                }
            }

            let binding_token = syntax.simple_binding_token();
            let is_binding = binding_token.is_some();

            let direct_binding = if is_binding {
                match binding_token {
                    Some(token) => self.push_pattern_binding(
                        scope,
                        syntax,
                        token,
                        mode,
                        &mut ordinal,
                        coherent,
                    )?,
                    None => None,
                }
            } else {
                None
            };

            let direct_bindings = direct_binding.into_iter().collect::<Vec<_>>();

            introduced.extend(direct_bindings.iter().copied());

            let mut unique = BTreeSet::new();
            introduced.retain(|binding| unique.insert(*binding));

            let pattern = BoundPattern::new(
                self.pattern_origin(syntax),
                input_type,
                bound_mode(mode),
                pattern_kind(syntax, is_binding, $has_alternatives),
                children,
                direct_bindings,
            )
            .with_mutability(syntax.mut_keyword().is_some())
            .with_recovery(syntax.is_recovered());

            let pattern = self
                .unit_mut()
                .tree_mut()
                .push_pattern(pattern)
                .map_err(crate::unit::BoundUnitConstructionError::from)?;

            Ok(BoundPatternBinding {
                pattern,
                bindings: introduced.into_boxed_slice(),
            })
        }
    };
}

impl<C> BinderRequestContext<'_, C>
where
    C: BinderFactContext + ?Sized,
{
    define_pattern_binder!(
        bind_irrefutable_pattern,
        bind_irrefutable_pattern_inner,
        IrrefutablePatternSyntax,
        irrefutable_patterns,
        irrefutable_pattern_entries,
        irrefutable_patterns,
        false
    );

    define_pattern_binder!(
        bind_case_pattern,
        bind_case_pattern_inner,
        CasePatternSyntax,
        case_patterns,
        case_pattern_entries,
        case_patterns,
        true
    );

    fn push_pattern_binding(
        &mut self,
        scope: LocalScopeId,
        syntax: &impl SourceSyntaxNode,
        token: SyntaxToken,
        mode: PatternBindingMode,
        ordinal: &mut u32,
        coherent: &[(SymbolName, LocalBindingSymbolId)],
    ) -> BindingResult<Option<LocalBindingSymbolId>> {
        if mode == PatternBindingMode::Assignment || token.is_missing() {
            return Ok(None);
        }

        let Some(text) = token.text(syntax.source().text()) else {
            return Ok(None);
        };

        let Some(name) = SymbolName::try_new(text) else {
            return Ok(None);
        };

        if let Some((_, binding)) = coherent.iter().find(|(candidate, _)| candidate == &name) {
            return Ok(Some(*binding));
        }

        let current = SymbolOrdinal::new(*ordinal);
        *ordinal = ordinal
            .checked_add(1)
            .ok_or(BindingError::IdentityCapacityExceeded)?;

        let anchor = SyntaxAnchor::from_node(syntax);

        let binding = self.unit_mut().push_binding(
            scope,
            name,
            [anchor],
            Some(current),
            anchor.is_recovered() || token.is_missing(),
        )?;

        Ok(Some(binding))
    }

    fn push_coherent_bindings(
        &mut self,
        scope: LocalScopeId,
        occurrences: Vec<BindingOccurrence>,
        mode: PatternBindingMode,
    ) -> BindingResult<Vec<(SymbolName, LocalBindingSymbolId)>> {
        if mode == PatternBindingMode::Assignment {
            return Ok(Vec::new());
        }

        let mut grouped = Vec::<(SymbolName, Vec<SyntaxAnchor>, bool)>::new();

        for occurrence in occurrences {
            match grouped
                .iter_mut()
                .find(|(name, _, _)| name == &occurrence.name)
            {
                Some((_, anchors, is_recovered)) => {
                    anchors.push(occurrence.anchor);
                    *is_recovered |= occurrence.is_recovered;
                }
                None => grouped.push((
                    occurrence.name,
                    vec![occurrence.anchor],
                    occurrence.is_recovered,
                )),
            }
        }

        grouped
            .into_iter()
            .enumerate()
            .map(|(index, (name, anchors, is_recovered))| {
                let ordinal = u32::try_from(index)
                    .map(SymbolOrdinal::new)
                    .map_err(|_| BindingError::IdentityCapacityExceeded)?;

                // The pending symbol and coherent-name lookup independently retain shared text.
                let lookup_name = name.clone();

                let binding = self.unit_mut().push_binding(
                    scope,
                    name,
                    anchors,
                    Some(ordinal),
                    is_recovered,
                )?;

                Ok((lookup_name, binding))
            })
            .collect()
    }

    pub(crate) fn activate_pattern_bindings(
        &mut self,
        scope: LocalScopeId,
        pattern: &BoundPatternBinding,
    ) -> BindingResult<()> {
        for binding in pattern.bindings() {
            self.unit_mut().activate_local(scope, *binding)?;
        }

        Ok(())
    }
}

const fn bound_mode(mode: PatternBindingMode) -> BoundPatternMode {
    match mode {
        PatternBindingMode::Declaration => BoundPatternMode::Declaration,
        PatternBindingMode::Assignment => BoundPatternMode::Assignment,
        PatternBindingMode::Match => BoundPatternMode::Match,
    }
}

fn pattern_kind(
    syntax: &impl PatternSyntax,
    is_binding: bool,
    has_alternatives: bool,
) -> BoundPatternKind {
    if has_alternatives && syntax.has_alternative_separator() {
        BoundPatternKind::Alternative
    } else if is_binding {
        BoundPatternKind::Binding
    } else if syntax.has_discard() {
        BoundPatternKind::Discard
    } else if syntax.has_literal() {
        BoundPatternKind::Literal
    } else if syntax.has_none() {
        BoundPatternKind::NullableAbsent
    } else if syntax.has_question() {
        BoundPatternKind::NullablePresent
    } else if syntax.has_box() {
        BoundPatternKind::Box
    } else if syntax.has_dot() {
        BoundPatternKind::Variant
    } else if syntax.has_open_brace() {
        BoundPatternKind::Product
    } else if syntax.has_open_bracket() {
        BoundPatternKind::Array
    } else if syntax.has_open_paren() && syntax.has_comma() {
        BoundPatternKind::Tuple
    } else if syntax.has_open_paren() {
        BoundPatternKind::Grouped
    } else if syntax.has_path() {
        BoundPatternKind::Path
    } else if syntax.has_dot_dot() {
        BoundPatternKind::Remaining
    } else {
        BoundPatternKind::Error
    }
}

trait PatternSyntax {
    fn coherent_binding_occurrences(&self) -> Vec<BindingOccurrence>;
    fn simple_binding_token(&self) -> Option<SyntaxToken>;
    fn has_alternative_separator(&self) -> bool;
    fn has_discard(&self) -> bool;
    fn has_literal(&self) -> bool;
    fn has_none(&self) -> bool;
    fn has_question(&self) -> bool;
    fn has_box(&self) -> bool;
    fn has_dot(&self) -> bool;
    fn has_open_brace(&self) -> bool;
    fn has_open_bracket(&self) -> bool;
    fn has_open_paren(&self) -> bool;
    fn has_comma(&self) -> bool;
    fn has_path(&self) -> bool;
    fn has_dot_dot(&self) -> bool;
}

macro_rules! impl_pattern_syntax {
    ($syntax:ty, $alternatives:expr, $occurrences:expr) => {
        impl PatternSyntax for $syntax {
            fn coherent_binding_occurrences(&self) -> Vec<BindingOccurrence> {
                ($occurrences)(self)
            }

            fn simple_binding_token(&self) -> Option<SyntaxToken> {
                let path_token = self.paths().next().and_then(|path| {
                    let mut tokens = path.identifier_tokens();
                    let first = tokens.next()?;

                    tokens.next().is_none().then_some(first)
                });

                let token = self.identifier_token().or(path_token)?;

                (self.dot_token().is_none()
                    && self.open_paren_token().is_none()
                    && self.open_bracket_token().is_none()
                    && self.open_brace_token().is_none())
                .then_some(token)
            }

            fn has_alternative_separator(&self) -> bool {
                ($alternatives)(self)
            }

            fn has_discard(&self) -> bool {
                self.discard_token().is_some()
            }

            fn has_literal(&self) -> bool {
                self.literal_token().is_some()
            }

            fn has_none(&self) -> bool {
                self.none_keyword().is_some()
            }

            fn has_question(&self) -> bool {
                self.question_token().is_some()
            }

            fn has_box(&self) -> bool {
                self.box_keyword().is_some()
            }

            fn has_dot(&self) -> bool {
                self.dot_token().is_some()
            }

            fn has_open_brace(&self) -> bool {
                self.open_brace_token().is_some()
            }

            fn has_open_bracket(&self) -> bool {
                self.open_bracket_token().is_some()
            }

            fn has_open_paren(&self) -> bool {
                self.open_paren_token().is_some()
            }

            fn has_comma(&self) -> bool {
                self.comma_token().is_some()
            }

            fn has_path(&self) -> bool {
                self.paths().next().is_some()
            }

            fn has_dot_dot(&self) -> bool {
                self.dot_dot_token().is_some()
            }
        }
    };
}

impl_pattern_syntax!(IrrefutablePatternSyntax, |_| false, |_| Vec::new());
impl_pattern_syntax!(
    CasePatternSyntax,
    |syntax: &CasePatternSyntax| syntax.alternative_separator_tokens().next().is_some(),
    coherent_case_binding_occurrences
);

struct BindingOccurrence {
    name: SymbolName,
    anchor: SyntaxAnchor,
    is_recovered: bool,
}

fn coherent_case_binding_occurrences(syntax: &CasePatternSyntax) -> Vec<BindingOccurrence> {
    if syntax.alternative_separator_tokens().next().is_none() {
        return Vec::new();
    }

    let mut occurrences = Vec::new();

    collect_case_binding_occurrences(syntax, &mut occurrences);

    occurrences
}

fn collect_case_binding_occurrences(
    syntax: &CasePatternSyntax,
    occurrences: &mut Vec<BindingOccurrence>,
) {
    if let Some(token) = syntax.simple_binding_token() {
        push_binding_occurrence(syntax, SyntaxAnchor::from_node(syntax), token, occurrences);
    }

    for entry in syntax.case_pattern_entries() {
        let nested = entry.case_patterns().collect::<Vec<_>>();

        if nested.is_empty()
            && entry.dot_dot_token().is_none()
            && let Some(token) = entry.identifier_token()
        {
            push_binding_occurrence(&entry, SyntaxAnchor::from_node(&entry), token, occurrences);
        }

        for child in nested {
            collect_case_binding_occurrences(&child, occurrences);
        }
    }

    for child in syntax.case_patterns() {
        collect_case_binding_occurrences(&child, occurrences);
    }
}

fn push_binding_occurrence(
    syntax: &impl SourceSyntaxNode,
    anchor: SyntaxAnchor,
    token: SyntaxToken,
    occurrences: &mut Vec<BindingOccurrence>,
) {
    if token.is_missing() {
        return;
    }

    let Some(text) = token.text(syntax.source().text()) else {
        return;
    };

    let Some(name) = SymbolName::try_new(text) else {
        return;
    };

    occurrences.push(BindingOccurrence {
        name,
        anchor,
        is_recovered: anchor.is_recovered(),
    });
}

#[cfg(test)]
mod tests {
    use bray_syntax::{
        CasePatternSyntax, SyntaxKind, SyntaxWalkControl, SyntaxWalkEvent, walk_syntax_node,
    };

    use crate::fact::test_support::TestFixture;
    use crate::request::PatternBindingMode;

    #[test]
    fn case_patterns_retain_match_mode_and_alternative_shape() {
        let fixture = TestFixture::from_source(
            "module app; const Size: Int = 1; func main() { match Size { case left | left when true {} } }",
        );
        let facts = fixture.context();
        let (mut request, block) = crate::binding::test_support::request_and_block(&facts);
        let pattern = first_case_pattern(&block);
        assert_eq!(super::coherent_case_binding_occurrences(&pattern).len(), 2);
        let root = request.unit().root_scope();

        let bound = match request.bind_case_pattern(
            root,
            &pattern,
            fixture.declared_type,
            PatternBindingMode::Match,
        ) {
            Ok(bound) => bound,
            Err(error) => panic!("case pattern must bind: {error:?}"),
        };

        let Some(pattern) = request.unit_view().pattern(bound.pattern()) else {
            panic!("bound case pattern must be committed");
        };

        assert_eq!(pattern.mode(), bray_bound_tree::BoundPatternMode::Match);
        assert_eq!(
            pattern.kind(),
            bray_bound_tree::BoundPatternKind::Alternative
        );
        assert_eq!(pattern.children().len(), 2);
        assert_eq!(bound.bindings().len(), 1);

        let result = match request.finish() {
            Ok(result) => result,
            Err(error) => panic!("case pattern identities must freeze: {error:?}"),
        };

        let [binding] = result.unit().local_symbols().bindings() else {
            panic!("coherent alternatives must publish one logical binding");
        };

        assert_eq!(binding.key().anchors().len(), 2);
    }

    #[test]
    fn assignment_patterns_do_not_introduce_local_identities() {
        let fixture = TestFixture::from_source(
            "module app; const Size: Int = 1; func main() { let value = 1; }",
        );
        let facts = fixture.context();
        let (mut request, block) = crate::binding::test_support::request_and_block(&facts);

        let Some(declaration) = block
            .block_items()
            .find_map(|item| item.local_binding_declaration())
        else {
            panic!("test block must contain a local declaration");
        };

        let root = request.unit().root_scope();

        let bound = match request.bind_irrefutable_pattern(
            root,
            &declaration.irrefutable_pattern(),
            fixture.declared_type,
            PatternBindingMode::Assignment,
        ) {
            Ok(bound) => bound,
            Err(error) => panic!("assignment pattern must bind: {error:?}"),
        };

        let Some(pattern) = request.unit_view().pattern(bound.pattern()) else {
            panic!("assignment pattern must be committed");
        };

        assert_eq!(
            pattern.mode(),
            bray_bound_tree::BoundPatternMode::Assignment
        );
        assert!(bound.bindings().is_empty());

        let result = match request.finish() {
            Ok(result) => result,
            Err(error) => panic!("assignment pattern must freeze: {error:?}"),
        };

        assert!(result.unit().local_symbols().bindings().is_empty());
    }

    fn first_case_pattern(block: &bray_syntax::BlockExpressionSyntax) -> CasePatternSyntax {
        let mut pattern = None;

        walk_syntax_node(block, |event| {
            let SyntaxWalkEvent::EnterNode(node) = event else {
                return SyntaxWalkControl::Continue;
            };

            if node.kind() != SyntaxKind::CasePattern {
                return SyntaxWalkControl::Continue;
            }

            pattern = node.cast();

            SyntaxWalkControl::Stop
        });

        match pattern {
            Some(pattern) => pattern,
            None => panic!("test block must contain a case pattern"),
        }
    }
}
