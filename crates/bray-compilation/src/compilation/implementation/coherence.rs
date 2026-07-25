use std::collections::{BTreeMap, BTreeSet};

use bray_binder::{BinderFactContext, NameAccess, bind_surface_path_with_re_exports};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};
use bray_source::SourceSpan;
use bray_symbols::{
    AnySymbolId, ImplementationCoherenceDomainKey, ImplementationOverloadSymbolId,
    ImplementationParticipationKind, ImplementationSymbolId, MemberLookupResult,
    NamedTraitImplementationSymbolId, SymbolOrigin,
};
use bray_syntax::{ImplementationOverloadDeclarationSyntax, PathSyntax};

use super::super::Compilation;
use super::index::{ImplementationFamilyKey, ImplementationFamilySubject};
use super::matching::implementation_headers_overlap;
use crate::compilation::binder::{CompilationBinderFacts, binder_fact_error};
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError};

impl Compilation {
    pub(in crate::compilation) fn implementation_coherence_diagnostics(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<&DiagnosticBag, FactQueryError> {
        self.query_fact_with_cancellation(
            CompilationFactKey::ImplementationCoherence,
            &self.state.implementation_coherence,
            cancellation,
            |cancellation| self.compute_implementation_coherence(cancellation),
        )
    }

    fn compute_implementation_coherence(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticBag, FactQueryError> {
        let domain = ImplementationCoherenceDomainKey::new(self.package_identity().clone());
        let participation =
            self.implementation_participation_with_cancellation(domain, cancellation)?;

        let index = self.implementation_header_index(cancellation)?;
        let values = self.semantic_value_store()?;
        let symbols = self.symbol_graph()?;
        let facts = self.binder_facts(cancellation)?;

        let mut diagnostics = participation.diagnostics().clone();
        let families = self.resolve_implementation_families(&facts, index.value(), cancellation)?;

        diagnostics.add_range(families.diagnostics.iter().cloned());

        let headers = index.value().headers();
        let participants = participation
            .value()
            .implementations()
            .iter()
            .map(|participant| (participant.implementation(), participant))
            .collect::<BTreeMap<_, _>>();

        let mut overlapping = BTreeSet::new();
        let mut by_trait = BTreeMap::new();

        for header in &headers {
            let application = values
                .trait_application_data(header.trait_application())
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            by_trait
                .entry(application.definition())
                .or_insert_with(Vec::new)
                .push(*header);
        }

        for trait_headers in by_trait.values() {
            for (index, left) in trait_headers.iter().enumerate() {
                for right in &trait_headers[index + 1..] {
                    cancellation.check()?;

                    if !implementation_headers_overlap(left, right, values)
                        .map_err(|_| FactQueryError::InfrastructureFailure)?
                    {
                        continue;
                    }

                    overlapping.extend([left.implementation(), right.implementation()]);

                    add_participant_diagnostic(
                        &mut diagnostics,
                        participants.get(&left.implementation()).copied(),
                        symbols,
                        DiagnosticKind::CheckingOverlappingImplementation,
                    );

                    add_participant_diagnostic(
                        &mut diagnostics,
                        participants.get(&right.implementation()).copied(),
                        symbols,
                        DiagnosticKind::CheckingOverlappingImplementation,
                    );
                }
            }
        }

        let mut by_family = BTreeMap::new();

        for header in headers {
            let Some(key) = header
                .family_key(values)
                .map_err(|_| FactQueryError::InfrastructureFailure)?
            else {
                continue;
            };

            by_family.entry(key).or_insert_with(Vec::new).push(header);
        }

        for family_headers in by_family.values().filter(|headers| headers.len() > 1) {
            if family_headers
                .iter()
                .any(|header| overlapping.contains(&header.implementation()))
            {
                continue;
            }

            let family = family_headers
                .first()
                .and_then(|header| families.single_family(header.implementation()));

            if family.is_some()
                && family_headers
                    .iter()
                    .all(|header| families.single_family(header.implementation()) == family)
            {
                continue;
            }

            for header in family_headers {
                add_participant_diagnostic(
                    &mut diagnostics,
                    participants.get(&header.implementation()).copied(),
                    symbols,
                    DiagnosticKind::CheckingUngroupedImplementationOverloads,
                );
            }
        }

        Ok(diagnostics)
    }

    fn resolve_implementation_families(
        &self,
        facts: &CompilationBinderFacts<'_>,
        index: &super::index::ImplementationHeaderIndex,
        cancellation: &CancellationToken,
    ) -> Result<ResolvedImplementationFamilies, FactQueryError> {
        let symbols = self.symbol_graph()?;
        let values = self.semantic_value_store()?;
        let headers = index
            .headers()
            .into_iter()
            .map(|header| (header.implementation(), header))
            .collect::<BTreeMap<_, _>>();

        let mut resolved = ResolvedImplementationFamilies::default();

        for family in symbols
            .implementation_overloads()
            .iter()
            .filter(|family| family.origin() == SymbolOrigin::Source)
        {
            cancellation.check()?;

            let declaration = family
                .syntax_anchor()
                .and_then(|anchor| {
                    anchor.find_descendant::<ImplementationOverloadDeclarationSyntax>(
                        self.syntax_tree(),
                    )
                })
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let module = symbols
                .containing_module(family.id().into())
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let expected =
                bind_family_header(facts, module.id(), &declaration, &mut resolved.diagnostics)?;

            let mut seen = BTreeSet::new();

            for arm_anchor in family.arm_syntax() {
                let path = arm_anchor
                    .find_descendant::<PathSyntax>(self.syntax_tree())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                let Some(implementation) =
                    bind_family_arm(facts, module.id(), &path, &mut resolved.diagnostics)?
                else {
                    continue;
                };

                let implementation = ImplementationSymbolId::from(implementation);

                resolved
                    .memberships
                    .entry(implementation)
                    .or_default()
                    .insert(family.id());

                if !seen.insert(implementation) {
                    resolved.diagnostics.add(source_diagnostic(
                        *arm_anchor,
                        DiagnosticKind::CheckingInvalidImplementationOverloadArm,
                    ));

                    continue;
                }

                let compatible = headers
                    .get(&implementation)
                    .and_then(|header| header.family_key(values).ok().flatten())
                    .is_some_and(|actual| Some(actual) == expected);

                if !compatible {
                    resolved.diagnostics.add(source_diagnostic(
                        *arm_anchor,
                        DiagnosticKind::CheckingInvalidImplementationOverloadArm,
                    ));
                }
            }
        }

        if let Some(imported) = facts.imported_symbols().map_err(binder_fact_error)? {
            for family in imported.implementation_overloads() {
                for arm in family.arms() {
                    if let AnySymbolId::NamedTraitImplementation(implementation) = arm {
                        resolved
                            .memberships
                            .entry(ImplementationSymbolId::from(*implementation))
                            .or_default()
                            .insert(family.id());
                    }
                }
            }
        }

        Ok(resolved)
    }
}

#[derive(Default)]
struct ResolvedImplementationFamilies {
    memberships: BTreeMap<ImplementationSymbolId, BTreeSet<ImplementationOverloadSymbolId>>,
    diagnostics: DiagnosticBag,
}

impl ResolvedImplementationFamilies {
    fn single_family(
        &self,
        implementation: ImplementationSymbolId,
    ) -> Option<ImplementationOverloadSymbolId> {
        let families = self.memberships.get(&implementation)?;

        if families.len() != 1 {
            return None;
        }

        families.first().copied()
    }
}

fn bind_family_header(
    facts: &CompilationBinderFacts<'_>,
    module: bray_symbols::ModuleSymbolId,
    declaration: &ImplementationOverloadDeclarationSyntax,
    diagnostics: &mut DiagnosticBag,
) -> Result<Option<ImplementationFamilyKey>, FactQueryError> {
    let subject = bind_surface_path(
        facts,
        module,
        &declaration.implementation_overload_subject().path(),
        diagnostics,
    )?;

    let trait_definition =
        bind_surface_path(facts, module, &declaration.trait_path(), diagnostics)?;

    let subject = match subject {
        Some(AnySymbolId::Struct(subject)) => {
            Some(ImplementationFamilySubject::Named(subject.into()))
        }
        Some(AnySymbolId::Union(subject)) => {
            Some(ImplementationFamilySubject::Named(subject.into()))
        }
        Some(_) => None,
        None => return Ok(None),
    };

    let Some(subject) = subject else {
        diagnostics.add(source_diagnostic(
            SyntaxAnchor::from_node(declaration),
            DiagnosticKind::CheckingInvalidImplementationOverloadHeader,
        ));

        return Ok(None);
    };

    let subject_syntax = declaration.implementation_overload_subject();
    let subject = if subject_syntax.ampersand_token().is_some() {
        let kind = if subject_syntax.mut_token().is_some() {
            bray_symbols::BorrowKind::Mutable
        } else {
            bray_symbols::BorrowKind::Shared
        };

        match subject {
            ImplementationFamilySubject::Named(subject) => {
                ImplementationFamilySubject::Borrowed(kind, subject)
            }
            ImplementationFamilySubject::Borrowed(_, _) => unreachable!(),
        }
    } else {
        subject
    };

    let Some(AnySymbolId::Trait(trait_definition)) = trait_definition else {
        if trait_definition.is_some() {
            diagnostics.add(source_diagnostic(
                SyntaxAnchor::from_node(declaration),
                DiagnosticKind::CheckingInvalidImplementationOverloadHeader,
            ));
        }

        return Ok(None);
    };

    Ok(Some(ImplementationFamilyKey::new(
        subject,
        trait_definition,
    )))
}

fn bind_family_arm(
    facts: &CompilationBinderFacts<'_>,
    module: bray_symbols::ModuleSymbolId,
    path: &PathSyntax,
    diagnostics: &mut DiagnosticBag,
) -> Result<Option<NamedTraitImplementationSymbolId>, FactQueryError> {
    let result = bind_surface_path(facts, module, path, diagnostics)?;

    let Some(result) = result else {
        return Ok(None);
    };

    let AnySymbolId::NamedTraitImplementation(implementation) = result else {
        diagnostics.add(source_diagnostic(
            SyntaxAnchor::from_node(path),
            DiagnosticKind::CheckingInvalidImplementationOverloadArm,
        ));

        return Ok(None);
    };

    Ok(Some(implementation))
}

fn bind_surface_path(
    facts: &CompilationBinderFacts<'_>,
    module: bray_symbols::ModuleSymbolId,
    path: &bray_syntax::PathSyntax,
    diagnostics: &mut DiagnosticBag,
) -> Result<Option<AnySymbolId>, FactQueryError> {
    let result = bind_surface_path_with_re_exports(
        facts,
        module,
        path,
        NameAccess::Internal,
        &mut |module, name, access| facts.module_re_export_lookup(module, name, access),
    )
    .map_err(binder_fact_error)?;

    diagnostics.add_range(result.diagnostics().iter().cloned());

    match result.value() {
        MemberLookupResult::Found(symbol) => Ok(Some(*symbol)),
        MemberLookupResult::NotFound
        | MemberLookupResult::WrongKind(_)
        | MemberLookupResult::Ambiguous(_)
        | MemberLookupResult::Inaccessible(_)
        | MemberLookupResult::Malformed(_) => Ok(None),
    }
}

fn add_participant_diagnostic(
    diagnostics: &mut DiagnosticBag,
    participant: Option<&bray_symbols::ParticipatingImplementation>,
    symbols: &bray_symbols::SymbolGraph,
    kind: DiagnosticKind,
) {
    let Some(participant) = participant else {
        return;
    };

    match participant.evidence().kind() {
        ImplementationParticipationKind::Declared => {
            if let Some(anchor) =
                symbols.declaration_syntax_anchor(participant.implementation().into_any())
            {
                diagnostics.add(source_diagnostic(anchor, kind));
            }
        }
        ImplementationParticipationKind::ExplicitUsing => {
            diagnostics.add_range(
                participant
                    .evidence()
                    .using_declarations()
                    .iter()
                    .copied()
                    .map(|anchor| source_diagnostic(anchor, kind)),
            );
        }
        ImplementationParticipationKind::CompilerKnown => {}
    }
}

fn source_diagnostic(anchor: SyntaxAnchor, kind: DiagnosticKind) -> Diagnostic {
    Diagnostic::new(
        DiagnosticId::new(anchor.full_range().start().bytes()),
        kind,
        SeverityKind::Error,
    )
    .with_primary_span(SourceSpan::new(anchor.source_id(), anchor.full_range()))
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;

    use crate::fact::CompilationFactKey;
    use crate::test_support::{compilation, diagnostic_kinds};

    const OVERLOAD_SURFACE: &str = concat!(
        "module app;\n",
        "\n",
        "trait Reader<Mode>\n",
        "{\n",
        "}\n",
        "\n",
        "struct Buffer<T>\n",
        "{\n",
        "}\n",
        "\n",
        "struct Bytes\n",
        "{\n",
        "}\n",
        "\n",
        "struct Lines\n",
        "{\n",
        "}\n",
    );

    #[test]
    fn disjoint_implementation_overload_arms_form_one_valid_family() {
        let compilation = compilation(&format!(
            "{OVERLOAD_SURFACE}{}",
            concat!(
                "\n",
                "impl BytesReader = Buffer<T>(Reader<Bytes>)\n",
                "{\n",
                "}\n",
                "\n",
                "impl LinesReader = Buffer<T>(Reader<Lines>)\n",
                "{\n",
                "}\n",
                "\n",
                "overload Buffer(Reader) =\n",
                "{\n",
                "    BytesReader,\n",
                "    LinesReader,\n",
                "}\n",
            )
        ));

        let diagnostics = compilation
            .implementation_coherence_diagnostics(&compilation.state.cancellation)
            .unwrap_or_else(|error| panic!("coherence validation must complete: {error:?}"));

        assert!(diagnostics.is_empty(), "{diagnostics:?}");

        assert!(
            compilation
                .state
                .fact_runtime
                .dependencies(&CompilationFactKey::ImplementationCoherence)
                .is_ok_and(|dependencies| dependencies.is_some())
        );
    }

    #[test]
    fn exact_and_generic_implementation_overlap_is_reported_at_each_declaration() {
        let exact = compilation(&format!(
            "{OVERLOAD_SURFACE}{}",
            concat!(
                "\n",
                "impl FirstReader = Buffer<Bytes>(Reader<Bytes>)\n",
                "{\n",
                "}\n",
                "\n",
                "impl SecondReader = Buffer<Bytes>(Reader<Bytes>)\n",
                "{\n",
                "}\n",
            )
        ));

        assert_eq!(
            diagnostic_kinds(exact.semantic_diagnostics())
                .into_iter()
                .filter(|kind| *kind == DiagnosticKind::CheckingOverlappingImplementation)
                .count(),
            2
        );

        let generic = compilation(&format!(
            "{OVERLOAD_SURFACE}{}",
            concat!(
                "\n",
                "impl FirstReader = Buffer<T>(Reader<T>)\n",
                "{\n",
                "}\n",
                "\n",
                "impl SecondReader = Buffer<U>(Reader<U>)\n",
                "{\n",
                "}\n",
            )
        ));

        assert_eq!(
            diagnostic_kinds(generic.semantic_diagnostics())
                .into_iter()
                .filter(|kind| *kind == DiagnosticKind::CheckingOverlappingImplementation)
                .count(),
            2
        );
    }

    #[test]
    fn disjoint_trait_applications_require_one_explicit_overload_family() {
        let compilation = compilation(&format!(
            "{OVERLOAD_SURFACE}{}",
            concat!(
                "\n",
                "impl BytesReader = Buffer<T>(Reader<Bytes>)\n",
                "{\n",
                "}\n",
                "\n",
                "impl LinesReader = Buffer<T>(Reader<Lines>)\n",
                "{\n",
                "}\n",
            )
        ));

        assert_eq!(
            diagnostic_kinds(compilation.semantic_diagnostics())
                .into_iter()
                .filter(|kind| {
                    *kind == DiagnosticKind::CheckingUngroupedImplementationOverloads
                })
                .count(),
            2
        );
    }

    #[test]
    fn overload_headers_and_arms_must_match_their_implementations() {
        let compilation = compilation(&format!(
            "{OVERLOAD_SURFACE}{}",
            concat!(
                "\n",
                "trait Writer<Mode>\n",
                "{\n",
                "}\n",
                "\n",
                "impl BytesWriter = Buffer<T>(Writer<Bytes>)\n",
                "{\n",
                "}\n",
                "\n",
                "overload Buffer(Reader) =\n",
                "{\n",
                "    BytesWriter,\n",
                "    BytesWriter,\n",
                "}\n",
                "\n",
                "overload Reader(Buffer) =\n",
                "{\n",
                "    BytesWriter,\n",
                "}\n",
            )
        ));

        let diagnostic_kinds = diagnostic_kinds(compilation.semantic_diagnostics());

        assert_eq!(
            diagnostic_kinds
                .iter()
                .filter(|kind| {
                    **kind == DiagnosticKind::CheckingInvalidImplementationOverloadHeader
                })
                .count(),
            1
        );

        assert_eq!(
            diagnostic_kinds
                .into_iter()
                .filter(|kind| {
                    *kind == DiagnosticKind::CheckingInvalidImplementationOverloadArm
                })
                .count(),
            3
        );
    }
}
