use std::collections::{BTreeMap, BTreeSet};

use bray_binder::SymbolQueryProvider;
use bray_bound_tree::{BoundCallableTarget, BoundReferenceTarget};
use bray_checker::{ExecutionObligation, ExecutionProperty, execution_condition_term};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, CallableDefinitionId, CallableExecutionContract, CallableExecutionDomain,
    CallableExecutionEvidence, CallableExecutionObligation, CallableExecutionOrigin,
    CallableExecutionTarget, CallableSignatureQuery, CallableSymbolId,
    ResolvedCallableExecutionContract, SymbolOrdinal, SymbolQueryRequest,
};

use crate::compilation::{
    Compilation, SemanticDataKind, SemanticQueryContext, SemanticQueryFailure,
    SemanticQueryViolation,
};
use crate::fact::{CancellationToken, FactQueryError};

type PortableObligation = CallableExecutionObligation<SymbolOrdinal>;
type SourceProofs = BTreeMap<
    (bray_declarations::SyntaxAnchor, ExecutionObligation),
    BTreeSet<(BoundCallableTarget, ExecutionObligation)>,
>;

impl Compilation {
    pub(in crate::compilation) fn export_execution_contracts(
        &self,
        selected: &BTreeSet<AnySymbolId>,
        cancellation: &CancellationToken,
    ) -> Result<
        DiagnosticResult<BTreeMap<AnySymbolId, ResolvedCallableExecutionContract>>,
        FactQueryError,
    > {
        let graph = self.symbol_graph()?;
        let mut diagnostics = DiagnosticBag::new();

        let (proofs, origins, source_symbols) =
            self.collect_execution_proofs(selected, &mut diagnostics, cancellation)?;

        let mut prepared = BTreeMap::new();
        let mut mappings = BTreeMap::new();

        for (anchor, symbol) in &source_symbols {
            let demanded = proofs
                .keys()
                .filter_map(|(owner, obligation)| (*owner == *anchor).then_some(*obligation))
                .collect::<BTreeSet<_>>();

            if demanded.is_empty() {
                continue;
            }

            let (domains, mapping, inferred, domain_diagnostics) =
                self.portable_execution_domains(*symbol, &demanded, cancellation)?;

            diagnostics.add_range(domain_diagnostics);

            prepared.insert(
                *symbol,
                CallableExecutionContract {
                    domains: domains.into(),
                    evidence: Default::default(),
                },
            );

            mappings.insert(*anchor, (mapping, inferred));
        }

        let values = self.semantic_value_store()?;

        for (anchor, symbol) in source_symbols {
            let Some(contract) = prepared.get_mut(&symbol) else {
                continue;
            };

            let (mapping, inferred) = mappings.get(&anchor).unwrap_or_else(|| {
                panic!("exported execution contract {symbol:?} has no obligation mapping")
            });

            let mut evidence = Vec::new();

            for ((owner, obligation), dependencies) in &proofs {
                if *owner != anchor {
                    continue;
                }

                let portable = mapping
                    .get(obligation)
                    .unwrap_or_else(|| panic!("checked execution obligation {obligation:?} of {symbol:?} has no portable mapping"));

                let mut targets = Vec::new();

                for (target, required) in dependencies {
                    match target {
                        BoundCallableTarget::Declaration(callable) => {
                            let target_anchor =
                                graph.declaration_syntax_anchor(callable.definition().symbol());

                            let required = target_anchor
                                .and_then(|anchor| mappings.get(&anchor))
                                .and_then(|(mapping, _)| mapping.get(required))
                                .copied()
                                .or_else(|| {
                                    super::imported::portable_imported_obligation(*required)
                                });

                            let required = required.unwrap_or_else(|| panic!("checked dependency of {symbol:?} on {callable:?} has no portable obligation"));

                            let callable = values
                                .intern_callable_instance(*callable)
                                .map_err(FactQueryError::SemanticValueStore)?;

                            targets.push((CallableExecutionTarget::Callable(callable), required));
                        }
                        BoundCallableTarget::Indirect(ty) => targets.push((
                            CallableExecutionTarget::Indirect(*ty),
                            PortableObligation::Property(ExecutionProperty::Pure, None),
                        )),
                        _ => panic!(
                            "checked execution proof of {symbol:?} contains unsupported target {target:?}"
                        ),
                    }
                }

                evidence.push(CallableExecutionEvidence::new(
                    origins
                        .get(&anchor)
                        .copied()
                        .unwrap_or(CallableExecutionOrigin::CheckedBody),
                    *portable,
                    targets,
                ));
            }

            for (total, post) in inferred {
                let proof = evidence.iter().find(|proof| proof.obligation == *total)
                    .unwrap_or_else(|| panic!("inferred completion {post:?} of {symbol:?} lacks its checked total proof {total:?}"));

                evidence.push(CallableExecutionEvidence {
                    origin: proof.origin,
                    obligation: *post,
                    dependencies: std::sync::Arc::clone(&proof.dependencies),
                });
            }

            evidence.sort_by_key(|proof| proof.obligation);
            contract.evidence = evidence.into();
        }

        Ok(DiagnosticResult::new(prepared, diagnostics))
    }

    fn collect_execution_proofs(
        &self,
        selected: &BTreeSet<AnySymbolId>,
        diagnostics: &mut DiagnosticBag,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            SourceProofs,
            BTreeMap<bray_declarations::SyntaxAnchor, CallableExecutionOrigin>,
            BTreeMap<bray_declarations::SyntaxAnchor, AnySymbolId>,
        ),
        FactQueryError,
    > {
        let graph = self.symbol_graph()?;

        let mut proofs = BTreeMap::<_, BTreeSet<_>>::new();
        let mut origins = BTreeMap::new();
        let mut source_symbols = BTreeMap::new();

        for symbol in selected.iter().copied() {
            let Some(definition) = CallableDefinitionId::try_new(symbol) else {
                continue;
            };

            let Some(anchor) = graph.declaration_syntax_anchor(symbol) else {
                continue;
            };

            source_symbols.insert(anchor, symbol);

            let declared = self.execution_declaration(anchor)?;

            diagnostics.add_range(declared.diagnostics().iter().cloned());

            if declared.value().clauses().is_empty() {
                continue;
            }

            if let Some(body) = self.callable_body_key(definition)? {
                let certificate = self.certified_execution_with_cancellation(body, cancellation)?;

                diagnostics.add_range(certificate.result().diagnostics().iter().cloned());

                if certificate.result().diagnostics().has_errors() {
                    continue;
                }

                let certificate = certificate.result().value();

                let obligations = certificate
                    .properties
                    .iter()
                    .map(|property| ExecutionObligation::Property(*property, None))
                    .chain(
                        certificate
                            .guarded_properties
                            .iter()
                            .map(|(source, property)| {
                                ExecutionObligation::Property(*property, Some((*source).into()))
                            }),
                    )
                    .chain(
                        certificate
                            .postconditions
                            .iter()
                            .map(|source| ExecutionObligation::Postcondition((*source).into())),
                    );

                for obligation in obligations {
                    proofs.entry((anchor, obligation)).or_default();
                }

                for (owner, obligation, target, required) in &certificate.dependencies {
                    proofs
                        .entry((*owner, *obligation))
                        .or_default()
                        .insert((*target, *required));

                    if let BoundCallableTarget::Declaration(callable) = target
                        && let Some(target_anchor) =
                            graph.declaration_syntax_anchor(callable.definition().symbol())
                    {
                        proofs.entry((target_anchor, *required)).or_default();
                    }
                }
            } else {
                let origin =
                    if matches!(graph.containing_symbol(symbol), Some(AnySymbolId::Trait(_))) {
                        CallableExecutionOrigin::Requirement
                    } else if self.intrinsic_projection_declaration(
                        symbol,
                        declared.value(),
                        cancellation,
                    )? {
                        CallableExecutionOrigin::CompilerIntrinsic
                    } else if let CallableSymbolId::Function(function) =
                        definition.callable_symbol()
                    {
                        let foreign = self
                            .foreign_callable_contract_with_cancellation(function, cancellation)?;

                        diagnostics.add_range(foreign.diagnostics().iter().cloned());

                        if foreign.value().as_ref().is_none_or(|contract| {
                            contract.direction() != bray_symbols::ForeignCallableDirection::Import
                        }) || foreign.diagnostics().has_errors()
                        {
                            continue;
                        }

                        CallableExecutionOrigin::ForeignAssertion
                    } else {
                        continue;
                    };

                origins.insert(anchor, origin);

                for domain in declared.value().domains() {
                    for property in &domain.properties {
                        proofs
                            .entry((
                                anchor,
                                ExecutionObligation::Property(
                                    property.property,
                                    (!domain.guards.is_empty()).then_some(property.source.into()),
                                ),
                            ))
                            .or_default();
                    }

                    for post in &domain.postconditions {
                        proofs
                            .entry((
                                anchor,
                                ExecutionObligation::Postcondition(
                                    bray_source::SourceSpan::new(
                                        post.source_id(),
                                        post.full_range(),
                                    )
                                    .into(),
                                ),
                            ))
                            .or_default();
                    }
                }
            }
        }

        Ok((proofs, origins, source_symbols))
    }

    fn portable_execution_domains(
        &self,
        symbol: AnySymbolId,
        demanded: &BTreeSet<ExecutionObligation>,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            Vec<CallableExecutionDomain<bray_symbols::ConstantTermId>>,
            BTreeMap<ExecutionObligation, PortableObligation>,
            Vec<(PortableObligation, PortableObligation)>,
            DiagnosticBag,
        ),
        FactQueryError,
    > {
        let Some(owner) = self.execution_contract_owner(symbol)? else {
            return Ok((
                Vec::new(),
                BTreeMap::new(),
                Vec::new(),
                DiagnosticBag::new(),
            ));
        };

        let declaration = self.execution_declaration(owner.source().syntax())?;

        let intrinsic =
            self.intrinsic_projection_declaration(symbol, declaration.value(), cancellation)?;

        let inputs = self.execution_callable_inputs(symbol, cancellation)?;
        let boolean = self.target_property_type(bray_target::TargetPropertyKind::ScalarBool)?;
        let values = self.semantic_value_store()?;
        let mut diagnostics = DiagnosticBag::new();

        diagnostics.add_range(declaration.diagnostics().iter().cloned());

        let candidates = if matches!(symbol, AnySymbolId::Finalizer(_))
            && demanded.iter().any(|obligation| {
                matches!(
                    obligation,
                    ExecutionObligation::Property(ExecutionProperty::Total, _)
                )
            }) {
            let definition =
                CallableDefinitionId::try_new(symbol).expect("finalizer is a callable definition");

            self.callable_body_key(definition)?
                .map(|body| self.execution_candidates_with_cancellation(body, cancellation))
                .transpose()?
        } else {
            None
        };

        let mut inferred = Vec::new();
        let mut domains = Vec::new();
        let mut mapping = BTreeMap::new();
        let mut post_index = 0usize;

        for (index, domain) in declaration.value().domains().iter().enumerate() {
            let ordinal = evidence_ordinal(symbol, index)?;

            let anchors = declaration
                .value()
                .requirements()
                .iter()
                // Intrinsic validity is checked by memory semantics and remains in the callable
                // contract. Its pure/total certificate adds no separate value-entry predicate.
                .filter(|_| !intrinsic)
                .chain(&domain.guards)
                .copied()
                .collect::<Vec<_>>();

            let entry = self.execution_condition_inputs(&owner, &anchors, cancellation)?;

            let posts =
                self.execution_condition_inputs(&owner, &domain.postconditions, cancellation)?;

            diagnostics.add_range(entry.diagnostics().iter().cloned());
            diagnostics.add_range(posts.diagnostics().iter().cloned());

            let mut entry_terms = Vec::new();
            let mut post_terms = Vec::new();
            let mut properties = BTreeSet::new();

            for (condition, source) in entry.value() {
                if let Some(term) = execution_condition_term(values, condition, &inputs, boolean)
                    .map_err(FactQueryError::SemanticValueStore)?
                {
                    entry_terms.push(term);
                } else {
                    diagnostics.add(super::proof::guarantee_diagnostic(
                        ExecutionObligation::Postcondition((*source).into()),
                        *source,
                        *source,
                        false,
                    ));
                }
            }

            for property in &domain.properties {
                let obligation = ExecutionObligation::Property(
                    property.property,
                    (!domain.guards.is_empty()).then_some(property.source.into()),
                );

                properties.insert(property.property);

                mapping.insert(
                    obligation,
                    PortableObligation::Property(property.property, Some(ordinal)),
                );
            }

            if domain.guards.is_empty() {
                for obligation in demanded {
                    if let ExecutionObligation::Property(property, None) = obligation {
                        properties.insert(*property);

                        mapping.insert(
                            *obligation,
                            PortableObligation::Property(*property, Some(ordinal)),
                        );
                    }
                }
            }

            let mut clause_terms = BTreeMap::<_, bray_symbols::ConstantTermId>::new();

            for (condition, source) in posts.value() {
                let Some(term) = execution_condition_term(values, condition, &inputs, boolean)
                    .map_err(FactQueryError::SemanticValueStore)?
                else {
                    diagnostics.add(super::proof::guarantee_diagnostic(
                        ExecutionObligation::Postcondition((*source).into()),
                        *source,
                        *source,
                        false,
                    ));

                    continue;
                };

                let term = if let Some(previous) = clause_terms.get(source) {
                    values
                        .intern_constant_term(bray_symbols::ConstantTermData::Binary {
                            operation: bray_symbols::ConstantBinaryOperation::LogicalAnd,
                            left: *previous,
                            right: term,
                        })
                        .map_err(FactQueryError::SemanticValueStore)?
                } else {
                    term
                };

                clause_terms.insert(*source, term);
            }

            for (source, term) in clause_terms {
                let post = evidence_ordinal(symbol, post_index)?;

                // The ordinal conversion bounds the counter below usize capacity on supported hosts.
                post_index += 1;
                post_terms.push((post, term));

                mapping.insert(
                    ExecutionObligation::Postcondition(source.into()),
                    PortableObligation::Postcondition(post),
                );
            }

            let total = PortableObligation::Property(ExecutionProperty::Total, Some(ordinal));

            let variant = candidates.as_ref().and_then(|candidates| {
                demanded.iter().find_map(|source| {
                    (mapping.get(source) == Some(&total))
                        .then(|| candidates.result().value().get(source))
                        .flatten()
                        .and_then(bray_checker::ExecutionCandidate::result_variant)
                })
            });

            if let Some(variant) = variant {
                let condition = bray_checker::ExecutionCondition::VariantTest(
                    variant,
                    std::sync::Arc::new(bray_checker::ExecutionCondition::Result),
                );

                let term = execution_condition_term(values, &condition, &inputs, boolean)
                    .map_err(FactQueryError::SemanticValueStore)?
                    .expect("a checked result variant is a portable completion condition");

                if !post_terms.iter().any(|(_, existing)| *existing == term) {
                    let post = evidence_ordinal(symbol, post_index)?;

                    post_index += 1;
                    post_terms.push((post, term));
                    inferred.push((total, PortableObligation::Postcondition(post)));
                }
            }

            domains.push(CallableExecutionDomain {
                ordinal,
                entry: entry_terms.into(),
                properties: properties.into_iter().collect(),
                postconditions: post_terms.into(),
            });
        }

        Ok((domains, mapping, inferred, diagnostics))
    }

    pub(in crate::compilation) fn execution_callable_inputs(
        &self,
        symbol: AnySymbolId,
        cancellation: &CancellationToken,
    ) -> Result<Vec<BoundReferenceTarget>, FactQueryError> {
        if let Some(predicate) = bray_symbols::PredicateDefinitionSymbolId::try_from_any(symbol) {
            let context = self.binding_context(cancellation)?;

            let signature = context
                .resolve_symbol_query(SymbolQueryRequest::<
                    bray_symbols::PredicateSignatureTemplateQuery,
                >::new(predicate))
                .map_err(crate::compilation::binder::binding_query_error)?;

            return Ok(signature
                .value()
                .parameters()
                .iter()
                .map(|parameter| BoundReferenceTarget::Surface(parameter.parameter().into()))
                .collect());
        }

        let Some(callable) = CallableSymbolId::try_from_any(symbol) else {
            return Ok(Vec::new());
        };

        let context = self.binding_context(cancellation)?;

        let signature = context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(callable))
            .map_err(crate::compilation::binder::binding_query_error)?;

        Ok(signature
            .value()
            .receiver()
            .map(|receiver| BoundReferenceTarget::Surface(receiver.parameter().into()))
            .into_iter()
            .chain(
                signature
                    .value()
                    .parameters()
                    .iter()
                    .map(|parameter| BoundReferenceTarget::Surface((*parameter).into())),
            )
            .collect())
    }
}

fn evidence_ordinal(symbol: AnySymbolId, index: usize) -> Result<SymbolOrdinal, FactQueryError> {
    u32::try_from(index).map(SymbolOrdinal::new).map_err(|_| {
        SemanticQueryFailure::contract(
            SemanticQueryContext::Symbol(symbol),
            SemanticQueryViolation::CapacityExceeded {
                data: SemanticDataKind::ExecutionEvidence,
                value: index,
            },
        )
        .into()
    })
}
