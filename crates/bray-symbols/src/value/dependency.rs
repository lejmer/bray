use crate::DependencyStorageEffect;
use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};

use crate::{
    StaticSymbolId, StructFieldSymbolId, SymbolOrdinal, UnionPayloadFieldSymbolId,
    UnionVariantSymbolId,
};

use super::{BorrowKind, ConstantTermId, ImplementationInstanceId};

/// A formal source-independent root referenced by a dependency contract.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencySubjectRoot {
    /// The callable receiver.
    Receiver,
    /// A callable parameter by stable declaration ordinal.
    Parameter(SymbolOrdinal),
    /// The callable result.
    Result,
    /// Storage owned by the evaluation whose returned contract is being inferred.
    EvaluationStorage,
    /// A scoped declaration capability by stable ordinal.
    ScopedCapability(SymbolOrdinal),
    /// A required selected implementation witness.
    ImplementationWitness(ImplementationInstanceId),
    /// Storage owned by one product-static instance.
    ProductStatic(StaticSymbolId),
    /// Storage owned by one exact native-thread static instance.
    ExactThreadStatic(StaticSymbolId),
}

/// One source-independent projection from a formal dependency subject.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyProjection {
    /// A named product field.
    ProductField(StructFieldSymbolId),
    /// A tuple element by stable ordinal.
    TupleElement(SymbolOrdinal),
    /// An array or slice element selected by a checked constant term.
    Element(ConstantTermId),
    /// The present value of a nullable subject.
    NullableValue,
    /// A named union payload field.
    UnionPayloadField(UnionPayloadFieldSymbolId),
    /// The target owned through an indirection layer.
    OwnedTarget,
    /// Inline protected storage. This projection establishes neither initialization nor access authority.
    UninitPayload,
}

/// A formal root plus its ordered source-independent projection path.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DependencySubject {
    root: DependencySubjectRoot,
    projections: Arc<[DependencyProjection]>,
}

impl DependencySubject {
    /// Creates a formal dependency subject.
    pub fn new(
        root: DependencySubjectRoot,
        projections: impl IntoIterator<Item = DependencyProjection>,
    ) -> Self {
        Self {
            root,
            projections: shared_slice(projections),
        }
    }

    /// Creates a formal subject without projections.
    pub fn root(root: DependencySubjectRoot) -> Self {
        Self::new(root, [])
    }

    /// Returns the formal root.
    pub const fn subject_root(&self) -> DependencySubjectRoot {
        self.root
    }

    /// Returns the ordered projection path.
    pub fn projections(&self) -> &[DependencyProjection] {
        &self.projections
    }
}

/// A portable lifecycle obligation retained by a dependency contract.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LifecycleObligationKind {
    /// Destruction must remain attached.
    Destruction,
    /// Finalization must remain attached.
    Finalization,
    /// Cancellation must remain attached.
    Cancellation,
    /// Joining must remain attached.
    Joining,
}

/// The exact requirement attached to one formal dependency subject.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyRequirementKind {
    /// The dependencies carried by the value must remain valid after it transfers.
    ValueDependencies,
    /// Reached storage must remain alive.
    StorageAlive,
    /// Reached storage must remain initialized.
    StorageInitialized,
    /// A borrow capability of the exact kind must remain active.
    BorrowCapabilityActive(BorrowKind),
    /// Exclusive mutation authority must remain active.
    ExclusiveMutationAuthority,
    /// A scoped capability must remain live.
    ScopedCapabilityLive,
    /// A lifecycle obligation must remain attached.
    LifecycleObligation(LifecycleObligationKind),
}

/// A semantic condition guarding nested dependency requirements.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyGuard {
    /// The nullable subject is present.
    NullablePresent(DependencySubject),
    /// The union subject has the exact active variant.
    ActiveUnionVariant {
        /// The guarded union subject.
        subject: DependencySubject,
        /// The required active variant.
        variant: UnionVariantSymbolId,
    },
}

/// A normalized guarded set of dependency requirements.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GuardedDependencyRequirement {
    guard: DependencyGuard,
    requirements: Arc<[DependencyRequirement]>,
}

impl GuardedDependencyRequirement {
    /// Creates a guarded requirement after deterministic sorting and deduplication.
    pub fn new(
        guard: DependencyGuard,
        requirements: impl IntoIterator<Item = DependencyRequirement>,
    ) -> Self {
        Self {
            guard,
            requirements: sorted_unique_shared_slice(requirements),
        }
    }

    /// Returns the semantic guard.
    pub const fn guard(&self) -> &DependencyGuard {
        &self.guard
    }

    /// Returns the normalized nested requirements.
    pub fn requirements(&self) -> &[DependencyRequirement] {
        &self.requirements
    }
}

/// One portable value, report, or cleanup requirement.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyRequirement {
    /// The type's remaining cleanup must execute synchronously.
    SynchronousCleanup(super::TypeId),
    /// A pending finalizer body must satisfy its synchronous callee obligations.
    FinalizerBodyCleanup(super::TypeId),
    /// Report dependencies retained only for capture types with cleanup work.
    CleanupDependencies {
        /// The capture type selected by generic specialization.
        ty: super::TypeId,
        /// Dependencies carried by a report from that capture's cleanup.
        requirements: Arc<[DependencyRequirement]>,
    },
    /// Cleanup performed while evaluating one exact declaration-owned default.
    DefaultCleanup {
        /// The selected parameter or field provider.
        provider: crate::DefaultValueProvider,
        /// The enclosing declaration's selected generic arguments.
        substitution: super::GenericSubstitutionId,
        /// The provider's lexical Self context and selected subject, when present.
        contextual_self: Option<(super::SelfTypeContext, super::TypeId)>,
    },
    /// Finite equations for dependencies carried around a local control-flow cycle.
    FixedPoint {
        /// Equation values, addressed by variable ordinal.
        definitions: Arc<[Arc<[DependencyRequirement]>]>,
        /// Dependencies selected from the equation group.
        result: Arc<[DependencyRequirement]>,
    },
    /// A reference to an enclosing finite equation group.
    Variable {
        /// Number of intervening equation groups.
        depth: u32,
        /// Equation ordinal in the selected group.
        ordinal: SymbolOrdinal,
    },
    /// An unconditional requirement on one formal subject.
    Direct {
        /// Formal subject carrying the dependency.
        subject: DependencySubject,
        /// Exact required semantic state.
        kind: DependencyRequirementKind,
    },
    /// Requirements active only while a semantic guard holds.
    Guarded(GuardedDependencyRequirement),
    /// A deferred callable result, optionally awaiting implementation selection.
    ResultCall {
        /// The selected value or owned report output.
        output: DependencyCallOutput,
        /// The declaration and its generic arguments.
        callable: super::CallableInstanceId,
        /// The subject and trait application selecting an implementation, when needed.
        requirement: Option<crate::ImplementationRequirementKey>,
        /// Dependencies of each actual input in the enclosing contract.
        inputs: Arc<[DependencyCallInput]>,
    },
}

/// The output selected from one checked callable summary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyCallOutput {
    /// The ordinary returned value.
    Value,
    /// Reports accompanying normal completion.
    CompletionReport,
    /// Reports accompanying panic or cancellation.
    AbnormalReport,
    /// Cleanup requirements imposed by synchronous bodies in the selected call graph.
    SynchronousCleanup,
}

/// The storage and value dependencies supplied to one symbolic result call.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DependencyCallInput {
    root: DependencySubjectRoot,
    values: Arc<[DependencyRequirement]>,
    storage: Arc<[DependencyRequirement]>,
}

impl DependencyCallInput {
    /// Creates an input with independently normalized value and storage requirements.
    pub fn new(
        root: DependencySubjectRoot,
        values: impl IntoIterator<Item = DependencyRequirement>,
        storage: impl IntoIterator<Item = DependencyRequirement>,
    ) -> Self {
        Self {
            root,
            values: sorted_unique_shared_slice(values),
            storage: sorted_unique_shared_slice(storage),
        }
    }

    /// Returns the receiver or parameter being supplied.
    pub const fn root(&self) -> DependencySubjectRoot {
        self.root
    }

    /// Returns dependencies carried by the supplied value.
    pub fn values(&self) -> &[DependencyRequirement] {
        &self.values
    }

    /// Returns dependencies keeping the supplied storage alive.
    pub fn storage(&self) -> &[DependencyRequirement] {
        &self.storage
    }
}

impl DependencyRequirement {
    /// Retains report dependencies until specialization can discharge trivial captures.
    pub fn cleanup_dependencies(
        ty: super::TypeId,
        requirements: impl IntoIterator<Item = DependencyRequirement>,
    ) -> Self {
        Self::CleanupDependencies { ty, requirements: sorted_unique_shared_slice(requirements) }
    }

    /// Creates a finite group of dependency equations.
    pub fn fixed_point(
        definitions: impl IntoIterator<Item = impl IntoIterator<Item = DependencyRequirement>>,
        result: impl IntoIterator<Item = DependencyRequirement>,
    ) -> Self {
        Self::FixedPoint {
            definitions: bray_base::shared_slice(
                definitions.into_iter().map(sorted_unique_shared_slice),
            ),
            result: sorted_unique_shared_slice(result),
        }
    }

    /// Refers to an enclosing dependency equation.
    pub const fn variable(depth: u32, ordinal: SymbolOrdinal) -> Self {
        Self::Variable { depth, ordinal }
    }

    /// Creates a deferred result relation with normalized inputs.
    pub fn result_call(
        callable: super::CallableInstanceId,
        requirement: Option<crate::ImplementationRequirementKey>,
        inputs: impl IntoIterator<Item = DependencyCallInput>,
        output: DependencyCallOutput,
    ) -> Self {
        Self::ResultCall {
            output,
            callable,
            requirement,
            inputs: sorted_unique_shared_slice(inputs),
        }
    }

    /// Creates an unconditional requirement.
    pub fn direct(subject: DependencySubject, kind: DependencyRequirementKind) -> Self {
        Self::Direct { subject, kind }
    }

    /// Creates normalized guarded requirements.
    pub fn guarded(
        guard: DependencyGuard,
        requirements: impl IntoIterator<Item = DependencyRequirement>,
    ) -> Self {
        Self::Guarded(GuardedDependencyRequirement::new(guard, requirements))
    }
}

/// The representation of a checked reference at normal completion.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyReferenceKind {
    /// A language borrow that observes its referent.
    Borrow,
    /// A raw address whose value does not itself observe its referent.
    Address,
}

/// Checked origin information across one callable's normal completion.
/// Exact relations identify the entry subject's original address. Other relations
/// jointly bound its possible regions without establishing address equality.
/// An absent entry represents the alternative outside those input regions.
/// This relation grants no storage, initialization, or acquisition authority.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DependencyBorrowRelation<Subject = DependencySubject, Projection = DependencyProjection>
{
    entry: Option<Subject>,
    exit: Subject,
    kind: DependencyReferenceKind,
    target_projections: Arc<[Projection]>,
    dereferences: u32,
    exact: bool,
}

impl<Subject, Projection> DependencyBorrowRelation<Subject, Projection> {
    /// Records a complete normal-exit origin bound.
    /// An absent input records a raw address's independence from invocation inputs,
    /// without establishing freshness, initialization, lifetime or trusted authority.
    pub fn new(entry: Option<Subject>, exit: Subject, kind: DependencyReferenceKind) -> Self {
        assert!(
            entry.is_some() || kind == DependencyReferenceKind::Address,
            "an independent raw-address bound cannot replace a borrow's input origin"
        );

        Self {
            exact: entry.is_some(),
            entry,
            exit,
            kind,
            target_projections: Arc::from([]),
            dereferences: 0,
        }
    }

    /// Retains a complete origin bound without claiming one exact address.
    pub const fn with_exactness(mut self, exact: bool) -> Self {
        assert!(
            !exact || self.entry.is_some(),
            "independent reference origin cannot identify an input address"
        );

        self.exact = exact;

        self
    }

    /// Whether the relation identifies one address on every completing path.
    pub const fn is_exact(&self) -> bool {
        self.exact
    }

    /// Selects additional nested borrow layers before reading the entry reference.
    pub fn with_dereferences(mut self, count: u32) -> Self {
        assert!(
            count == 0 || self.entry.is_some(),
            "independent reference origin has no input to dereference"
        );

        self.dereferences = count;

        self
    }

    /// Additional borrow layers beyond the reference selected by the entry subject.
    pub const fn dereferences(&self) -> u32 {
        self.dereferences
    }

    /// Projects the reached storage without reading a reference value stored in the selected field.
    pub fn with_target_projections(
        mut self,
        projections: impl IntoIterator<Item = Projection>,
    ) -> Self {
        self.target_projections = shared_slice(projections);

        assert!(
            self.entry.is_some() || self.target_projections.is_empty(),
            "independent reference origin has no input target to project"
        );

        self
    }

    /// Returns the address path relative to the entry reference's target.
    pub fn target_projections(&self) -> &[Projection] {
        &self.target_projections
    }

    /// Returns the reference representation established by dependency inference.
    pub const fn kind(&self) -> DependencyReferenceKind {
        self.kind
    }

    /// Returns the input origin, or None for a complete independent-input bound.
    pub const fn entry(&self) -> Option<&Subject> {
        self.entry.as_ref()
    }

    /// Returns the borrow observed after normal completion.
    pub const fn exit(&self) -> &Subject {
        &self.exit
    }
}

/// A checked trusted condition carried by a complete returned owner.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DependencyWitnessCarrier<Projection = DependencyProjection, Condition = ConstantTermId> {
    guarantee: Condition,
    path: Arc<[Projection]>,
}

impl<Projection, Condition: Copy> DependencyWitnessCarrier<Projection, Condition> {
    /// Records a checked carrier path relative to the complete returned value.
    pub fn new(guarantee: Condition, path: impl IntoIterator<Item = Projection>) -> Self {
        Self {
            guarantee,
            path: path.into_iter().collect(),
        }
    }

    /// Returns the portable condition independently supported by this owner.
    pub const fn guarantee(&self) -> Condition {
        self.guarantee
    }

    /// Returns the complete carrier's owned path within the result.
    pub fn path(&self) -> &[Projection] {
        &self.path
    }
}

/// A normalized source-independent dependency-contract template.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DependencyContractTemplateData {
    requirements: Arc<[DependencyRequirement]>,
    completion_reports: Option<Arc<[DependencyRequirement]>>,
    abnormal_reports: Option<Arc<[DependencyRequirement]>>,
    borrows: Arc<[DependencyBorrowRelation]>,
    returned_input: Option<ConstantTermId>,
    observed_result: Option<ConstantTermId>,
    carriers: Arc<[DependencyWitnessCarrier]>,
    effects: Arc<[super::DependencyStorageEffect]>,
    synchronous_cleanup: Arc<[DependencyRequirement]>,
    normal_completion: bool,
    normal_storage_preserved: bool,
}

impl DependencyContractTemplateData {
    /// Records a value observation shared by every mutation-free normal completion.
    /// This carries no ownership, storage identity or trusted authority.
    pub fn with_observed_result(mut self, result: Option<ConstantTermId>) -> Self {
        self.observed_result = result;

        self
    }

    /// Returns the checked value observation in receiver-first formal argument order.
    pub const fn observed_result(&self) -> Option<ConstantTermId> {
        self.observed_result
    }

    /// Records an unchanged owned input returned as the complete result on every normal exit.
    /// Normal completion must have no observable storage mutation or cleanup effects.
    /// The term contains only a formal input and owned projections, never an equality or dereference.
    pub fn with_returned_input(mut self, input: Option<ConstantTermId>) -> Self {
        self.returned_input = input;

        self
    }

    /// Returns the checked whole-value transport, without introducing trusted guarantees.
    pub const fn returned_input(&self) -> Option<ConstantTermId> {
        self.returned_input
    }

    /// Attaches dependencies of reports that can accompany normal completion.
    /// An empty set still records that an independently owned report can be produced.
    pub fn with_completion_reports(
        mut self,
        requirements: impl IntoIterator<Item = DependencyRequirement>,
    ) -> Self {
        self.completion_reports = Some(sorted_unique_shared_slice(requirements));

        self
    }

    /// Returns report dependencies for normal completion, absent when no report is produced.
    pub fn completion_reports(&self) -> Option<&[DependencyRequirement]> {
        self.completion_reports.as_deref()
    }

    /// Attaches dependencies of reports propagated with panic or cancellation.
    /// An empty set still records that an independently owned report can be produced.
    pub fn with_abnormal_reports(
        mut self,
        requirements: impl IntoIterator<Item = DependencyRequirement>,
    ) -> Self {
        self.abnormal_reports = Some(sorted_unique_shared_slice(requirements));

        self
    }

    /// Returns report dependencies for abnormal completion, absent when no report is produced.
    pub fn abnormal_reports(&self) -> Option<&[DependencyRequirement]> {
        self.abnormal_reports.as_deref()
    }

    /// Removes implicit report outputs consumed by a native ABI boundary.
    /// Explicit value dependencies, reference relations and effects remain unchanged.
    pub fn without_reports(mut self) -> Self {
        self.completion_reports = None;
        self.abnormal_reports = None;

        self
    }

    /// Transforms requirements in every output while preserving report presence.
    pub fn try_map_requirements<E>(
        &self,
        mut map: impl FnMut(
            DependencyCallOutput,
            &[DependencyRequirement],
        ) -> Result<Vec<DependencyRequirement>, E>,
    ) -> Result<Self, E> {
        let mut result =
            self.with_requirements(map(DependencyCallOutput::Value, self.requirements())?);

        if let Some(requirements) = self.completion_reports() {
            result = result.with_completion_reports(map(
                DependencyCallOutput::CompletionReport,
                requirements,
            )?);
        }

        if let Some(requirements) = self.abnormal_reports() {
            result = result
                .with_abnormal_reports(map(DependencyCallOutput::AbnormalReport, requirements)?);
        }

        result = result.with_synchronous_cleanup(map(
            DependencyCallOutput::SynchronousCleanup,
            self.synchronous_cleanup(),
        )?);

        Ok(result)
    }

    /// Enumerates requirements in every output of this callable.
    pub fn all_requirements(&self) -> impl Iterator<Item = &DependencyRequirement> {
        self.requirements
            .iter()
            .chain(
                self.completion_reports
                    .iter()
                    .flat_map(|requirements| requirements.iter()),
            )
            .chain(
                self.abnormal_reports
                    .iter()
                    .flat_map(|requirements| requirements.iter()),
            )
            .chain(self.synchronous_cleanup.iter())
    }

    /// Enumerates formal reference paths required by exact relations and mutation effects.
    pub fn reference_subjects(&self) -> impl Iterator<Item = &DependencySubject> {
        self.borrows()
            .iter()
            .flat_map(|relation| {
                relation
                    .entry()
                    .into_iter()
                    .chain(std::iter::once(relation.exit()))
            })
            .chain(self.effects().iter().filter_map(|effect| match effect {
                DependencyStorageEffect::Atomic
                | DependencyStorageEffect::Ambient
                | DependencyStorageEffect::Unknown
                | DependencyStorageEffect::Escape
                | DependencyStorageEffect::Cleanup(_) => None,
                DependencyStorageEffect::Direct { subject, .. } => Some(subject),
                DependencyStorageEffect::Indirect { reference, .. } => Some(reference),
            }))
    }

    /// Returns requirements for one selected output.
    /// Absence and an independent report both have no lifetime requirements.
    pub fn output_requirements(&self, output: DependencyCallOutput) -> &[DependencyRequirement] {
        match output {
            DependencyCallOutput::Value => self.requirements(),
            DependencyCallOutput::CompletionReport => self.completion_reports().unwrap_or(&[]),
            DependencyCallOutput::AbnormalReport => self.abnormal_reports().unwrap_or(&[]),
            DependencyCallOutput::SynchronousCleanup => self.synchronous_cleanup(),
        }
    }

    /// Creates a template after deterministic sorting and deduplication.
    pub fn new(requirements: impl IntoIterator<Item = DependencyRequirement>) -> Self {
        Self {
            requirements: sorted_unique_shared_slice(requirements),
            completion_reports: None,
            abnormal_reports: None,
            borrows: Arc::new([]),
            returned_input: None,
            observed_result: None,
            carriers: Arc::new([]),
            effects: Arc::new(super::DependencyStorageEffect::opaque_call()),
            synchronous_cleanup: Arc::new([]),
            normal_completion: true,
            normal_storage_preserved: false,
        }
    }

    /// Replaces requirements while retaining the checked reference, effect and completion summary.
    pub fn with_requirements(
        &self,
        requirements: impl IntoIterator<Item = DependencyRequirement>,
    ) -> Self {
        Self {
            requirements: sorted_unique_shared_slice(requirements),
            // Each output retains the same immutable report summary during value-only refinement.
            completion_reports: self.completion_reports.as_ref().map(Arc::clone),
            abnormal_reports: self.abnormal_reports.as_ref().map(Arc::clone),
            // Transformations own a new contract while retaining the immutable checked summary.
            borrows: Arc::clone(&self.borrows),
            returned_input: self.returned_input,
            observed_result: self.observed_result,
            carriers: Arc::clone(&self.carriers),
            effects: Arc::clone(&self.effects),
            synchronous_cleanup: Arc::clone(&self.synchronous_cleanup),
            normal_completion: self.normal_completion,
            normal_storage_preserved: self.normal_storage_preserved,
        }
    }

    /// Attaches complete carrier paths proved by the owning body analysis.
    pub fn with_carriers(
        mut self,
        carriers: impl IntoIterator<Item = DependencyWitnessCarrier>,
    ) -> Self {
        self.carriers = sorted_unique_shared_slice(carriers);

        self
    }

    /// Returns the proven returned carriers of declared trusted guarantees.
    pub fn carriers(&self) -> &[DependencyWitnessCarrier] {
        &self.carriers
    }

    /// Records whether the checked body can produce a normal result.
    pub fn with_normal_completion(mut self, possible: bool) -> Self {
        self.normal_completion = possible;

        self
    }

    /// False proves that execution has no normal result. Unknown callees default to true.
    pub const fn normal_completion(&self) -> bool {
        self.normal_completion
    }

    /// Records the owning analysis's proof that normal return preserves external storage.
    /// Ownership transfers and effects on abnormal outcomes remain independent.
    pub fn with_normal_storage_preservation(mut self, preserved: bool) -> Self {
        self.normal_storage_preserved = preserved;

        self
    }

    /// Whether normal return preserves storage, independently of input ownership transfers.
    pub fn preserves_storage_on_normal_return(&self) -> bool {
        self.normal_storage_preserved || self.effects.is_empty()
    }

    /// Attaches complete entry-to-exit origin bounds and exact identities in deterministic order.
    pub fn with_borrows(
        mut self,
        borrows: impl IntoIterator<Item = DependencyBorrowRelation>,
    ) -> Self {
        let borrows = sorted_unique_shared_slice(borrows);

        if borrows.len() < 2 {
            self.borrows = borrows;

            return self;
        }

        let mut alternatives = std::collections::BTreeMap::new();

        for relation in borrows.iter() {
            *alternatives.entry(relation.exit()).or_insert(0usize) += 1;
        }

        self.borrows = sorted_unique_shared_slice(borrows.iter().cloned().map(|relation| {
            let exact = relation.is_exact() && alternatives[relation.exit()] == 1;

            relation.with_exactness(exact)
        }));

        self
    }

    /// Returns checked origin bounds and exact identities, independently of lifetime requirements.
    pub fn borrows(&self) -> &[DependencyBorrowRelation] {
        &self.borrows
    }

    /// Attaches the complete may-mutation summary proved by the owning body analysis.
    pub fn with_effects(
        mut self,
        effects: impl IntoIterator<Item = super::DependencyStorageEffect>,
    ) -> Self {
        self.effects = super::DependencyStorageEffect::normalize(effects);

        self
    }

    /// Records remaining cleanup that must be admissible in a synchronous callee.
    /// These obligations survive forwarding through asynchronous callers.
    pub fn with_synchronous_cleanup(
        mut self,
        requirements: impl IntoIterator<Item = DependencyRequirement>,
    ) -> Self {
        self.synchronous_cleanup = sorted_unique_shared_slice(requirements);

        self
    }

    /// Returns cleanup obligations to discharge after generic substitution.
    pub fn synchronous_cleanup(&self) -> &[DependencyRequirement] {
        &self.synchronous_cleanup
    }

    /// Returns possible mutations. An empty slice proves storage is unchanged.
    pub fn effects(&self) -> &[super::DependencyStorageEffect] {
        &self.effects
    }

    /// Returns the normalized requirements.
    pub fn requirements(&self) -> &[DependencyRequirement] {
        &self.requirements
    }
}

#[cfg(test)]
mod tests {
    use crate::SymbolOrdinal;

    use super::{
        DependencyContractTemplateData, DependencyRequirement, DependencyRequirementKind,
        DependencySubject, DependencySubjectRoot,
    };

    #[test]
    fn output_mapping_preserves_absence_and_normalizes_every_owned_output() {
        let input = DependencyRequirement::direct(
            DependencySubject::root(DependencySubjectRoot::Receiver),
            DependencyRequirementKind::StorageAlive,
        );

        let template = DependencyContractTemplateData::new([])
            .with_completion_reports([input.clone()])
            .with_abnormal_reports([]);

        let mapped = template
            .try_map_requirements::<std::convert::Infallible>(|_, requirements| {
                Ok(requirements.iter().chain(requirements).cloned().collect())
            })
            .unwrap();

        assert_eq!(mapped, template);
        assert_eq!(mapped.all_requirements().collect::<Vec<_>>(), [&input]);

        assert_ne!(
            DependencyContractTemplateData::new([]),
            DependencyContractTemplateData::new([]).with_completion_reports([])
        );

        assert!(
            DependencyContractTemplateData::new([])
                .try_map_requirements::<()>(|_, _| Ok(Vec::new()))
                .unwrap()
                .completion_reports()
                .is_none()
        );
    }

    #[test]
    fn dependency_templates_are_normalized() {
        let subject =
            DependencySubject::root(DependencySubjectRoot::Parameter(SymbolOrdinal::new(0)));

        let requirement =
            DependencyRequirement::direct(subject, DependencyRequirementKind::StorageAlive);

        let template =
            DependencyContractTemplateData::new([requirement.clone(), requirement.clone()]);

        assert_eq!(template.requirements(), &[requirement]);
    }
    #[test]
    fn replacing_requirements_retains_the_checked_summary() {
        let subject = DependencySubject::root(DependencySubjectRoot::Receiver);

        let relation = crate::DependencyBorrowRelation::new(
            Some(subject.clone()),
            DependencySubject::root(DependencySubjectRoot::Result),
            crate::DependencyReferenceKind::Borrow,
        );

        let template = DependencyContractTemplateData::new([])
            .with_borrows([relation])
            .with_effects([crate::DependencyStorageEffect::Direct {
                subject: subject.clone(),
                kind: crate::DependencyMutationKind::Write,
            }])
            .with_completion_reports([])
            .with_abnormal_reports([DependencyRequirement::direct(
                subject.clone(),
                DependencyRequirementKind::StorageAlive,
            )])
            .with_normal_completion(false);

        let requirement =
            DependencyRequirement::direct(subject, DependencyRequirementKind::StorageAlive);

        let changed = template.with_requirements([requirement.clone(), requirement.clone()]);

        assert_eq!(changed.requirements(), &[requirement]);
        assert_eq!(changed.borrows(), template.borrows());
        assert_eq!(changed.effects(), template.effects());
        assert_eq!(changed.completion_reports(), Some([].as_slice()));
        assert_eq!(changed.abnormal_reports(), template.abnormal_reports());
        assert!(!changed.normal_completion());
        assert!(template.requirements().is_empty());
    }
}
