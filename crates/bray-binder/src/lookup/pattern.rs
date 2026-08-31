use std::collections::BTreeSet;

use bray_bound_tree::BoundPatternTarget;
use bray_diagnostics::DiagnosticNameKind;
use bray_source::SourceSnapshot;
use bray_symbols::{MemberLookupResult, NamedTypeSymbolId, TypeData, TypeId};
use bray_syntax::SyntaxToken;

use super::binding::{NameLookupResult, lookup_surface_name, lookup_unqualified_name};
use super::category::ResolvedName;
use super::diagnostic::{malformed_lookup, report_lookup_result};
use super::path::{PathBindingContext, classify_pattern_target, token_reference};
use crate::{BindingQueryContext, BindingQueryResult, binder::Binder};

impl<C> Binder<'_, C>
where
    C: BindingQueryContext + ?Sized,
{
    pub(crate) fn bind_pattern_identifier(
        &mut self,
        context: PathBindingContext,
        source: &SourceSnapshot,
        token: SyntaxToken,
        assignment: bool,
        input_type: TypeId,
    ) -> BindingQueryResult<NameLookupResult<BoundPatternTarget>, C::UpstreamError> {
        let Some(reference) = token_reference(source, token) else {
            return Ok(malformed_lookup());
        };

        let result =
            self.lookup_pattern_identifier(context, reference.text(), assignment, input_type)?;

        if assignment
            || matches!(
                result,
                MemberLookupResult::Ambiguous(_)
                    | MemberLookupResult::Inaccessible(_)
                    | MemberLookupResult::Malformed(_)
            )
        {
            report_lookup_result(
                self,
                &reference,
                if assignment {
                    DiagnosticNameKind::Value
                } else {
                    DiagnosticNameKind::Pattern
                },
                &result,
            );
        }

        Ok(result)
    }

    pub(crate) fn lookup_pattern_identifier(
        &self,
        context: PathBindingContext,
        name: &str,
        assignment: bool,
        input_type: TypeId,
    ) -> BindingQueryResult<NameLookupResult<BoundPatternTarget>, C::UpstreamError> {
        let lookup = lookup_unqualified_name(
            self.unit(),
            self.binding_context().symbols(),
            context.scope(),
            context.module(),
            name,
            context.access(),
        );

        if assignment {
            return Ok(lookup.map(
                |name| match name {
                    ResolvedName::Local(id) => BoundPatternTarget::Local(id),
                    ResolvedName::Surface(id) => BoundPatternTarget::Surface(id),
                },
                |name| name,
            ));
        }

        let lexical = lookup.classify(classify_pattern_target);
        let subject = self.lookup_subject_pattern_name(context, input_type, name)?;

        Ok(combine_pattern_lookups(lexical, subject))
    }

    fn lookup_subject_pattern_name(
        &self,
        context: PathBindingContext,
        input_type: TypeId,
        name: &str,
    ) -> BindingQueryResult<NameLookupResult<BoundPatternTarget>, C::UpstreamError> {
        let input = self
            .binding_context()
            .semantic_values()
            .type_data(input_type)
            .map_err(|_| crate::BindingQueryError::DependencyUnavailable)?;

        let TypeData::Named {
            definition: NamedTypeSymbolId::Union(union),
            ..
        } = input.as_ref()
        else {
            return Ok(MemberLookupResult::NotFound);
        };

        Ok(lookup_surface_name(
            self.binding_context().symbols(),
            (*union).into(),
            name,
            context.access(),
        )
        .classify(classify_pattern_target))
    }
}

fn combine_pattern_lookups(
    first: NameLookupResult<BoundPatternTarget>,
    second: NameLookupResult<BoundPatternTarget>,
) -> NameLookupResult<BoundPatternTarget> {
    let mut found = Vec::new();
    let mut ambiguous = BTreeSet::new();
    let mut wrong_kind = BTreeSet::new();
    let mut inaccessible = BTreeSet::new();
    let mut malformed = BTreeSet::new();
    let mut has_malformed = false;

    for lookup in [first, second] {
        match lookup {
            MemberLookupResult::Found(target) => {
                if !found.iter().any(|(candidate, _)| *candidate == target) {
                    found.push((target, pattern_target_name(target)));
                }
            }
            MemberLookupResult::NotFound => {}
            MemberLookupResult::WrongKind(candidates) => {
                wrong_kind.extend(candidates);
            }
            MemberLookupResult::Ambiguous(candidates) => {
                ambiguous.extend(candidates);
            }
            MemberLookupResult::Inaccessible(candidates) => {
                inaccessible.extend(candidates);
            }
            MemberLookupResult::Malformed(candidates) => {
                has_malformed = true;
                malformed.extend(candidates);
            }
        }
    }

    let accessible = found
        .iter()
        .map(|(_, candidate)| *candidate)
        .chain(ambiguous.iter().copied())
        .chain(malformed.iter().copied())
        .collect::<BTreeSet<_>>();

    if has_malformed {
        return MemberLookupResult::Malformed(accessible.into_iter().collect());
    }

    if !ambiguous.is_empty() || found.len() > 1 {
        return MemberLookupResult::Ambiguous(accessible.into_iter().collect());
    }

    if let Some((target, _)) = found.first() {
        return MemberLookupResult::Found(*target);
    }

    if !inaccessible.is_empty() {
        return MemberLookupResult::Inaccessible(inaccessible.into_iter().collect());
    }

    if !wrong_kind.is_empty() {
        return MemberLookupResult::WrongKind(wrong_kind.into_iter().collect());
    }

    MemberLookupResult::NotFound
}

const fn pattern_target_name(target: BoundPatternTarget) -> ResolvedName {
    match target {
        BoundPatternTarget::Local(local) => ResolvedName::Local(local),
        BoundPatternTarget::Surface(surface) => ResolvedName::Surface(surface),
    }
}
