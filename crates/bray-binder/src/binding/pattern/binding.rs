use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    BoundPattern, BoundPatternEntry, BoundPatternEntryKind, BoundPatternId, BoundPatternTarget,
    BoundReferenceTarget,
};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{
    Diagnostic, DiagnosticId, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind, SeverityKind,
};
use bray_symbols::{LocalBindingSymbolId, LocalScopeId, SymbolName, SymbolOrdinal, TypeId};
use bray_syntax::{CasePatternSyntax, IrrefutablePatternSyntax, SourceSyntaxNode, SyntaxToken};

use super::syntax::{BindingOccurrence, PatternSyntax, bound_mode, pattern_kind};
use crate::BindingQueryContext;
use crate::binder::{Binder, PatternBindingMode};
use crate::binding::name::{
    name_is_available, name_text_is_available, report_name_already_defined, symbol_name,
};
use crate::binding::{BindingError, BindingResult};
use crate::lookup::PathBindingContext;

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct BoundPatternBinding {
    pattern: BoundPatternId,
    bindings: Box<[LocalBindingSymbolId]>,
    contextual_bindings: Box<[(LocalBindingSymbolId, BoundPatternId)]>,
}

impl BoundPatternBinding {
    pub(crate) const fn pattern(&self) -> BoundPatternId {
        self.pattern
    }

    pub(crate) fn bindings(&self) -> &[LocalBindingSymbolId] {
        &self.bindings
    }

    fn contextual_bindings(&self) -> &[(LocalBindingSymbolId, BoundPatternId)] {
        &self.contextual_bindings
    }
}

struct PatternBindingState {
    context: PathBindingContext,
    error_type: TypeId,
    mode: PatternBindingMode,
    coherent: Vec<(SymbolName, LocalBindingSymbolId)>,
    pending_names: BTreeMap<SymbolName, bray_source::SourceSpan>,
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
            error_type: TypeId,
            mode: PatternBindingMode,
        ) -> BindingResult<BoundPatternBinding, C::UpstreamError> {
            let (alternatives_are_coherent, occurrences) = if $has_alternatives {
                self.resolve_alternative_bindings(context, syntax, input_type, mode)?
            } else {
                (true, Vec::new())
            };

            if !alternatives_are_coherent {
                self.report_incoherent_alternative_pattern(syntax);
            }

            let coherent = self.push_coherent_bindings(context, occurrences, mode)?;

            // Pending identities and coherent lookup independently retain shared name text.
            let pending_names = coherent
                .iter()
                .map(|(name, binding)| {
                    let anchor = self.unit().local_symbol_syntax_anchor((*binding).into());
                    (
                        name.clone(),
                        bray_source::SourceSpan::new(anchor.source_id(), anchor.full_range()),
                    )
                })
                .collect::<BTreeMap<_, _>>();

            let mut state = PatternBindingState {
                context,
                error_type,
                mode,
                coherent,
                pending_names,
                suppress_bindings: !alternatives_are_coherent,
            };

            self.$inner(syntax, input_type, &mut state)
        }

        fn $inner(
            &mut self,
            syntax: &$syntax,
            input_type: TypeId,
            state: &mut PatternBindingState,
        ) -> BindingResult<BoundPatternBinding, C::UpstreamError> {
            self.check_cancellation()?;

            let mut children = Vec::new();
            let mut entries = Vec::new();
            let mut introduced = Vec::new();
            let mut contextual_bindings = Vec::new();
            let mut ordinal = 0_u32;

            let child_input_type = if syntax.has_alternative_separator() {
                input_type
            } else {
                state.error_type
            };

            for child in syntax.$children() {
                let bound = self.$inner(&child, child_input_type, state)?;

                children.push(bound.pattern());
                introduced.extend_from_slice(bound.bindings());
                contextual_bindings.extend_from_slice(bound.contextual_bindings());
            }

            for entry in syntax.$entries() {
                let nested = entry.$entry_children().collect::<Vec<_>>();

                let name = entry
                    .identifier_token()
                    .and_then(|token| symbol_name(entry.source(), &token));

                let mut nested_pattern = None;
                let mut shorthand_binding = None;

                for child in &nested {
                    let bound = self.$inner(child, state.error_type, state)?;

                    nested_pattern.get_or_insert(bound.pattern());
                    children.push(bound.pattern());
                    introduced.extend_from_slice(bound.bindings());
                    contextual_bindings.extend_from_slice(bound.contextual_bindings());
                }

                if nested.is_empty()
                    && entry.dot_dot_token().is_none()
                    && let Some(token) = entry.identifier_token()
                    && let Some(binding) =
                        self.push_pattern_binding(&entry, token, &mut ordinal, state)?
                {
                    shorthand_binding = Some(binding);
                    introduced.push(binding);
                }

                let kind = if let Some(pattern) = nested_pattern {
                    BoundPatternEntryKind::Pattern(pattern)
                } else if let Some(binding) = shorthand_binding {
                    BoundPatternEntryKind::Binding(binding)
                } else if entry.dot_dot_token().is_some() {
                    BoundPatternEntryKind::Remaining
                } else {
                    BoundPatternEntryKind::Recovered
                };

                entries.push(BoundPatternEntry::new(name, kind));
            }

            let binding_token = syntax.simple_binding_token();

            let (target, name_can_bind, is_contextual_name) =
                self.bind_pattern_target(state.context, syntax, input_type, state.mode)?;

            let is_binding = binding_token.is_some()
                && name_can_bind
                && state.mode != PatternBindingMode::Assignment;

            let direct_binding = if is_binding {
                match binding_token {
                    Some(token) => self.push_pattern_binding(syntax, token, &mut ordinal, state)?,
                    None => None,
                }
            } else {
                None
            };

            if let Some(binding) = direct_binding {
                self.record_value_type(BoundReferenceTarget::Local(binding.into()), input_type);
            }

            let direct_bindings = direct_binding.into_iter().collect::<Vec<_>>();

            introduced.extend(direct_bindings.iter().copied());

            let mut unique = BTreeSet::new();

            introduced.retain(|binding| unique.insert(*binding));

            let pattern = BoundPattern::new(
                self.pattern_origin(syntax),
                input_type,
                bound_mode(state.mode),
                pattern_kind(syntax, is_binding, target, $has_alternatives),
                children,
                direct_bindings,
            )
            .with_entries(entries)
            .with_mutability(syntax.mut_keyword().is_some())
            .with_recovery(syntax.is_recovered())
            .with_target(target)
            .with_name(syntax.pattern_name())
            .with_literal(syntax.pattern_literal())
            .with_contextual_name(is_contextual_name);

            let pattern = self
                .unit_mut()
                .tree_mut()
                .push_pattern(pattern)
                .map_err(crate::unit::BoundUnitConstructionError::from)?;

            if is_contextual_name && let Some(binding) = direct_binding {
                contextual_bindings.push((binding, pattern));
            }

            Ok(BoundPatternBinding {
                pattern,
                bindings: introduced.into_boxed_slice(),
                contextual_bindings: contextual_bindings.into_boxed_slice(),
            })
        }
    };
}

impl<C> Binder<'_, C>
where
    C: BindingQueryContext + ?Sized,
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
    ) -> BindingResult<Option<LocalBindingSymbolId>, C::UpstreamError> {
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
        let span = bray_source::SourceSpan::new(syntax.source().source_id(), token.range());

        if let Some(prior) = state.pending_names.get(&name).copied() {
            report_name_already_defined(self, name.as_str(), span, [prior]);

            return Ok(None);
        }

        state.pending_names.insert(name.clone(), span);

        if !name_is_available(self, state.context, syntax.source(), &token)? {
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
    ) -> BindingResult<Vec<(SymbolName, LocalBindingSymbolId)>, C::UpstreamError> {
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

            if !name_text_is_available(self, context, name.as_str(), span)? {
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

    fn resolve_alternative_bindings(
        &self,
        context: PathBindingContext,
        syntax: &impl PatternSyntax,
        input_type: TypeId,
        mode: PatternBindingMode,
    ) -> BindingResult<(bool, Vec<BindingOccurrence>), C::UpstreamError> {
        let alternatives = syntax.alternative_binding_occurrences();

        if alternatives.is_empty() {
            return Ok((true, Vec::new()));
        }

        let mut resolved = Vec::with_capacity(alternatives.len());

        for alternative in alternatives {
            let mut bindings = Vec::new();

            for occurrence in alternative {
                let result = self.lookup_pattern_identifier(
                    context,
                    occurrence.name.as_str(),
                    mode == PatternBindingMode::Assignment,
                    input_type,
                )?;

                if matches!(
                    result,
                    bray_symbols::MemberLookupResult::NotFound
                        | bray_symbols::MemberLookupResult::WrongKind(_)
                ) {
                    bindings.push(occurrence);
                }
            }

            resolved.push(bindings);
        }

        if self.type_is_error(input_type) {
            // Subject-dependent names remain provisional until the checker resolves variants.
            return Ok((true, resolved.into_iter().flatten().collect()));
        }

        let Some(first) = resolved.first() else {
            return Ok((true, Vec::new()));
        };

        let first_names = first
            .iter()
            .map(|occurrence| &occurrence.name)
            .collect::<BTreeSet<_>>();

        let is_coherent = resolved.iter().skip(1).all(|bindings| {
            bindings
                .iter()
                .map(|occurrence| &occurrence.name)
                .collect::<BTreeSet<_>>()
                == first_names
        });

        let occurrences = if is_coherent {
            resolved.into_iter().flatten().collect()
        } else {
            Vec::new()
        };

        Ok((is_coherent, occurrences))
    }

    pub(crate) fn activate_pattern_bindings(
        &mut self,
        scope: LocalScopeId,
        pattern: &BoundPatternBinding,
    ) -> BindingResult<(), C::UpstreamError> {
        for binding in pattern.bindings() {
            let contextual = pattern
                .contextual_bindings()
                .iter()
                .find(|(candidate, _)| candidate == binding)
                .copied();

            if let Some((_, owner)) = contextual {
                let Some(name) = self
                    .unit_view()
                    .pattern(owner)
                    .and_then(BoundPattern::name)
                    .cloned()
                else {
                    continue;
                };

                self.record_contextual_pattern_binding(scope, name, *binding, owner);
            } else {
                self.unit_mut().activate_local(scope, *binding)?;
            }
        }

        Ok(())
    }

    fn bind_pattern_target(
        &mut self,
        context: PathBindingContext,
        syntax: &impl PatternSyntax,
        input_type: TypeId,
        mode: PatternBindingMode,
    ) -> BindingResult<(Option<BoundPatternTarget>, bool, bool), C::UpstreamError> {
        let is_contextual_name = syntax.bare_name_token().is_some()
            && matches!(
                mode,
                PatternBindingMode::MatchObserve | PatternBindingMode::MatchConsume
            )
            && self.type_is_error(input_type);

        let result = if let Some(token) = syntax.bare_name_token() {
            self.bind_pattern_identifier(
                context,
                syntax.source(),
                token,
                mode == PatternBindingMode::Assignment,
                input_type,
            )?
        } else {
            match (syntax.first_path(), syntax.simple_binding_token()) {
                (Some(path), _) => match mode {
                    PatternBindingMode::Assignment => {
                        self.bind_assignment_pattern_path(context, &path)?
                    }
                    PatternBindingMode::Declaration
                    | PatternBindingMode::MatchObserve
                    | PatternBindingMode::MatchConsume => self.bind_pattern_path(context, &path)?,
                },
                (None, Some(token)) => self.bind_pattern_identifier(
                    context,
                    syntax.source(),
                    token,
                    mode == PatternBindingMode::Assignment,
                    input_type,
                )?,
                (None, None) => return Ok((None, false, false)),
            }
        };

        Ok(match result {
            bray_symbols::MemberLookupResult::Found(target) => {
                (Some(target), false, is_contextual_name)
            }
            bray_symbols::MemberLookupResult::NotFound
            | bray_symbols::MemberLookupResult::WrongKind(_) => (None, true, is_contextual_name),
            bray_symbols::MemberLookupResult::Ambiguous(_)
            | bray_symbols::MemberLookupResult::Inaccessible(_)
            | bray_symbols::MemberLookupResult::Malformed(_) => (None, false, false),
        })
    }

    fn type_is_error(&self, ty: TypeId) -> bool {
        matches!(
            self.binding_context()
                .semantic_values()
                .type_data(ty)
                .as_ref(),
            bray_symbols::TypeData::Error
        )
    }

    fn report_incoherent_alternative_pattern(&mut self, syntax: &impl SourceSyntaxNode) {
        let span = bray_source::SourceSpan::new(syntax.source().source_id(), syntax.full_range());

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(syntax.full_range().start().bytes()),
            DiagnosticKind::BindingIncoherentAlternativePattern,
            SeverityKind::Error,
        )
        .with_primary_span(span)
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::AlternativePattern,
            span,
        ));

        self.add_diagnostic(diagnostic);
    }
}

#[cfg(test)]
mod tests {
    use bray_syntax::CasePatternSyntax;

    use crate::binder::PatternBindingMode;
    use crate::query::test_support::TestFixture;

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

        let binding_context = fixture.context();

        let (mut binder, block) = crate::binding::test_support::binder_and_block(&binding_context);

        let Some(pattern) =
            crate::binding::test_support::first_descendant::<CasePatternSyntax>(&block)
        else {
            panic!("test block must contain a case pattern");
        };

        assert_eq!(
            super::super::syntax::case_alternative_binding_occurrences(&pattern)
                .into_iter()
                .flatten()
                .count(),
            2
        );

        let root = binder.unit().root_scope();

        let context =
            crate::binding::test_support::internal_path_context(binder.binding_context(), root);

        let bound = match binder.bind_case_pattern(
            context,
            &pattern,
            fixture.declared_type,
            fixture.declared_type,
            PatternBindingMode::MatchObserve,
        ) {
            Ok(bound) => bound,
            Err(error) => panic!("case pattern must bind: {error:?}"),
        };

        let Some(pattern) = binder.unit_view().pattern(bound.pattern()) else {
            panic!("bound case pattern must be committed");
        };

        assert_eq!(
            pattern.mode(),
            bray_bound_tree::BoundPatternMode::MatchObserve
        );

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

        let binding_context = fixture.context();

        let (mut binder, block) = crate::binding::test_support::binder_and_block(&binding_context);

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

        let context =
            crate::binding::test_support::internal_path_context(binder.binding_context(), root);

        let bound = match binder.bind_irrefutable_pattern(
            context,
            &declaration.irrefutable_pattern(),
            fixture.declared_type,
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

        let binding_context = fixture.context();

        let (mut binder, block) = crate::binding::test_support::binder_and_block(&binding_context);

        let Some(pattern) =
            crate::binding::test_support::first_descendant::<CasePatternSyntax>(&block)
        else {
            panic!("test block must contain a case pattern");
        };

        let root = binder.unit().root_scope();

        let context =
            crate::binding::test_support::internal_path_context(binder.binding_context(), root);

        let subject_type = fixture
            .semantic_values
            .intern_type(bray_symbols::TypeData::tuple([]))
            .unwrap();

        let bound = match binder.bind_case_pattern(
            context,
            &pattern,
            subject_type,
            fixture.declared_type,
            PatternBindingMode::MatchObserve,
        ) {
            Ok(bound) => bound,
            Err(error) => panic!("incoherent pattern must recover: {error:?}"),
        };

        assert!(bound.bindings().is_empty());

        let result = match binder.finish() {
            Ok(result) => result,
            Err(error) => panic!("incoherent pattern recovery must freeze: {error:?}"),
        };

        bray_testing::assert_goal_state_diagnostic_kind(
            result.diagnostics(),
            bray_diagnostics::DiagnosticKind::BindingIncoherentAlternativePattern,
        );

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

        let binding_context = fixture.context();

        let (mut binder, block) = crate::binding::test_support::binder_and_block(&binding_context);

        let Some(pattern) =
            crate::binding::test_support::first_descendant::<CasePatternSyntax>(&block)
        else {
            panic!("test block must contain a case pattern");
        };

        let root = binder.unit().root_scope();

        let context =
            crate::binding::test_support::internal_path_context(binder.binding_context(), root);

        let bound = match binder.bind_case_pattern(
            context,
            &pattern,
            fixture.declared_type,
            fixture.declared_type,
            PatternBindingMode::MatchObserve,
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
