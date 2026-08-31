use std::collections::{BTreeMap, BTreeSet};

use bray_binder::NameAccess;
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticImplementationBorrowKind,
    DiagnosticImplementationFamily, DiagnosticImplementationFamilySubject,
    DiagnosticImplementationOverloadProblem, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticRelatedLocation, DiagnosticRelatedLocationKind, DiagnosticType, SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    AnySymbolId, ImplementationCoherenceDomainKey, ImplementationOverloadSymbolId,
    ImplementationParticipationKind, ImplementationSymbolId, ImportedSymbolSkeleton,
    MemberLookupResult, NamedTraitImplementationSymbolId, SymbolGraph, SymbolOrigin,
    diagnostic_symbol_kind,
};
use bray_syntax::{ImplementationOverloadDeclarationSyntax, PathSyntax};

use super::super::Compilation;
use super::index::{ImplementationFamilyKey, ImplementationFamilySubject};
use crate::compilation::binder::{CompilationBindingContext, binding_query_error};
use crate::compilation::diagnostics::{source_diagnostic, symbol_diagnostic_identity};
use crate::compilation::limits::try_count_comparison;
use crate::compilation::overlap::{
    implementation_headers_overlap, implementation_subjects_overlap,
};
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError};

impl Compilation {
    pub(in crate::compilation) fn implementation_coherence_diagnostics(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<&DiagnosticBag, FactQueryError> {
        self.query_with_cancellation(
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
        // The domain key independently retains the Arc-backed package identity.
        let domain = ImplementationCoherenceDomainKey::new(self.package_identity().clone());

        let participation =
            self.implementation_participation_with_cancellation(domain, cancellation)?;

        let index = self.implementation_header_index(cancellation)?;
        let values = self.semantic_value_store()?;
        let symbols = self.symbol_graph()?;
        let binding_context = self.binding_context(cancellation)?;

        let imported = binding_context
            .imported_symbols()
            .map_err(binding_query_error)?;

        // The published coherence result owns its merged diagnostic bag.
        let mut diagnostics = participation.diagnostics().clone();

        let families = self.resolve_implementation_families(
            &binding_context,
            imported,
            index.value(),
            cancellation,
        )?;

        diagnostics.add_range(families.diagnostics.iter().cloned());

        let headers = index.value().headers();

        let participants = participation
            .value()
            .implementations()
            .iter()
            .map(|participant| (participant.implementation(), participant))
            .collect::<BTreeMap<_, _>>();

        let mut requires_family = Vec::new();
        let mut by_trait = BTreeMap::new();

        let maximum_comparisons = self.semantic_pairwise_limit();

        let mut comparisons = 0;

        for header in &headers {
            if !self.generic_declaration_may_be_satisfied(header.generic(), cancellation)? {
                continue;
            }

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

                    let left_participant = participants.get(&left.implementation()).copied();
                    let right_participant = participants.get(&right.implementation()).copied();

                    if [left_participant, right_participant]
                        .into_iter()
                        .all(|participant| {
                            participant.is_some_and(|participant| {
                                participant.evidence().kind()
                                    == ImplementationParticipationKind::CompilerKnown
                            })
                        })
                    {
                        continue;
                    }

                    if !try_count_comparison(&mut comparisons, maximum_comparisons) {
                        let attempted = comparisons
                            .checked_add(1)
                            .ok_or(FactQueryError::InfrastructureFailure)?;

                        diagnostics.add(coherence_limit_diagnostic(
                            left_participant,
                            right_participant,
                            symbols,
                            attempted,
                            maximum_comparisons,
                        ));

                        return Ok(diagnostics);
                    }

                    if implementation_headers_overlap(left, right, values)
                        .map_err(|_| FactQueryError::InfrastructureFailure)?
                    {
                        let left_context = self.implementation_diagnostic_context(
                            left,
                            symbols,
                            imported,
                            cancellation,
                        )?;

                        let right_context = self.implementation_diagnostic_context(
                            right,
                            symbols,
                            imported,
                            cancellation,
                        )?;

                        add_participant_diagnostic(
                            &mut diagnostics,
                            left_participant,
                            symbols,
                            DiagnosticKind::CheckingOverlappingImplementation,
                            &left_context,
                            right_participant.and_then(|participant| {
                                participant_source_anchor(participant, symbols)
                            }),
                        );

                        add_participant_diagnostic(
                            &mut diagnostics,
                            right_participant,
                            symbols,
                            DiagnosticKind::CheckingOverlappingImplementation,
                            &right_context,
                            left_participant.and_then(|participant| {
                                participant_source_anchor(participant, symbols)
                            }),
                        );

                        continue;
                    }

                    if implementation_subjects_overlap(left, right, values)
                        .map_err(|_| FactQueryError::InfrastructureFailure)?
                    {
                        requires_family.push((*left, *right));
                    }
                }
            }
        }

        for (left, right) in requires_family {
            let family = families.single_family(left.implementation());

            if family.is_some() && families.single_family(right.implementation()) == family {
                continue;
            }

            let left_participant = participants.get(&left.implementation()).copied();
            let right_participant = participants.get(&right.implementation()).copied();

            let left_context =
                self.implementation_diagnostic_context(left, symbols, imported, cancellation)?;

            let right_context =
                self.implementation_diagnostic_context(right, symbols, imported, cancellation)?;

            add_participant_diagnostic(
                &mut diagnostics,
                left_participant,
                symbols,
                DiagnosticKind::CheckingUngroupedImplementationOverloads,
                &left_context,
                right_participant
                    .and_then(|participant| participant_source_anchor(participant, symbols)),
            );

            add_participant_diagnostic(
                &mut diagnostics,
                right_participant,
                symbols,
                DiagnosticKind::CheckingUngroupedImplementationOverloads,
                &right_context,
                left_participant
                    .and_then(|participant| participant_source_anchor(participant, symbols)),
            );
        }

        Ok(diagnostics)
    }

    fn implementation_diagnostic_context(
        &self,
        header: &super::index::ImplementationHeader,
        symbols: &SymbolGraph,
        imported: Option<&ImportedSymbolSkeleton>,
        cancellation: &CancellationToken,
    ) -> Result<ImplementationDiagnosticContext, FactQueryError> {
        let checker = self.checker_context(cancellation)?;

        let subject = bray_checker::diagnostic_type(&checker, header.subject())
            .map_err(FactQueryError::from)?;

        let application = self
            .semantic_value_store()?
            .trait_application_data(header.trait_application())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let trait_definition =
            symbol_diagnostic_identity(symbols, imported, application.definition().into())?;

        Ok(ImplementationDiagnosticContext {
            subject,
            trait_definition,
        })
    }

    fn resolve_implementation_families(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        imported: Option<&ImportedSymbolSkeleton>,
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

            let expected = bind_family_header(
                binding_context,
                symbols,
                imported,
                module.id(),
                &declaration,
                &mut resolved.diagnostics,
            )?;

            let mut seen = BTreeMap::<ImplementationSymbolId, Vec<SyntaxAnchor>>::new();

            for arm_anchor in family.arm_syntax() {
                let path = arm_anchor
                    .find_descendant::<PathSyntax>(self.syntax_tree())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                let Some(implementation) = bind_family_arm(
                    binding_context,
                    symbols,
                    imported,
                    module.id(),
                    &path,
                    &mut resolved.diagnostics,
                )?
                else {
                    continue;
                };

                let implementation = ImplementationSymbolId::from(implementation);

                resolved
                    .memberships
                    .entry(implementation)
                    .or_default()
                    .insert(family.id());

                let identity =
                    symbol_diagnostic_identity(symbols, imported, implementation.into_any())?;

                if let Some(previous) = seen.get_mut(&implementation) {
                    resolved.diagnostics.add(implementation_overload_diagnostic(
                        *arm_anchor,
                        DiagnosticKind::CheckingDuplicateImplementationOverloadArm,
                        DiagnosticImplementationOverloadProblem::DuplicateArm {
                            implementation: identity,
                        },
                        previous,
                    ));

                    previous.push(*arm_anchor);

                    continue;
                }

                seen.insert(implementation, vec![*arm_anchor]);

                let Some(expected) = expected else {
                    continue;
                };

                let header = headers
                    .get(&implementation)
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                let actual = header
                    .family_key(values)
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                let Some(actual) = actual else {
                    resolved.diagnostics.add(implementation_overload_diagnostic(
                        *arm_anchor,
                        DiagnosticKind::CheckingInvalidImplementationOverloadArm,
                        DiagnosticImplementationOverloadProblem::ArmSubjectNotFamilyCompatible {
                            implementation: identity,
                        },
                        &[],
                    ));

                    continue;
                };

                if actual != expected {
                    resolved.diagnostics.add(implementation_overload_diagnostic(
                        *arm_anchor,
                        DiagnosticKind::CheckingInvalidImplementationOverloadArm,
                        DiagnosticImplementationOverloadProblem::FamilyMismatch {
                            implementation: identity,
                            required: diagnostic_implementation_family(
                                symbols, imported, expected,
                            )?,
                            provided: diagnostic_implementation_family(symbols, imported, actual)?,
                        },
                        &[],
                    ));
                }
            }
        }

        if let Some(imported) = imported {
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

struct ImplementationDiagnosticContext {
    subject: DiagnosticType,
    trait_definition: bray_diagnostics::DiagnosticInterfaceSymbolIdentity,
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
    binding_context: &CompilationBindingContext<'_>,
    symbols: &SymbolGraph,
    imported: Option<&ImportedSymbolSkeleton>,
    module: bray_symbols::ModuleSymbolId,
    declaration: &ImplementationOverloadDeclarationSyntax,
    diagnostics: &mut DiagnosticBag,
) -> Result<Option<ImplementationFamilyKey>, FactQueryError> {
    let subject = bind_surface_path(
        binding_context,
        module,
        &declaration.implementation_overload_subject().path(),
        diagnostics,
    )?;

    let trait_definition = bind_surface_path(
        binding_context,
        module,
        &declaration.trait_path(),
        diagnostics,
    )?;

    let subject = match subject {
        Some(AnySymbolId::Struct(subject)) => {
            Some(ImplementationFamilySubject::Named(subject.into()))
        }
        Some(AnySymbolId::Union(subject)) => {
            Some(ImplementationFamilySubject::Named(subject.into()))
        }
        Some(symbol) => {
            diagnostics.add(implementation_overload_diagnostic(
                SyntaxAnchor::from_node(&declaration.implementation_overload_subject().path()),
                DiagnosticKind::CheckingInvalidImplementationOverloadHeader,
                DiagnosticImplementationOverloadProblem::HeaderSubjectKind {
                    subject: symbol_diagnostic_identity(symbols, imported, symbol)?,
                    actual: diagnostic_symbol_kind(symbol.kind()),
                },
                &[],
            ));

            None
        }
        None => return Ok(None),
    };

    let Some(subject) = subject else {
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

    let trait_definition = match trait_definition {
        Some(AnySymbolId::Trait(trait_definition)) => trait_definition,
        Some(symbol) => {
            diagnostics.add(implementation_overload_diagnostic(
                SyntaxAnchor::from_node(&declaration.trait_path()),
                DiagnosticKind::CheckingInvalidImplementationOverloadHeader,
                DiagnosticImplementationOverloadProblem::HeaderTraitKind {
                    trait_definition: symbol_diagnostic_identity(symbols, imported, symbol)?,
                    actual: diagnostic_symbol_kind(symbol.kind()),
                },
                &[],
            ));

            return Ok(None);
        }
        None => return Ok(None),
    };

    Ok(Some(ImplementationFamilyKey::new(
        subject,
        trait_definition,
    )))
}

fn bind_family_arm(
    binding_context: &CompilationBindingContext<'_>,
    symbols: &SymbolGraph,
    imported: Option<&ImportedSymbolSkeleton>,
    module: bray_symbols::ModuleSymbolId,
    path: &PathSyntax,
    diagnostics: &mut DiagnosticBag,
) -> Result<Option<NamedTraitImplementationSymbolId>, FactQueryError> {
    let result = bind_surface_path(binding_context, module, path, diagnostics)?;

    let Some(result) = result else {
        return Ok(None);
    };

    let AnySymbolId::NamedTraitImplementation(implementation) = result else {
        diagnostics.add(implementation_overload_diagnostic(
            SyntaxAnchor::from_node(path),
            DiagnosticKind::CheckingInvalidImplementationOverloadArm,
            DiagnosticImplementationOverloadProblem::ArmSymbolKind {
                implementation: symbol_diagnostic_identity(symbols, imported, result)?,
                actual: diagnostic_symbol_kind(result.kind()),
            },
            &[],
        ));

        return Ok(None);
    };

    Ok(Some(implementation))
}

fn implementation_overload_diagnostic(
    anchor: SyntaxAnchor,
    kind: DiagnosticKind,
    problem: DiagnosticImplementationOverloadProblem,
    previous: &[SyntaxAnchor],
) -> Diagnostic {
    let span = SourceSpan::new(anchor.source_id(), anchor.full_range());

    let label = if kind == DiagnosticKind::CheckingDuplicateImplementationOverloadArm {
        DiagnosticLabelKind::DuplicateOverloadArm
    } else {
        DiagnosticLabelKind::InvalidOverload
    };

    previous.iter().copied().fold(
        source_diagnostic(anchor, kind)
            .with_label(DiagnosticLabel::primary(label, span))
            .with_arg(DiagnosticArg::implementation_overload_problem(problem)),
        |diagnostic, previous| {
            diagnostic.with_related_location(DiagnosticRelatedLocation::new(
                DiagnosticRelatedLocationKind::FirstDeclaration,
                SourceSpan::new(previous.source_id(), previous.full_range()),
            ))
        },
    )
}

fn diagnostic_implementation_family(
    symbols: &SymbolGraph,
    imported: Option<&ImportedSymbolSkeleton>,
    family: ImplementationFamilyKey,
) -> Result<DiagnosticImplementationFamily, FactQueryError> {
    let subject = match family.subject() {
        ImplementationFamilySubject::Named(subject) => {
            DiagnosticImplementationFamilySubject::Named(symbol_diagnostic_identity(
                symbols,
                imported,
                named_type_symbol(subject),
            )?)
        }
        ImplementationFamilySubject::Borrowed(kind, subject) => {
            DiagnosticImplementationFamilySubject::Borrowed {
                kind: match kind {
                    bray_symbols::BorrowKind::Shared => DiagnosticImplementationBorrowKind::Shared,
                    bray_symbols::BorrowKind::Mutable => {
                        DiagnosticImplementationBorrowKind::Mutable
                    }
                },
                subject: symbol_diagnostic_identity(symbols, imported, named_type_symbol(subject))?,
            }
        }
    };

    Ok(DiagnosticImplementationFamily::new(
        subject,
        symbol_diagnostic_identity(symbols, imported, family.trait_definition().into())?,
    ))
}

const fn named_type_symbol(subject: bray_symbols::NamedTypeSymbolId) -> AnySymbolId {
    match subject {
        bray_symbols::NamedTypeSymbolId::Struct(subject) => AnySymbolId::Struct(subject),
        bray_symbols::NamedTypeSymbolId::Union(subject) => AnySymbolId::Union(subject),
    }
}

fn bind_surface_path(
    binding_context: &CompilationBindingContext<'_>,
    module: bray_symbols::ModuleSymbolId,
    path: &bray_syntax::PathSyntax,
    diagnostics: &mut DiagnosticBag,
) -> Result<Option<AnySymbolId>, FactQueryError> {
    let result = binding_context
        .bind_surface_path(module, path, NameAccess::Internal)
        .map_err(binding_query_error)?;

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
    context: &ImplementationDiagnosticContext,
    related: Option<SyntaxAnchor>,
) {
    let Some(participant) = participant else {
        return;
    };

    match participant.evidence().kind() {
        ImplementationParticipationKind::Declared => {
            if let Some(anchor) =
                symbols.declaration_syntax_anchor(participant.implementation().into_any())
            {
                diagnostics.add(participant_diagnostic(anchor, kind, context, related));
            }
        }
        ImplementationParticipationKind::ExplicitUsing => {
            diagnostics.add_range(
                participant
                    .evidence()
                    .using_declarations()
                    .iter()
                    .copied()
                    .map(|anchor| participant_diagnostic(anchor, kind, context, related)),
            );
        }
        ImplementationParticipationKind::CompilerKnown => {}
    }
}

fn participant_diagnostic(
    anchor: SyntaxAnchor,
    kind: DiagnosticKind,
    context: &ImplementationDiagnosticContext,
    related: Option<SyntaxAnchor>,
) -> Diagnostic {
    let label = match kind {
        DiagnosticKind::CheckingOverlappingImplementation => {
            DiagnosticLabelKind::OverlappingImplementation
        }
        DiagnosticKind::CheckingUngroupedImplementationOverloads => {
            DiagnosticLabelKind::UngroupedImplementationOverload
        }
        _ => unreachable!("participant diagnostics must describe implementation conflicts"),
    };

    let span = SourceSpan::new(anchor.source_id(), anchor.full_range());

    let diagnostic = source_diagnostic(anchor, kind)
        .with_label(DiagnosticLabel::primary(label, span))
        .with_arg(DiagnosticArg::actual_type(context.subject.clone()))
        .with_arg(DiagnosticArg::interface_symbol_identity(
            context.trait_definition.clone(),
        ));

    let Some(related) = related else {
        return diagnostic;
    };

    diagnostic.with_related_location(DiagnosticRelatedLocation::new(
        DiagnosticRelatedLocationKind::ConflictingDeclaration,
        SourceSpan::new(related.source_id(), related.full_range()),
    ))
}

fn coherence_limit_diagnostic(
    left: Option<&bray_symbols::ParticipatingImplementation>,
    right: Option<&bray_symbols::ParticipatingImplementation>,
    symbols: &bray_symbols::SymbolGraph,
    actual: u64,
    maximum: u64,
) -> Diagnostic {
    let kind = DiagnosticKind::CheckingImplementationCoherenceLimitExceeded;

    let diagnostic = [left, right]
        .into_iter()
        .flatten()
        .find_map(|participant| participant_source_anchor(participant, symbols))
        .map_or_else(
            || Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error),
            |anchor| {
                let span = SourceSpan::new(anchor.source_id(), anchor.full_range());

                source_diagnostic(anchor, kind).with_label(DiagnosticLabel::primary(
                    DiagnosticLabelKind::ImplementationCoherenceLimitExceeded,
                    span,
                ))
            },
        );

    diagnostic
        .with_arg(DiagnosticArg::actual_count(actual))
        .with_arg(DiagnosticArg::maximum_count(maximum))
}

fn participant_source_anchor(
    participant: &bray_symbols::ParticipatingImplementation,
    symbols: &bray_symbols::SymbolGraph,
) -> Option<SyntaxAnchor> {
    match participant.evidence().kind() {
        ImplementationParticipationKind::Declared => {
            symbols.declaration_syntax_anchor(participant.implementation().into_any())
        }
        ImplementationParticipationKind::ExplicitUsing => {
            participant.evidence().using_declarations().first().copied()
        }
        ImplementationParticipationKind::CompilerKnown => None,
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;

    use crate::fact::CompilationFactKey;
    use crate::test_support::{compilation, compilation_with_options, diagnostic_kinds};
    use crate::{CompilationOptions, SelectedTarget, SemanticAnalysisLimits, WorkerBudget};

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
        "struct Pair<First, Second>\n",
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

        bray_testing::assert_goal_state_diagnostic_kind(
            exact.semantic_diagnostics(),
            DiagnosticKind::CheckingOverlappingImplementation,
        );

        for diagnostic in exact.semantic_diagnostics().iter().filter(|diagnostic| {
            diagnostic.kind() == DiagnosticKind::CheckingOverlappingImplementation
        }) {
            bray_testing::assert_goal_state_diagnostic(diagnostic);
        }

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

        for diagnostic in generic.semantic_diagnostics().iter().filter(|diagnostic| {
            diagnostic.kind() == DiagnosticKind::CheckingOverlappingImplementation
        }) {
            bray_testing::assert_goal_state_diagnostic(diagnostic);
        }
    }

    #[test]
    fn implementation_coherence_stops_at_the_comparison_limit() {
        let source = format!(
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
        );

        let options = CompilationOptions::new(
            WorkerBudget::serial(),
            bray_symbols::ProductKind::Library,
            SelectedTarget::baseline(),
        )
        .with_semantic_analysis_limits(SemanticAnalysisLimits::new(256, 0));

        let compilation = compilation_with_options(&source, options);

        let diagnostics = compilation
            .implementation_coherence_diagnostics(&compilation.state.cancellation)
            .unwrap_or_else(|error| panic!("limited coherence validation must finish: {error:?}"));

        assert_eq!(
            diagnostic_kinds(diagnostics),
            [DiagnosticKind::CheckingImplementationCoherenceLimitExceeded]
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            diagnostics,
            DiagnosticKind::CheckingImplementationCoherenceLimitExceeded,
        );
    }

    #[test]
    fn constraints_and_repeated_parameters_can_prove_implementations_disjoint() {
        let compilation = compilation(&format!(
            "{OVERLOAD_SURFACE}{}",
            concat!(
                "\n",
                "impl Repeated = Pair<T, T>(Reader<Bytes>)\n",
                "{\n",
                "}\n",
                "\n",
                "impl Different = Pair<Bytes, Lines>(Reader<Bytes>)\n",
                "{\n",
                "}\n",
                "\n",
                "impl Impossible = Buffer<T>(Reader<Bytes>)\n",
                "    with(false)\n",
                "{\n",
                "}\n",
                "\n",
                "impl Concrete = Buffer<Bytes>(Reader<Bytes>)\n",
                "{\n",
                "}\n",
            )
        ));

        let diagnostic_kinds = diagnostic_kinds(compilation.semantic_diagnostics());

        assert!(!diagnostic_kinds.contains(&DiagnosticKind::CheckingOverlappingImplementation));

        assert!(
            !diagnostic_kinds.contains(&DiagnosticKind::CheckingUngroupedImplementationOverloads)
        );
    }

    #[test]
    fn disjoint_subject_patterns_do_not_require_an_overload_family() {
        let compilation = compilation(&format!(
            "{OVERLOAD_SURFACE}{}",
            concat!(
                "\n",
                "impl BytesReader = Buffer<Bytes>(Reader<Bytes>)\n",
                "{\n",
                "}\n",
                "\n",
                "impl LinesReader = Buffer<Lines>(Reader<Lines>)\n",
                "{\n",
                "}\n",
            )
        ));

        let diagnostic_kinds = diagnostic_kinds(compilation.semantic_diagnostics());

        assert!(
            !diagnostic_kinds.contains(&DiagnosticKind::CheckingUngroupedImplementationOverloads)
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

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.semantic_diagnostics(),
            DiagnosticKind::CheckingUngroupedImplementationOverloads,
        );
    }

    #[test]
    fn implementation_overload_header_requires_a_structural_subject() {
        let compilation = compilation(&format!(
            "{OVERLOAD_SURFACE}{}",
            concat!("\n", "overload Reader(Buffer) =\n", "{\n", "}\n",)
        ));

        let diagnostics = compilation
            .implementation_coherence_diagnostics(&compilation.state.cancellation)
            .unwrap_or_else(|error| panic!("coherence validation must complete: {error:?}"));

        assert_eq!(
            diagnostic_kinds(diagnostics),
            [DiagnosticKind::CheckingInvalidImplementationOverloadHeader]
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            diagnostics,
            DiagnosticKind::CheckingInvalidImplementationOverloadHeader,
        );
    }

    #[test]
    fn implementation_overload_arm_reports_the_exact_family_mismatch() {
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
                "}\n",
            )
        ));

        let diagnostics = compilation
            .implementation_coherence_diagnostics(&compilation.state.cancellation)
            .unwrap_or_else(|error| panic!("coherence validation must complete: {error:?}"));

        assert_eq!(
            diagnostic_kinds(diagnostics),
            [DiagnosticKind::CheckingInvalidImplementationOverloadArm]
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            diagnostics,
            DiagnosticKind::CheckingInvalidImplementationOverloadArm,
        );
    }

    #[test]
    fn implementation_overload_repeated_arm_retains_every_prior_origin() {
        let compilation = compilation(&format!(
            "{OVERLOAD_SURFACE}{}",
            concat!(
                "\n",
                "impl BytesReader = Buffer<T>(Reader<Bytes>)\n",
                "{\n",
                "}\n",
                "\n",
                "overload Buffer(Reader) =\n",
                "{\n",
                "    BytesReader,\n",
                "    BytesReader,\n",
                "    BytesReader,\n",
                "}\n",
            )
        ));

        let diagnostics = compilation
            .implementation_coherence_diagnostics(&compilation.state.cancellation)
            .unwrap_or_else(|error| panic!("coherence validation must complete: {error:?}"));

        let duplicates = diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.kind() == DiagnosticKind::CheckingDuplicateImplementationOverloadArm
            })
            .collect::<Vec<_>>();

        assert_eq!(duplicates.len(), 2);
        assert_eq!(duplicates[0].related_locations().len(), 1);
        assert_eq!(duplicates[1].related_locations().len(), 2);

        bray_testing::assert_goal_state_diagnostic_kind(
            diagnostics,
            DiagnosticKind::CheckingDuplicateImplementationOverloadArm,
        );
    }

    #[test]
    fn implementation_overload_arm_requires_a_named_trait_implementation() {
        let compilation = compilation(&format!(
            "{OVERLOAD_SURFACE}{}",
            concat!(
                "\n",
                "overload Buffer(Reader) =\n",
                "{\n",
                "    Bytes,\n",
                "}\n",
            )
        ));

        let diagnostics = compilation
            .implementation_coherence_diagnostics(&compilation.state.cancellation)
            .unwrap_or_else(|error| panic!("coherence validation must complete: {error:?}"));

        assert_eq!(
            diagnostic_kinds(diagnostics),
            [DiagnosticKind::CheckingInvalidImplementationOverloadArm]
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            diagnostics,
            DiagnosticKind::CheckingInvalidImplementationOverloadArm,
        );
    }
}
