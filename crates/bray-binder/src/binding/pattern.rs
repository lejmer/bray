use std::collections::BTreeSet;

use bray_bound_tree::{
    BoundPattern, BoundPatternId, BoundPatternKind, BoundPatternMode, BoundPatternTarget,
};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{Diagnostic, DiagnosticId, DiagnosticKind, SeverityKind};
use bray_symbols::{LocalBindingSymbolId, LocalScopeId, SymbolName, SymbolOrdinal, TypeId};
use bray_syntax::{CasePatternSyntax, IrrefutablePatternSyntax, SourceSyntaxNode, SyntaxToken};

use super::name::{name_is_available, name_text_is_available, report_name_already_defined};
use super::{BindingError, BindingResult};
use crate::BinderFactContext;
use crate::binder::{Binder, PatternBindingMode};
use crate::lookup::PathBindingContext;

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

struct PatternBindingState {
    context: PathBindingContext,
    input_type: TypeId,
    mode: PatternBindingMode,
    coherent: Vec<(SymbolName, LocalBindingSymbolId)>,
    pending_names: BTreeSet<SymbolName>,
    suppress_bindings: bool,
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
            context: PathBindingContext,
            syntax: &$syntax,
            input_type: TypeId,
            mode: PatternBindingMode,
        ) -> BindingResult<BoundPatternBinding> {
            let alternatives_are_coherent = !$has_alternatives
                || !syntax.has_alternative_separator()
                || syntax.alternative_bindings_are_coherent();

            if !alternatives_are_coherent {
                self.report_incoherent_alternative_pattern(syntax);
            }

            let occurrences = alternatives_are_coherent
                .then(|| syntax.coherent_binding_occurrences())
                .unwrap_or_default();

            let coherent = self.push_coherent_bindings(context, occurrences, mode)?;

            // Pending identities and coherent lookup independently retain shared name text.
            let pending_names = coherent
                .iter()
                .map(|(name, _)| name.clone())
                .collect::<BTreeSet<_>>();

            let mut state = PatternBindingState {
                context,
                input_type,
                mode,
                coherent,
                pending_names,
                suppress_bindings: !alternatives_are_coherent,
            };

            self.$inner(syntax, &mut state)
        }

        fn $inner(
            &mut self,
            syntax: &$syntax,
            state: &mut PatternBindingState,
        ) -> BindingResult<BoundPatternBinding> {
            self.check_cancellation()?;

            let mut children = Vec::new();
            let mut introduced = Vec::new();
            let mut ordinal = 0_u32;

            for child in syntax.$children() {
                let bound = self.$inner(&child, state)?;

                children.push(bound.pattern());
                introduced.extend_from_slice(bound.bindings());
            }

            for entry in syntax.$entries() {
                let nested = entry.$entry_children().collect::<Vec<_>>();

                for child in &nested {
                    let bound = self.$inner(child, state)?;

                    children.push(bound.pattern());
                    introduced.extend_from_slice(bound.bindings());
                }

                if nested.is_empty()
                    && entry.dot_dot_token().is_none()
                    && let Some(token) = entry.identifier_token()
                    && let Some(binding) =
                        self.push_pattern_binding(&entry, token, &mut ordinal, state)?
                {
                    introduced.push(binding);
                }
            }

            let binding_token = syntax.simple_binding_token();
            let target = self.bind_pattern_target(state.context, syntax, state.mode)?;

            let is_binding = binding_token.is_some()
                && target.is_none()
                && state.mode != PatternBindingMode::Assignment;

            let direct_binding = if is_binding {
                match binding_token {
                    Some(token) => self.push_pattern_binding(syntax, token, &mut ordinal, state)?,
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
                state.input_type,
                bound_mode(state.mode),
                pattern_kind(syntax, is_binding, target.is_some(), $has_alternatives),
                children,
                direct_bindings,
            )
            .with_mutability(syntax.mut_keyword().is_some())
            .with_recovery(syntax.is_recovered())
            .with_target(target);

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

impl<C> Binder<'_, C>
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
        syntax: &impl SourceSyntaxNode,
        token: SyntaxToken,
        ordinal: &mut u32,
        state: &mut PatternBindingState,
    ) -> BindingResult<Option<LocalBindingSymbolId>> {
        if state.suppress_bindings
            || state.mode == PatternBindingMode::Assignment
            || token.is_missing()
        {
            return Ok(None);
        }

        let Some(text) = token.text(syntax.source().text()) else {
            return Ok(None);
        };

        let Some(name) = SymbolName::try_new(text) else {
            return Ok(None);
        };

        if let Some((_, binding)) = state
            .coherent
            .iter()
            .find(|(candidate, _)| candidate == &name)
        {
            return Ok(Some(*binding));
        }

        // The pending-name set and local symbol independently retain shared name text.
        if !state.pending_names.insert(name.clone()) {
            let span = bray_source::SourceSpan::new(syntax.source().source_id(), token.range());
            report_name_already_defined(self, name.as_str(), span);

            return Ok(None);
        }

        if !name_is_available(self, state.context, syntax.source(), &token) {
            return Ok(None);
        }

        let current = SymbolOrdinal::new(*ordinal);
        *ordinal = ordinal
            .checked_add(1)
            .ok_or(BindingError::IdentityCapacityExceeded)?;

        let anchor = SyntaxAnchor::from_node(syntax);

        let binding = self.unit_mut().push_binding(
            state.context.scope(),
            name,
            [anchor],
            Some(current),
            anchor.is_recovered() || token.is_missing(),
        )?;

        Ok(Some(binding))
    }

    fn push_coherent_bindings(
        &mut self,
        context: PathBindingContext,
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

        let mut bindings = Vec::new();

        for (name, anchors, is_recovered) in grouped {
            let Some(first_anchor) = anchors.first().copied() else {
                continue;
            };

            let span =
                bray_source::SourceSpan::new(first_anchor.source_id(), first_anchor.full_range());

            if !name_text_is_available(self, context, name.as_str(), span) {
                continue;
            }

            let ordinal = u32::try_from(bindings.len())
                .map(SymbolOrdinal::new)
                .map_err(|_| BindingError::IdentityCapacityExceeded)?;

            // The pending symbol and coherent-name lookup independently retain shared text.
            let lookup_name = name.clone();

            let binding = self.unit_mut().push_binding(
                context.scope(),
                name,
                anchors,
                Some(ordinal),
                is_recovered,
            )?;

            bindings.push((lookup_name, binding));
        }

        Ok(bindings)
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

    fn bind_pattern_target(
        &mut self,
        context: PathBindingContext,
        syntax: &impl PatternSyntax,
        mode: PatternBindingMode,
    ) -> BindingResult<Option<BoundPatternTarget>> {
        let result = match (syntax.first_path(), syntax.simple_binding_token()) {
            (Some(path), _) => match mode {
                PatternBindingMode::Assignment => self.bind_assignment_pattern_path(context, &path),
                PatternBindingMode::Declaration | PatternBindingMode::Match => {
                    self.bind_pattern_path(context, &path)
                }
            },
            (None, Some(token)) => Ok(self.bind_pattern_identifier(
                context,
                syntax.source(),
                token,
                mode == PatternBindingMode::Assignment,
            )),
            (None, None) => return Ok(None),
        }?;

        Ok(match result {
            bray_symbols::MemberLookupResult::Found(target) => Some(target),
            bray_symbols::MemberLookupResult::NotFound
            | bray_symbols::MemberLookupResult::WrongKind(_) => None,
            bray_symbols::MemberLookupResult::Ambiguous(_)
            | bray_symbols::MemberLookupResult::Inaccessible(_)
            | bray_symbols::MemberLookupResult::Malformed(_) => None,
        })
    }

    fn report_incoherent_alternative_pattern(&mut self, syntax: &impl SourceSyntaxNode) {
        let span = bray_source::SourceSpan::new(syntax.source().source_id(), syntax.full_range());

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(syntax.full_range().start().bytes()),
            DiagnosticKind::BindingIncoherentAlternativePattern,
            SeverityKind::Error,
        )
        .with_primary_span(span);

        self.add_diagnostic(diagnostic);
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
    has_target: bool,
    has_alternatives: bool,
) -> BoundPatternKind {
    if has_alternatives && syntax.has_alternative_separator() {
        BoundPatternKind::Alternative
    } else if is_binding {
        BoundPatternKind::Binding
    } else if has_target {
        BoundPatternKind::Path
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

// This private adapter unifies binder operations over existing typed syntax nodes.
// It does not define syntax tree structure, so it belongs here rather than in bray-syntax.
trait PatternSyntax: SourceSyntaxNode {
    fn coherent_binding_occurrences(&self) -> Vec<BindingOccurrence>;
    fn alternative_bindings_are_coherent(&self) -> bool;
    fn simple_binding_token(&self) -> Option<SyntaxToken>;
    fn first_path(&self) -> Option<bray_syntax::PathSyntax>;
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
    ($syntax:ty, $alternatives:expr, $occurrences:expr, $coherent:expr) => {
        impl PatternSyntax for $syntax {
            fn coherent_binding_occurrences(&self) -> Vec<BindingOccurrence> {
                ($occurrences)(self)
            }

            fn alternative_bindings_are_coherent(&self) -> bool {
                ($coherent)(self)
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

            fn first_path(&self) -> Option<bray_syntax::PathSyntax> {
                self.paths().next()
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

impl_pattern_syntax!(IrrefutablePatternSyntax, |_| false, |_| Vec::new(), |_| {
    true
});
impl_pattern_syntax!(
    CasePatternSyntax,
    |syntax: &CasePatternSyntax| syntax.alternative_separator_tokens().next().is_some(),
    coherent_case_binding_occurrences,
    case_alternatives_are_coherent
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

fn case_alternatives_are_coherent(syntax: &CasePatternSyntax) -> bool {
    let binding_sets = syntax
        .case_patterns()
        .map(|alternative| {
            let mut occurrences = Vec::new();
            collect_case_binding_occurrences(&alternative, &mut occurrences);

            occurrences
                .into_iter()
                .map(|occurrence| occurrence.name)
                .collect::<BTreeSet<_>>()
        })
        .collect::<Vec<_>>();

    let Some(first) = binding_sets.first() else {
        return true;
    };

    binding_sets.iter().all(|bindings| bindings == first)
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
    use bray_syntax::CasePatternSyntax;

    use crate::binder::PatternBindingMode;
    use crate::fact::test_support::TestFixture;

    #[test]
    fn case_patterns_retain_match_mode_and_alternative_shape() {
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "func main()\n",
            "{\n",
            "    match size\n",
            "    {\n",
            "        case left | left when true\n",
            "        {\n",
            "        }\n",
            "    }\n",
            "}",
        ));

        let facts = fixture.context();

        let (mut binder, block) = crate::binding::test_support::binder_and_block(&facts);

        let Some(pattern) =
            crate::binding::test_support::first_descendant::<CasePatternSyntax>(&block)
        else {
            panic!("test block must contain a case pattern");
        };

        assert_eq!(super::coherent_case_binding_occurrences(&pattern).len(), 2);

        let root = binder.unit().root_scope();
        let context = crate::binding::test_support::internal_path_context(binder.facts(), root);

        let bound = match binder.bind_case_pattern(
            context,
            &pattern,
            fixture.declared_type,
            PatternBindingMode::Match,
        ) {
            Ok(bound) => bound,
            Err(error) => panic!("case pattern must bind: {error:?}"),
        };

        let Some(pattern) = binder.unit_view().pattern(bound.pattern()) else {
            panic!("bound case pattern must be committed");
        };

        assert_eq!(pattern.mode(), bray_bound_tree::BoundPatternMode::Match);

        assert_eq!(
            pattern.kind(),
            bray_bound_tree::BoundPatternKind::Alternative
        );

        assert_eq!(pattern.children().len(), 2);
        assert_eq!(bound.bindings().len(), 1);

        let result = match binder.finish() {
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
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "func main()\n",
            "{\n",
            "    let value = 1;\n",
            "}",
        ));

        let facts = fixture.context();

        let (mut binder, block) = crate::binding::test_support::binder_and_block(&facts);

        let Some(declaration) = block
            .block_items()
            .find_map(|item| item.local_binding_declaration())
        else {
            panic!("test block must contain a local declaration");
        };

        let root = binder.unit().root_scope();

        let Some(name) = bray_symbols::SymbolName::try_new("value") else {
            panic!("assignment target name must be valid");
        };

        let existing = match binder.unit_mut().push_binding(
            root,
            name,
            [bray_declarations::SyntaxAnchor::from_node(&declaration)],
            Some(bray_symbols::SymbolOrdinal::new(0)),
            false,
        ) {
            Ok(binding) => binding,
            Err(error) => panic!("assignment target must build: {error:?}"),
        };

        if let Err(error) = binder.unit_mut().activate_local(root, existing) {
            panic!("assignment target must activate: {error:?}");
        }

        let context = crate::binding::test_support::internal_path_context(binder.facts(), root);

        let bound = match binder.bind_irrefutable_pattern(
            context,
            &declaration.irrefutable_pattern(),
            fixture.declared_type,
            PatternBindingMode::Assignment,
        ) {
            Ok(bound) => bound,
            Err(error) => panic!("assignment pattern must bind: {error:?}"),
        };

        let Some(pattern) = binder.unit_view().pattern(bound.pattern()) else {
            panic!("assignment pattern must be committed");
        };

        assert_eq!(
            pattern.mode(),
            bray_bound_tree::BoundPatternMode::Assignment
        );

        assert_eq!(
            pattern.target(),
            Some(bray_bound_tree::BoundPatternTarget::Local(existing.into()))
        );

        assert!(bound.bindings().is_empty());

        let result = match binder.finish() {
            Ok(result) => result,
            Err(error) => panic!("assignment pattern must freeze: {error:?}"),
        };

        assert_eq!(result.unit().local_symbols().bindings().len(), 1);
    }

    #[test]
    fn incoherent_alternatives_emit_recovery_without_partial_bindings() {
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "func main()\n",
            "{\n",
            "    match size\n",
            "    {\n",
            "        case left | right when true\n",
            "        {\n",
            "        }\n",
            "    }\n",
            "}",
        ));

        let facts = fixture.context();

        let (mut binder, block) = crate::binding::test_support::binder_and_block(&facts);

        let Some(pattern) =
            crate::binding::test_support::first_descendant::<CasePatternSyntax>(&block)
        else {
            panic!("test block must contain a case pattern");
        };

        let root = binder.unit().root_scope();
        let context = crate::binding::test_support::internal_path_context(binder.facts(), root);

        let bound = match binder.bind_case_pattern(
            context,
            &pattern,
            fixture.declared_type,
            PatternBindingMode::Match,
        ) {
            Ok(bound) => bound,
            Err(error) => panic!("incoherent pattern must recover: {error:?}"),
        };

        assert!(bound.bindings().is_empty());

        let result = match binder.finish() {
            Ok(result) => result,
            Err(error) => panic!("incoherent pattern recovery must freeze: {error:?}"),
        };

        assert_eq!(
            result
                .diagnostics()
                .iter()
                .map(bray_diagnostics::Diagnostic::kind)
                .collect::<Vec<_>>(),
            [bray_diagnostics::DiagnosticKind::BindingIncoherentAlternativePattern]
        );
    }

    #[test]
    fn bare_pattern_names_resolve_pattern_capable_declarations_before_binding() {
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "func main()\n",
            "{\n",
            "    match size\n",
            "    {\n",
            "        case size when true\n",
            "        {\n",
            "        }\n",
            "    }\n",
            "}",
        ));

        let facts = fixture.context();

        let (mut binder, block) = crate::binding::test_support::binder_and_block(&facts);

        let Some(pattern) =
            crate::binding::test_support::first_descendant::<CasePatternSyntax>(&block)
        else {
            panic!("test block must contain a case pattern");
        };

        let root = binder.unit().root_scope();
        let context = crate::binding::test_support::internal_path_context(binder.facts(), root);

        let bound = match binder.bind_case_pattern(
            context,
            &pattern,
            fixture.declared_type,
            PatternBindingMode::Match,
        ) {
            Ok(bound) => bound,
            Err(error) => panic!("constant pattern must bind: {error:?}"),
        };

        let Some(pattern) = binder.unit_view().pattern(bound.pattern()) else {
            panic!("constant pattern must be committed");
        };

        assert!(
            matches!(
                pattern.target(),
                Some(bray_bound_tree::BoundPatternTarget::Surface(_))
            ),
            "{pattern:?}"
        );

        assert!(bound.bindings().is_empty());
    }
}
