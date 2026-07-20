use std::{collections::BTreeMap, collections::btree_map::Entry, sync::Arc};

use bray_base::shared_slice;

use crate::{
    GenericConstraintTemplate, GenericSubstitutionId, ImplementationRequirementKey,
    ImplementationSymbolId, SymbolKey, TargetFactDependency,
};

/// Reports malformed evidence for one exact implementation coherence key.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ImplementationCoherenceEvidenceError {
    /// The evidence names no participating implementation declaration.
    NoImplementations,
    /// One stable key names conflicting implementation declarations.
    ConflictingParticipantKey,
}

/// One implementation declaration participating in coherence under a stable semantic key.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationCoherenceParticipant {
    key: SymbolKey,
    implementation: ImplementationSymbolId,
}

impl ImplementationCoherenceParticipant {
    /// Creates one coherence participant.
    ///
    /// `key` must be the stable semantic key for `implementation`.
    pub const fn new(key: SymbolKey, implementation: ImplementationSymbolId) -> Self {
        Self {
            key,
            implementation,
        }
    }

    /// Returns the participant's stable semantic key.
    pub const fn key(&self) -> &SymbolKey {
        &self.key
    }

    /// Returns the participating implementation declaration.
    pub const fn implementation(&self) -> ImplementationSymbolId {
        self.implementation
    }
}

/// The participating implementation declarations retained for one exact coherence key.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationCoherenceEvidence {
    key: ImplementationRequirementKey,
    participants: Arc<[ImplementationCoherenceParticipant]>,
}

impl ImplementationCoherenceEvidence {
    /// Creates canonical non-empty coherence evidence.
    ///
    /// Returns an error when no implementation participates or one stable key names conflicting
    /// implementation declarations.
    pub fn try_new(
        key: ImplementationRequirementKey,
        participants: impl IntoIterator<Item = ImplementationCoherenceParticipant>,
    ) -> Result<Self, ImplementationCoherenceEvidenceError> {
        let mut canonical = BTreeMap::new();

        for participant in participants {
            match canonical.entry(participant.key) {
                Entry::Vacant(entry) => {
                    entry.insert(participant.implementation);
                }
                Entry::Occupied(entry) if *entry.get() == participant.implementation => {}
                Entry::Occupied(_) => {
                    return Err(ImplementationCoherenceEvidenceError::ConflictingParticipantKey);
                }
            }
        }

        if canonical.is_empty() {
            return Err(ImplementationCoherenceEvidenceError::NoImplementations);
        }

        let participants = canonical.into_iter().map(|(key, implementation)| {
            ImplementationCoherenceParticipant::new(key, implementation)
        });

        Ok(Self {
            key,
            participants: shared_slice(participants),
        })
    }

    /// Returns the exact coherence key supported by this evidence.
    pub const fn key(&self) -> ImplementationRequirementKey {
        self.key
    }

    /// Returns participating implementations in canonical semantic-key order.
    pub fn participants(&self) -> &[ImplementationCoherenceParticipant] {
        &self.participants
    }
}

/// Reports malformed retained evidence for one implementation candidate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ImplementationCandidateError {
    /// The candidate declaration is absent from its coherence evidence.
    MissingCoherenceParticipant,
    /// One declaration-order position names conflicting generic constraint templates.
    ConflictingConstraintOrdinal,
    /// One stable target-fact key names conflicting dependencies.
    ConflictingTargetFactKey,
}

/// One uncommitted implementation candidate and the evidence required to check applicability.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationCandidate {
    key: SymbolKey,
    implementation: ImplementationSymbolId,
    substitution: GenericSubstitutionId,
    constraints: Arc<[GenericConstraintTemplate]>,
    target_dependencies: Arc<[TargetFactDependency]>,
    coherence: ImplementationCoherenceEvidence,
}

impl ImplementationCandidate {
    /// Creates one candidate without evaluating its constraints or target availability.
    ///
    /// Returns an error when the candidate is absent from its coherence evidence, one ordinal names
    /// conflicting constraints, or one stable target-fact key names conflicting dependencies.
    pub fn try_new(
        key: SymbolKey,
        implementation: ImplementationSymbolId,
        substitution: GenericSubstitutionId,
        constraints: impl IntoIterator<Item = GenericConstraintTemplate>,
        target_dependencies: impl IntoIterator<Item = TargetFactDependency>,
        coherence: ImplementationCoherenceEvidence,
    ) -> Result<Self, ImplementationCandidateError> {
        let participant = coherence
            .participants
            .binary_search_by(|participant| participant.key.cmp(&key))
            .ok()
            .map(|index| &coherence.participants[index]);

        if participant.is_none_or(|participant| participant.implementation != implementation) {
            return Err(ImplementationCandidateError::MissingCoherenceParticipant);
        }

        let mut canonical_constraints = BTreeMap::new();

        for constraint in constraints {
            match canonical_constraints.entry(constraint.ordinal()) {
                Entry::Vacant(entry) => {
                    entry.insert(constraint);
                }
                Entry::Occupied(entry) if *entry.get() == constraint => {}
                Entry::Occupied(_) => {
                    return Err(ImplementationCandidateError::ConflictingConstraintOrdinal);
                }
            }
        }

        let mut ordered_target_dependencies: Vec<_> = target_dependencies.into_iter().collect();

        ordered_target_dependencies.sort_by(|left, right| left.key().cmp(right.key()));

        let mut canonical_target_dependencies: Vec<TargetFactDependency> =
            Vec::with_capacity(ordered_target_dependencies.len());

        for dependency in ordered_target_dependencies {
            if let Some(previous) = canonical_target_dependencies.last()
                && previous.key() == dependency.key()
            {
                if previous != &dependency {
                    return Err(ImplementationCandidateError::ConflictingTargetFactKey);
                }

                continue;
            }

            canonical_target_dependencies.push(dependency);
        }

        Ok(Self {
            key,
            implementation,
            substitution,
            constraints: shared_slice(canonical_constraints.into_values()),
            target_dependencies: canonical_target_dependencies.into(),
            coherence,
        })
    }

    /// Returns the stable semantic definition key used for canonical ordering.
    pub const fn key(&self) -> &SymbolKey {
        &self.key
    }

    /// Returns the implementation declaration represented by this candidate.
    pub const fn implementation(&self) -> ImplementationSymbolId {
        self.implementation
    }

    /// Returns the inferred substitution without asserting that its predicates hold.
    pub const fn substitution(&self) -> GenericSubstitutionId {
        self.substitution
    }

    /// Returns generic constraint templates in declaration order.
    pub fn constraints(&self) -> &[GenericConstraintTemplate] {
        &self.constraints
    }

    /// Returns target-fact dependencies in canonical order.
    pub fn target_dependencies(&self) -> &[TargetFactDependency] {
        &self.target_dependencies
    }

    /// Returns the retained coherence evidence.
    pub const fn coherence(&self) -> &ImplementationCoherenceEvidence {
        &self.coherence
    }
}

/// Reports candidate collections that cannot represent one exact lookup result.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ImplementationCandidateSetError {
    /// A candidate carries coherence evidence for another lookup key.
    MismatchedCoherenceKey,
    /// One stable key names candidates with conflicting retained evidence.
    ConflictingCandidateKey,
}

/// Immutable implementation candidates in canonical semantic-key order.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationCandidateSet {
    candidates: Arc<[ImplementationCandidate]>,
}

impl ImplementationCandidateSet {
    /// Creates a canonical candidate set for one exact lookup key.
    ///
    /// Returns an error when candidate evidence belongs to another lookup key or one stable
    /// implementation key names conflicting candidates.
    pub fn try_new(
        key: ImplementationRequirementKey,
        candidates: impl IntoIterator<Item = ImplementationCandidate>,
    ) -> Result<Self, ImplementationCandidateSetError> {
        let mut ordered: Vec<_> = candidates.into_iter().collect();

        ordered.sort_by(|left, right| left.key.cmp(&right.key));

        let mut canonical: Vec<ImplementationCandidate> = Vec::with_capacity(ordered.len());

        for candidate in ordered {
            if candidate.coherence.key != key {
                return Err(ImplementationCandidateSetError::MismatchedCoherenceKey);
            }

            if let Some(previous) = canonical.last()
                && previous.key == candidate.key
            {
                if previous != &candidate {
                    return Err(ImplementationCandidateSetError::ConflictingCandidateKey);
                }

                continue;
            }

            canonical.push(candidate);
        }

        Ok(Self {
            candidates: canonical.into(),
        })
    }

    /// Returns retained candidates in canonical semantic-key order.
    pub fn candidates(&self) -> &[ImplementationCandidate] {
        &self.candidates
    }
}

#[cfg(test)]
mod tests {
    use bray_declarations::DeclarationId;

    use crate::{
        AnySymbolId, CheckedConstraint, ConstantSymbolId, ConstantValueData, ConstantValueId,
        ConstantValueKind, DependencyContractTemplateData, DependencyRequirement,
        DependencyRequirementKind, DependencySubject, DependencySubjectRoot,
        GenericConstraintTemplate, GenericOwnerId, GenericSubstitutionData, GenericSubstitutionId,
        ImplementationRequirementKey, NamedTraitImplementationSymbolId, PackageIdentity,
        PredicateSemanticSummary, SemanticValueStore, SymbolId, SymbolKey, SymbolKind,
        SymbolOrdinal, TargetFactDependency, TraitApplicationData, TraitSymbolId, TypeData,
    };

    use super::{
        ImplementationCandidate, ImplementationCandidateError, ImplementationCandidateSet,
        ImplementationCandidateSetError, ImplementationCoherenceEvidence,
        ImplementationCoherenceEvidenceError, ImplementationCoherenceParticipant,
    };

    struct CandidateFixture {
        lookup: ImplementationRequirementKey,
        other_lookup: ImplementationRequirementKey,
        first_definition: NamedTraitImplementationSymbolId,
        second_definition: NamedTraitImplementationSymbolId,
        first_substitution: GenericSubstitutionId,
        second_substitution: GenericSubstitutionId,
        first_value: ConstantValueId,
        second_value: ConstantValueId,
        predicate: PredicateSemanticSummary,
        other_predicate: PredicateSemanticSummary,
    }

    impl CandidateFixture {
        fn new() -> Self {
            let Ok(store) = SemanticValueStore::try_new() else {
                panic!("semantic value store identity must be available");
            };

            let Ok(subject) = store.intern_type(TypeData::Error) else {
                panic!("error type must be internable");
            };

            let Ok(other_subject) = store.intern_type(TypeData::tuple([])) else {
                panic!("unit tuple type must be internable");
            };

            let trait_definition = TraitSymbolId::from_symbol_id(SymbolId::new(10));
            let trait_substitution = empty_substitution(&store, trait_definition.into());

            let Ok(trait_application) = store.intern_trait_application(TraitApplicationData::new(
                trait_definition,
                trait_substitution,
            )) else {
                panic!("trait application must be internable");
            };

            let first_definition =
                NamedTraitImplementationSymbolId::from_symbol_id(SymbolId::new(20));

            let second_definition =
                NamedTraitImplementationSymbolId::from_symbol_id(SymbolId::new(21));

            let first_substitution = empty_substitution(&store, first_definition.into());
            let second_substitution = empty_substitution(&store, second_definition.into());

            let Ok(dependencies) =
                store.intern_dependency_contract_template(DependencyContractTemplateData::new([]))
            else {
                panic!("empty dependency contract must be internable");
            };

            let dependency = DependencyRequirement::direct(
                DependencySubject::root(DependencySubjectRoot::Result),
                DependencyRequirementKind::StorageAlive,
            );

            let Ok(other_dependencies) =
                store.intern_dependency_contract_template(DependencyContractTemplateData::new([
                    dependency,
                ]))
            else {
                panic!("non-empty dependency contract must be internable");
            };

            let Ok(first_value) = store.intern_constant_value(ConstantValueData::new(
                subject,
                ConstantValueKind::Boolean(true),
            )) else {
                panic!("first target value must be internable");
            };

            let Ok(second_value) = store.intern_constant_value(ConstantValueData::new(
                subject,
                ConstantValueKind::Boolean(false),
            )) else {
                panic!("second target value must be internable");
            };

            Self {
                lookup: ImplementationRequirementKey::new(subject, trait_application),
                other_lookup: ImplementationRequirementKey::new(other_subject, trait_application),
                first_definition,
                second_definition,
                first_substitution,
                second_substitution,
                first_value,
                second_value,
                predicate: PredicateSemanticSummary::new(dependencies),
                other_predicate: PredicateSemanticSummary::new(other_dependencies),
            }
        }

        fn coherence(&self) -> ImplementationCoherenceEvidence {
            let Ok(evidence) = ImplementationCoherenceEvidence::try_new(
                self.lookup,
                [
                    ImplementationCoherenceParticipant::new(
                        implementation_key(2),
                        self.first_definition.into(),
                    ),
                    ImplementationCoherenceParticipant::new(
                        implementation_key(1),
                        self.second_definition.into(),
                    ),
                    ImplementationCoherenceParticipant::new(
                        implementation_key(2),
                        self.first_definition.into(),
                    ),
                ],
            ) else {
                panic!("fixture coherence evidence must be valid");
            };

            evidence
        }
    }

    #[test]
    fn candidate_contracts_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ImplementationCoherenceEvidence>();
        assert_send_sync::<ImplementationCoherenceParticipant>();
        assert_send_sync::<ImplementationCandidate>();
        assert_send_sync::<ImplementationCandidateSet>();
    }

    #[test]
    fn candidate_sets_preserve_uncommitted_evidence_in_canonical_order() {
        let fixture = CandidateFixture::new();
        let coherence = fixture.coherence();

        let later_constraint = GenericConstraintTemplate::Resolved(CheckedConstraint::new(
            SymbolOrdinal::new(2),
            fixture.predicate,
        ));

        let earlier_constraint = GenericConstraintTemplate::Resolved(CheckedConstraint::new(
            SymbolOrdinal::new(1),
            fixture.predicate,
        ));

        let first_target = target_dependency(30, fixture.first_value);
        let second_target = target_dependency(31, fixture.second_value);

        let Ok(first) = ImplementationCandidate::try_new(
            implementation_key(2),
            fixture.first_definition.into(),
            fixture.first_substitution,
            [later_constraint, earlier_constraint],
            [
                second_target.clone(),
                first_target.clone(),
                second_target.clone(),
            ],
            coherence.clone(),
        ) else {
            panic!("first retained candidate must be valid");
        };

        let Ok(second) = ImplementationCandidate::try_new(
            implementation_key(1),
            fixture.second_definition.into(),
            fixture.second_substitution,
            [],
            [],
            coherence.clone(),
        ) else {
            panic!("second retained candidate must be valid");
        };

        let Ok(candidates) = ImplementationCandidateSet::try_new(
            fixture.lookup,
            [first.clone(), second.clone(), first],
        ) else {
            panic!("candidate set must canonicalize exact duplicates");
        };

        assert_eq!(coherence.key(), fixture.lookup);

        assert_eq!(
            coherence.participants(),
            &[
                ImplementationCoherenceParticipant::new(
                    implementation_key(1),
                    fixture.second_definition.into(),
                ),
                ImplementationCoherenceParticipant::new(
                    implementation_key(2),
                    fixture.first_definition.into(),
                ),
            ]
        );

        assert_eq!(candidates.candidates().len(), 2);
        assert_eq!(candidates.candidates()[0], second);
        assert_eq!(candidates.candidates()[1].key(), &implementation_key(2));

        assert_eq!(
            candidates.candidates()[1].implementation(),
            fixture.first_definition.into()
        );

        assert_eq!(
            candidates.candidates()[1].substitution(),
            fixture.first_substitution
        );

        assert_eq!(
            candidates.candidates()[1].constraints(),
            &[earlier_constraint, later_constraint]
        );

        assert_eq!(
            candidates.candidates()[1].target_dependencies(),
            &[first_target, second_target]
        );

        assert_eq!(candidates.candidates()[1].coherence(), &coherence);
    }

    #[test]
    fn candidate_constraints_are_ordered_and_exact_duplicates_are_removed() {
        let fixture = CandidateFixture::new();

        let later = GenericConstraintTemplate::Resolved(CheckedConstraint::new(
            SymbolOrdinal::new(2),
            fixture.predicate,
        ));

        let earlier = GenericConstraintTemplate::Resolved(CheckedConstraint::new(
            SymbolOrdinal::new(1),
            fixture.predicate,
        ));

        let Ok(candidate) = ImplementationCandidate::try_new(
            implementation_key(2),
            fixture.first_definition.into(),
            fixture.first_substitution,
            [later, earlier, earlier],
            [],
            fixture.coherence(),
        ) else {
            panic!("candidate constraints must canonicalize");
        };

        assert_eq!(candidate.constraints(), &[earlier, later]);
    }

    #[test]
    fn candidate_constraints_reject_conflicting_entries_at_one_ordinal() {
        let fixture = CandidateFixture::new();

        let first = GenericConstraintTemplate::Resolved(CheckedConstraint::new(
            SymbolOrdinal::new(1),
            fixture.predicate,
        ));

        let conflicting = GenericConstraintTemplate::Resolved(CheckedConstraint::new(
            SymbolOrdinal::new(1),
            fixture.other_predicate,
        ));

        assert_eq!(
            ImplementationCandidate::try_new(
                implementation_key(2),
                fixture.first_definition.into(),
                fixture.first_substitution,
                [first, conflicting],
                [],
                fixture.coherence(),
            ),
            Err(ImplementationCandidateError::ConflictingConstraintOrdinal)
        );
    }

    #[test]
    fn candidate_contracts_reject_conflicting_or_mismatched_evidence() {
        let fixture = CandidateFixture::new();

        assert_eq!(
            ImplementationCoherenceEvidence::try_new(fixture.lookup, []),
            Err(ImplementationCoherenceEvidenceError::NoImplementations)
        );

        assert_eq!(
            ImplementationCoherenceEvidence::try_new(
                fixture.lookup,
                [
                    ImplementationCoherenceParticipant::new(
                        implementation_key(1),
                        fixture.first_definition.into(),
                    ),
                    ImplementationCoherenceParticipant::new(
                        implementation_key(1),
                        fixture.second_definition.into(),
                    ),
                ],
            ),
            Err(ImplementationCoherenceEvidenceError::ConflictingParticipantKey)
        );

        let Ok(second_only) = ImplementationCoherenceEvidence::try_new(
            fixture.lookup,
            [ImplementationCoherenceParticipant::new(
                implementation_key(2),
                fixture.second_definition.into(),
            )],
        ) else {
            panic!("non-empty coherence evidence must be valid");
        };

        assert_eq!(
            ImplementationCandidate::try_new(
                implementation_key(1),
                fixture.first_definition.into(),
                fixture.first_substitution,
                [],
                [],
                second_only,
            ),
            Err(ImplementationCandidateError::MissingCoherenceParticipant)
        );

        let coherence = fixture.coherence();

        let conflicting_target = TargetFactDependency::new(
            constant_key(30),
            ConstantSymbolId::from_symbol_id(SymbolId::new(31)),
            fixture.second_value,
        );

        assert_eq!(
            ImplementationCandidate::try_new(
                implementation_key(2),
                fixture.first_definition.into(),
                fixture.first_substitution,
                [],
                [
                    target_dependency(30, fixture.first_value),
                    conflicting_target
                ],
                coherence,
            ),
            Err(ImplementationCandidateError::ConflictingTargetFactKey)
        );

        let first_only = coherence_for(
            fixture.lookup,
            [ImplementationCoherenceParticipant::new(
                implementation_key(1),
                fixture.first_definition.into(),
            )],
        );

        let expanded = coherence_for(
            fixture.lookup,
            [
                ImplementationCoherenceParticipant::new(
                    implementation_key(1),
                    fixture.first_definition.into(),
                ),
                ImplementationCoherenceParticipant::new(
                    implementation_key(2),
                    fixture.second_definition.into(),
                ),
            ],
        );

        let first_candidate = candidate(&fixture, first_only);
        let conflicting_candidate = candidate(&fixture, expanded);

        assert_eq!(
            ImplementationCandidateSet::try_new(
                fixture.lookup,
                [first_candidate, conflicting_candidate],
            ),
            Err(ImplementationCandidateSetError::ConflictingCandidateKey)
        );

        let other_evidence = coherence_for(
            fixture.other_lookup,
            [ImplementationCoherenceParticipant::new(
                implementation_key(1),
                fixture.first_definition.into(),
            )],
        );

        let mismatched_candidate = candidate(&fixture, other_evidence);

        assert_eq!(
            ImplementationCandidateSet::try_new(fixture.lookup, [mismatched_candidate]),
            Err(ImplementationCandidateSetError::MismatchedCoherenceKey)
        );
    }

    fn candidate(
        fixture: &CandidateFixture,
        coherence: ImplementationCoherenceEvidence,
    ) -> ImplementationCandidate {
        let Ok(candidate) = ImplementationCandidate::try_new(
            implementation_key(1),
            fixture.first_definition.into(),
            fixture.first_substitution,
            [],
            [],
            coherence,
        ) else {
            panic!("fixture candidate must be valid");
        };

        candidate
    }

    fn coherence_for<const N: usize>(
        lookup: ImplementationRequirementKey,
        participants: [ImplementationCoherenceParticipant; N],
    ) -> ImplementationCoherenceEvidence {
        let Ok(evidence) = ImplementationCoherenceEvidence::try_new(lookup, participants) else {
            panic!("fixture coherence evidence must be valid");
        };

        evidence
    }

    fn target_dependency(declaration: u32, value: ConstantValueId) -> TargetFactDependency {
        TargetFactDependency::new(
            constant_key(declaration),
            ConstantSymbolId::from_symbol_id(SymbolId::new(declaration)),
            value,
        )
    }

    fn implementation_key(declaration: u32) -> SymbolKey {
        source_key(SymbolKind::NamedTraitImplementation, declaration)
    }

    fn constant_key(declaration: u32) -> SymbolKey {
        source_key(SymbolKind::Constant, declaration)
    }

    fn source_key(kind: SymbolKind, declaration: u32) -> SymbolKey {
        let Some(package) = PackageIdentity::try_new("example.package") else {
            panic!("package identity must be valid");
        };

        let owner = SymbolKey::package(package);

        let Some(key) = SymbolKey::source_declaration(owner, kind, DeclarationId::new(declaration))
        else {
            panic!("fixture kind must support source declaration keys");
        };

        key
    }

    fn empty_substitution(store: &SemanticValueStore, owner: AnySymbolId) -> GenericSubstitutionId {
        let Some(owner) = GenericOwnerId::try_new(owner) else {
            panic!("implementation or trait must be a generic owner");
        };

        let Ok(substitution) = GenericSubstitutionData::try_new(owner, [], []) else {
            panic!("empty generic substitution must be valid");
        };

        let Ok(substitution) = store.intern_generic_substitution(substitution) else {
            panic!("empty generic substitution must be internable");
        };

        substitution
    }
}
