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
    BTreeSet<(bray_checker::ExecutionDependencyTarget, ExecutionObligation)>,
>;

type CollectedExecutionProofs = (
    SourceProofs,
    BTreeMap<bray_declarations::SyntaxAnchor, CallableExecutionOrigin>,
    BTreeMap<bray_declarations::SyntaxAnchor, AnySymbolId>,
);
type PortableExecutionContract = (
    Vec<CallableExecutionDomain<bray_symbols::ConstantTermId>>,
    BTreeMap<ExecutionObligation, PortableObligation>,
    Vec<(PortableObligation, PortableObligation)>,
    DiagnosticBag,
);

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

            let mut mapping = mapping;

            for internal in [
                ExecutionObligation::Observation,
                ExecutionObligation::Termination,
                ExecutionObligation::FiniteIteration,
            ] {
                if demanded.contains(&internal) {
                    mapping.insert(
                        internal,
                        match internal {
                            ExecutionObligation::Observation => PortableObligation::Observation,
                            ExecutionObligation::Termination => PortableObligation::Termination,
                            ExecutionObligation::FiniteIteration => {
                                PortableObligation::FiniteIteration
                            }
                            _ => unreachable!("internal proof role is selected above"),
                        },
                    );
                }
            }

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
                        bray_checker::ExecutionDependencyTarget::Callable(
                            BoundCallableTarget::Declaration(callable),
                        ) => {
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
                        bray_checker::ExecutionDependencyTarget::Callable(
                            BoundCallableTarget::Indirect(ty),
                        ) => targets.push((
                            CallableExecutionTarget::Indirect(*ty),
                            PortableObligation::Property(ExecutionProperty::Pure, None),
                        )),
                        bray_checker::ExecutionDependencyTarget::Requirement {
                            callable,
                            dispatch,
                        } => {
                            let binding = self.binding_context(cancellation)?;

                            let requirement =
                                crate::compilation::implementation::dispatch_requirement(
                                    &binding, *dispatch,
                                )
                                .map_err(FactQueryError::from)?;

                            let required = super::imported::portable_imported_obligation(*required)
                                .expect("selected implementation premise retains an internal proof role");

                            targets.push((
                                CallableExecutionTarget::Requirement {
                                    callable: values.intern_callable_instance(*callable)?,
                                    requirement,
                                },
                                required,
                            ));
                        }
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
    ) -> Result<CollectedExecutionProofs, FactQueryError> {
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

            if self.callable_body_key(definition)?.is_none() {
                let context = self.checker_context(cancellation)?;

                if bray_checker::CheckerRequestContext::implementation_hook(&context, symbol)?
                    .is_some_and(|hook| {
                        hook.is_available()
                            && (hook.hook().is_value_observation()
                                || hook.hook().is_range_iteration())
                    })
                {
                    origins.insert(anchor, CallableExecutionOrigin::CompilerIntrinsic);
                }
            }

            let declared = self.execution_declaration(anchor)?;

            diagnostics.add_range(declared.diagnostics().iter().cloned());

            let protocol = self.observational_protocol(symbol, diagnostics, cancellation)?;

            if declared.value().clauses().is_empty()
                && protocol.is_none()
                && !self.declares_constant_callable(definition, cancellation)?
            {
                continue;
            }

            if let Some(body) = self.callable_body_key(definition)? {
                let certificate =
                    self.certified_execution_with_cancellation(body.clone(), cancellation)?;

                diagnostics.add_range(certificate.result().diagnostics().iter().cloned());

                if certificate.result().diagnostics().has_errors() {
                    continue;
                }

                if let Some(protocol) = protocol {
                    let roles = [
                        ExecutionObligation::Observation,
                        ExecutionObligation::Termination,
                    ]
                    .into_iter()
                    .chain(
                        (protocol == bray_compiler_known::CompilerKnownIterationRole::IteratorNext)
                            .then_some(ExecutionObligation::FiniteIteration),
                    );

                    for role in roles {
                        let mut local_diagnostics = DiagnosticBag::new();

                        let (failure, _, dependencies) = self.certify_execution_obligation(
                            &body,
                            role,
                            &mut local_diagnostics,
                            false,
                            cancellation,
                        )?;

                        if failure.is_some() {
                            continue;
                        }

                        diagnostics.add_range(local_diagnostics.iter().cloned());
                        proofs.entry((anchor, role)).or_default();
                        record_proof_dependencies(graph, dependencies, &mut proofs);
                    }
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

                record_proof_dependencies(
                    graph,
                    certificate.dependencies.iter().copied(),
                    &mut proofs,
                );
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

                if origin == CallableExecutionOrigin::CompilerIntrinsic
                    && self.declares_constant_callable(definition, cancellation)?
                {
                    for property in [ExecutionProperty::Pure, ExecutionProperty::Total] {
                        proofs
                            .entry((anchor, ExecutionObligation::Property(property, None)))
                            .or_default();
                    }
                }

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

                    // Checked memory semantics own intrinsic output witnesses. This export
                    // certifies execution properties, not arbitrary source postconditions.
                    for post in domain
                        .postconditions
                        .iter()
                        .filter(|_| origin != CallableExecutionOrigin::CompilerIntrinsic)
                    {
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

    fn observational_protocol(
        &self,
        symbol: AnySymbolId,
        diagnostics: &mut DiagnosticBag,
        cancellation: &CancellationToken,
    ) -> Result<Option<bray_compiler_known::CompilerKnownIterationRole>, FactQueryError> {
        let Some(protocol) = self.available_compiler_known_symbols().iteration_protocol() else {
            return Ok(None);
        };

        let AnySymbolId::TraitCallableFulfillment(fulfillment) = symbol else {
            return Ok(None);
        };

        let graph = self.symbol_graph()?;
        let name = graph.member_name(symbol);

        let matches_protocol_name = [protocol.iterator_next(), protocol.iterable_iterate()]
            .iter()
            .any(|member| name.is_some() && graph.member_name((*member).into()) == name);

        if !matches_protocol_name {
            return Ok(None);
        }

        let implementation = graph
            .containing_symbol(symbol)
            .and_then(bray_symbols::ImplementationSymbolId::try_from_any)
            .expect("checked trait fulfillment retains its implementation");

        let conformance =
            self.trait_implementation_conformance_with_cancellation(implementation, cancellation)?;

        diagnostics.add_range(conformance.diagnostics().iter().cloned());

        Ok(conformance.value().requirements().iter().find_map(|entry| {
            if entry.resolution()
                != bray_symbols::TraitRequirementResolution::Explicit(
                    bray_symbols::TraitMemberFulfillmentId::Callable(fulfillment),
                )
            {
                return None;
            }

            match entry.requirement() {
                bray_symbols::TraitMemberRequirementId::Callable(member)
                    if member == protocol.iterator_next() =>
                {
                    Some(bray_compiler_known::CompilerKnownIterationRole::IteratorNext)
                }
                bray_symbols::TraitMemberRequirementId::Callable(member)
                    if member == protocol.iterable_iterate() =>
                {
                    Some(bray_compiler_known::CompilerKnownIterationRole::IterableIterate)
                }
                _ => None,
            }
        }))
    }

    fn portable_execution_domains(
        &self,
        symbol: AnySymbolId,
        demanded: &BTreeSet<ExecutionObligation>,
        cancellation: &CancellationToken,
    ) -> Result<PortableExecutionContract, FactQueryError> {
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

            let posts = domain
                .postconditions
                .iter()
                .copied()
                .filter(|post| {
                    demanded.contains(&ExecutionObligation::Postcondition(
                        bray_source::SourceSpan::new(post.source_id(), post.full_range()).into(),
                    ))
                })
                .collect::<Vec<_>>();

            let posts = self.execution_condition_inputs(&owner, &posts, cancellation)?;

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
                        .and_then(|candidate| candidate.result_variant())
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

fn record_proof_dependencies(
    graph: &bray_symbols::SymbolGraph,
    dependencies: impl IntoIterator<
        Item = (
            bray_declarations::SyntaxAnchor,
            ExecutionObligation,
            bray_checker::ExecutionDependencyTarget,
            ExecutionObligation,
        ),
    >,
    proofs: &mut SourceProofs,
) {
    for (owner, obligation, target, required) in dependencies {
        proofs
            .entry((owner, obligation))
            .or_default()
            .insert((target, required));

        if let bray_checker::ExecutionDependencyTarget::Callable(BoundCallableTarget::Declaration(
            callable,
        )) = target
            && let Some(anchor) = graph.declaration_syntax_anchor(callable.definition().symbol())
        {
            proofs.entry((anchor, required)).or_default();
        }
    }
}
