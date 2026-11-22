use super::DependencyOwnedResult;
use crate::DependencyStorageEffect;
use std::sync::Arc;

use super::DependencyWitnessCarrier;

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
    /// An implementation witness selected when its generic subject becomes concrete.
    ImplementationRequirement(crate::ImplementationRequirementKey),
    /// Declaration-wide region for a product static.
    ProductStatic(StaticSymbolId),
    /// Declaration-wide region for a native-thread static.
    ExactThreadStatic(StaticSymbolId),
    /// Exact selected storage in the product or thread domain.
    StaticInstance(crate::StaticReferenceId, crate::StaticStorageDuration),
}

impl DependencySubjectRoot {
    /// Whether this root names a static declaration region or an exact selection.
    pub const fn is_static(self) -> bool {
        self.static_duration().is_some()
    }

    /// Returns the ownership domain of a static region or selection.
    pub const fn static_duration(self) -> Option<crate::StaticStorageDuration> {
        match self {
            Self::ProductStatic(_) => Some(crate::StaticStorageDuration::Product),
            Self::ExactThreadStatic(_) => Some(crate::StaticStorageDuration::ExactThread),
            Self::StaticInstance(_, duration) => Some(duration),
            _ => None,
        }
    }

    /// Returns the declaration that can bound this static region.
    pub fn static_declaration(self, values: &crate::SemanticValueStore) -> Option<StaticSymbolId> {
        match self {
            Self::ProductStatic(declaration) | Self::ExactThreadStatic(declaration) => Some(declaration),
            Self::StaticInstance(reference, _) => Some(values.static_reference(reference).template().declaration()),
            _ => None,
        }
    }

    /// Position of this formal among the evaluated receiver and arguments.
    pub fn input_ordinal(self, has_receiver: bool) -> Option<SymbolOrdinal> {
        match self {
            Self::Receiver if has_receiver => Some(SymbolOrdinal::new(0)),
            Self::Parameter(ordinal) => Some(SymbolOrdinal::new(ordinal.raw().checked_add(u32::from(has_receiver))
                .expect("formal ordinal fits receiver-first order"))),
            _ => None,
        }
    }
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
    /// A region in an unspecified array or slice element, without an exact address.
    AnyElement,
    /// The value reached through a raw data address. Establishes no access or ownership.
    AddressTarget,
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

    /// Whether the other subject is the same subtree or a descendant with the same formal root.
    pub fn contains(&self, other: &Self) -> bool {
        self.root == other.root && other.projections.starts_with(&self.projections)
    }
}

/// A formal stored-value subtree and terminal borrow layers after its projections.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DependencyStoredValuePath<Subject = DependencySubject> {
    subject: Subject,
    dereferences: u32,
}

impl<Subject> DependencyStoredValuePath<Subject> {
    /// Creates a subtree endpoint with its terminal borrow depth.
    pub const fn new(subject: Subject, dereferences: u32) -> Self { Self { subject, dereferences } }

    /// Returns the formal subject before terminal dereferences.
    pub const fn subject(&self) -> &Subject { &self.subject }

    /// Returns the number of terminal borrow layers.
    pub const fn dereferences(&self) -> u32 { self.dereferences }
}

/// Possible input-derived values retained within one normal-output subtree.
/// This describes physical dependencies, not equality or authority.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DependencyStoredValue<Subject = DependencySubject> {
    destination: DependencyStoredValuePath<Subject>,
    sources: Arc<[DependencyStoredValuePath<Subject>]>,
    complete: bool,
}

impl<Subject: Ord> DependencyStoredValue<Subject> {
    /// Creates a canonical possible-source bound for one output subtree.
    pub fn new(destination: DependencyStoredValuePath<Subject>, sources: impl IntoIterator<Item = DependencyStoredValuePath<Subject>>, complete: bool) -> Self {
        Self { destination, sources: sorted_unique_shared_slice(sources), complete }
    }
}

impl<Subject: Clone + Ord> DependencyStoredValue<Subject> {
    /// Merges alternative summaries by destination.
    pub fn normalize(values: impl IntoIterator<Item = Self>) -> Arc<[Self]> {
        let mut destinations = std::collections::BTreeMap::<DependencyStoredValuePath<Subject>, (std::collections::BTreeSet<DependencyStoredValuePath<Subject>>, bool)>::new();

        for value in values {
            let entry = destinations.entry(value.destination).or_insert_with(|| (Default::default(), true));

            entry.0.extend(value.sources.iter().cloned());
            entry.1 &= value.complete;
        }

        destinations.into_iter().map(|(destination, (sources, complete))| Self::new(destination, sources, complete)).collect::<Vec<_>>().into()
    }
}

impl<Subject> DependencyStoredValue<Subject> {
    /// Returns the output subtree.
    pub const fn destination(&self) -> &DependencyStoredValuePath<Subject> { &self.destination }

    /// Returns the possible captured input sources.
    pub fn sources(&self) -> &[DependencyStoredValuePath<Subject>] { &self.sources }

    /// Whether the listed sources and separate reference relations cover the subtree.
    pub const fn is_complete(&self) -> bool { self.complete }

    /// Retains known sources when another possible outcome has unknown contents.
    pub fn incomplete(mut self) -> Self {
        self.complete = false;

        self
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
        Self::CleanupDependencies {
            ty,
            requirements: sorted_unique_shared_slice(requirements),
        }
    }

    /// Creates a finite group of dependency equations.
    pub fn fixed_point(
        definitions: impl IntoIterator<Item = impl IntoIterator<Item = DependencyRequirement>>,
        result: impl IntoIterator<Item = DependencyRequirement>,
    ) -> Self {
        Self::FixedPoint {
            definitions: shared_slice(
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

/// What a checked reference relation establishes about its input.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyReferenceProvenance {
    /// The output has the same address as the selected input.
    Exact,
    /// The output forwards one of the established input origins.
    Alternatives,
    /// The output may reach these input regions without forwarding their authority.
    InputRegions,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum ReferenceSource<Subject> {
    Input(Subject),
    Independent,
    Allocation(Subject),
    Empty,
    NoPayload,
}

/// Checked origin information across one callable's normal completion.
/// An input can identify an address or bound its possible regions. Allocations
/// retain producer identity and independence from pre-existing storage. Other
/// independent origins establish no freshness. None grants access authority.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DependencyBorrowRelation<Subject = DependencySubject, Projection = DependencyProjection>
{
    source: ReferenceSource<Subject>,
    exit: Subject,
    kind: DependencyReferenceKind,
    target_projections: Arc<[Projection]>,
    dereferences: u32,
    provenance: DependencyReferenceProvenance,
}

impl<Subject, Projection> DependencyBorrowRelation<Subject, Projection> {
    /// Records a complete normal-exit origin bound.
    /// An absent input records a reference's independence from invocation inputs,
    /// without establishing freshness, initialization, lifetime or trusted authority.
    pub fn new(entry: Option<Subject>, exit: Subject, kind: DependencyReferenceKind) -> Self {
        Self {
            provenance: if entry.is_some() {
                DependencyReferenceProvenance::Exact
            } else {
                DependencyReferenceProvenance::Alternatives
            },
            source: entry.map_or(ReferenceSource::Independent, ReferenceSource::Input),
            exit,
            kind,
            target_projections: Arc::from([]),
            dereferences: 0,
        }
    }

    /// Records storage allocated during this invocation. References with the same
    /// allocation subject share one allocation identity, including aliased outputs.
    pub fn new_allocation(allocation: Subject, exit: Subject, kind: DependencyReferenceKind) -> Self {
        assert_eq!(kind, DependencyReferenceKind::Address, "a raw allocation does not establish a borrowed lifetime");

        Self {
            source: ReferenceSource::Allocation(allocation),
            ..Self::new(None, exit, kind)
        }
    }

    /// Records a live reference with a null address.
    pub fn new_empty(exit: Subject, kind: DependencyReferenceKind) -> Self {
        Self { source: ReferenceSource::Empty, ..Self::new(None, exit, kind) }
    }

    /// Whether the producer established that this slot has no reference target.
    pub const fn is_empty_reference(&self) -> bool {
        matches!(self.source, ReferenceSource::Empty)
    }

    /// Records an inactive slot that contributes no reference value.
    pub fn new_no_payload(exit: Subject, kind: DependencyReferenceKind) -> Self {
        Self { source: ReferenceSource::NoPayload, ..Self::new(None, exit, kind) }
    }

    /// Whether the slot contributes no live payload to this normal outcome.
    pub const fn is_no_payload(&self) -> bool {
        matches!(self.source, ReferenceSource::NoPayload)
    }

    /// Retains a complete origin bound without claiming one exact address.
    pub const fn with_exactness(mut self, exact: bool) -> Self {
        assert!(
            !exact || self.entry().is_some(),
            "independent reference origin cannot identify an input address"
        );

        if exact {
            self.provenance = DependencyReferenceProvenance::Exact;
        } else if !matches!(self.provenance, DependencyReferenceProvenance::InputRegions) {
            self.provenance = DependencyReferenceProvenance::Alternatives;
        }

        self
    }

    /// Whether the relation identifies one address on every completing path.
    pub const fn is_exact(&self) -> bool {
        matches!(self.provenance, DependencyReferenceProvenance::Exact)
    }

    /// Records whether the relation forwards a value or only bounds possible reachability.
    pub fn with_provenance(mut self, provenance: DependencyReferenceProvenance) -> Self {
        assert!(!(self.is_empty_reference() || self.is_no_payload()) || provenance == DependencyReferenceProvenance::Alternatives,
            "an empty address or inactive slot has no input provenance");

        assert!(
            provenance != DependencyReferenceProvenance::Exact || self.entry().is_some(),
            "independent reference origin cannot identify an input address"
        );

        self.provenance = provenance;

        self
    }

    /// Whether the relation identifies an address, forwards an origin, or only bounds reachability.
    pub const fn provenance(&self) -> DependencyReferenceProvenance {
        self.provenance
    }

    /// Selects additional nested borrow layers before reading the entry reference.
    pub fn with_dereferences(mut self, count: u32) -> Self {
        assert!(
            count == 0 || self.entry().is_some(),
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
            self.source_subject().is_some() || self.target_projections.is_empty(),
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

    /// Returns the input origin. Allocations and other independent origins have no input.
    pub const fn entry(&self) -> Option<&Subject> {
        match &self.source {
            ReferenceSource::Input(subject) => Some(subject),
            ReferenceSource::Independent | ReferenceSource::Allocation(_) | ReferenceSource::Empty | ReferenceSource::NoPayload => None,
        }
    }

    /// Returns the output subject identifying storage allocated by this invocation.
    pub const fn allocation(&self) -> Option<&Subject> {
        match &self.source {
            ReferenceSource::Allocation(subject) => Some(subject),
            ReferenceSource::Independent | ReferenceSource::Input(_) | ReferenceSource::Empty | ReferenceSource::NoPayload => None,
        }
    }

    /// Returns the input or allocation subject carried by this origin.
    pub const fn source_subject(&self) -> Option<&Subject> {
        match &self.source {
            ReferenceSource::Allocation(subject) | ReferenceSource::Input(subject) => Some(subject),
            ReferenceSource::Independent | ReferenceSource::Empty | ReferenceSource::NoPayload => None,
        }
    }

    /// Maps portable subjects and projections while retaining the complete origin contract.
    pub fn remap<S, P>(&self, source: Option<S>, exit: S, projections: impl IntoIterator<Item = P>) -> DependencyBorrowRelation<S, P> {
        let source = match &self.source {
            ReferenceSource::Input(_) => ReferenceSource::Input(source.expect("mapped input origin retains its subject")),
            ReferenceSource::Allocation(_) => ReferenceSource::Allocation(source.expect("mapped allocation origin retains its subject")),
            ReferenceSource::Independent => {
                assert!(source.is_none(), "independent origin has no source subject");

                ReferenceSource::Independent
            }
            ReferenceSource::Empty => {
                assert!(source.is_none(), "empty reference has no source subject");

                ReferenceSource::Empty
            }
            ReferenceSource::NoPayload => {
                assert!(source.is_none(), "inactive slot has no source subject");

                ReferenceSource::NoPayload
            }
        };

        DependencyBorrowRelation {
            source, exit, kind: self.kind,
            target_projections: shared_slice(projections),
            dereferences: self.dereferences, provenance: self.provenance,
        }
    }

    /// Returns the borrow observed after normal completion.
    pub const fn exit(&self) -> &Subject {
        &self.exit
    }
}

/// A normalized source-independent dependency-contract template.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DependencyContractTemplateData {
    requirements: Arc<[DependencyRequirement]>,
    failure_outcomes: Option<Arc<[super::CapturedFailureOutcome]>>,
    captured_values: Arc<[super::CapturedValueOutcome]>,
    effects: super::DependencyEffectOutcomes<DependencyStorageEffect>,
    synchronous_cleanup: Arc<[DependencyRequirement]>,
    normal_completion: bool,
}

impl DependencyContractTemplateData {
    /// Records the actual whole-value expression produced by normal completion.
    pub fn with_captured_values(mut self, values: impl IntoIterator<Item = super::CapturedValueOutcome>) -> Self {
        self.captured_values = sorted_unique_shared_slice(values);

        self.refresh_normal_effects()
    }

    /// Returns the whole-value expression, with explicit unknown for lost precision.
    pub fn captured_values(&self) -> &[super::CapturedValueOutcome] { &self.captured_values }

    /// Shares the normalized outcome rows with a bound invocation.
    pub fn shared_captured_values(&self) -> Arc<[super::CapturedValueOutcome]> {
        Arc::clone(&self.captured_values)
    }

    /// Derives possible stored-value dependencies across normal outcomes.
    pub fn stored_values(&self) -> Arc<[DependencyStoredValue]> {
        super::CapturedValueOutcome::stored_dependencies(self.captured_values.iter(), DependencySubject::contains)
    }

    /// Derives the ownership layout before selecting one normal exit.
    pub fn owned_result(&self) -> DependencyOwnedResult {
        super::CapturedValueOutcome::ownership_layout(&self.captured_values)
    }

    /// Attaches dependencies of reports that can accompany normal completion.
    /// An empty set still records that an independently owned report can be produced.
    pub fn with_completion_reports(
        mut self,
        requirements: impl IntoIterator<Item = DependencyRequirement>,
    ) -> Self {
        let report = sorted_unique_shared_slice(requirements);

        if self.captured_values.is_empty() {
            self.captured_values = [super::CapturedValueOutcome::new(None, [])
                .with_effects(self.normal_effects().iter().cloned()).with_report(Some(report))].into();
        } else {
            self.captured_values = sorted_unique_shared_slice(self.captured_values.iter().cloned()
                .map(|row| row.with_report(Some(Arc::clone(&report)))));
        }

        self
    }

    /// Returns report dependencies for normal completion, absent when no report is produced.
    pub fn completion_reports(&self) -> Option<Arc<[DependencyRequirement]>> {
        super::CapturedValueOutcome::possible_report(self.captured_values.iter())
    }

    /// Describes a possible report from a callable whose failure prefix is opaque.
    pub fn with_abnormal_reports(
        self,
        requirements: impl IntoIterator<Item = DependencyRequirement>,
    ) -> Self {
        self.with_failure_outcomes(Some([super::CapturedFailureOutcome {
            kind: None,
            guard: None,
            report: Some(sorted_unique_shared_slice(requirements)),
            effects: None,
            cell_changes: None,
            writes: Arc::new([]),
        }]))
    }

    /// Records checked failure prefixes. An empty list proves no abnormal exit.
    pub fn with_failure_outcomes(mut self, outcomes: Option<impl IntoIterator<Item = super::CapturedFailureOutcome>>) -> Self {
        self.failure_outcomes = outcomes.map(sorted_unique_shared_slice);

        self.refresh_normal_effects()
    }

    /// Returns reached failure rows, absent when the body's failure path is opaque.
    pub fn failure_outcomes(&self) -> Option<&[super::CapturedFailureOutcome]> {
        self.failure_outcomes.as_deref()
    }

    /// Derives possible report dependencies across failure outcomes.
    pub fn abnormal_reports(&self) -> Option<Arc<[DependencyRequirement]>> {
        let mut reports = self.failure_outcomes.as_deref()?.iter().filter_map(|row| row.report.as_ref());
        let first = reports.next()?;

        Some(sorted_unique_shared_slice(first.iter().chain(reports.flat_map(|report| report.iter())).cloned()))
    }

    /// Removes implicit report outputs consumed by a native ABI boundary.
    /// Explicit value dependencies, reference relations and effects remain unchanged.
    pub fn without_reports(mut self) -> Self {
        if self.captured_values.iter().any(|row| row.report.is_some()) {
            self.captured_values = sorted_unique_shared_slice(self.captured_values.iter().cloned()
                .map(|row| row.with_report(None)));
        }

        let failures = self.failure_outcomes.as_deref().map(|rows|
            rows.iter().cloned().map(|row| super::CapturedFailureOutcome { report: None, ..row }).collect::<Vec<_>>());

        self.with_failure_outcomes(failures)
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

        if self.captured_values.iter().any(|row| row.report.is_some() || !row.report_payloads.is_empty()) {
            let rows = self.captured_values.iter().cloned().map(|row| {
                let report = row.report.as_deref().map(|requirements|
                    map(DependencyCallOutput::CompletionReport, requirements).map(sorted_unique_shared_slice)
                ).transpose()?;

                let payloads = row.report_payloads.iter().map(|payload| {
                    Ok(super::CapturedReportPayload {
                        destination: payload.destination.clone(),
                        requirements: sorted_unique_shared_slice(map(DependencyCallOutput::Value, &payload.requirements)?),
                        complete: payload.complete,
                    })
                }).collect::<Result<Vec<_>, E>>()?;

                Ok(row.with_report(report).with_report_payloads(payloads))
            }).collect::<Result<Vec<_>, E>>()?;

            result = result.with_captured_values(rows);
        }

        if let Some(rows) = self.failure_outcomes() {
            let rows = rows.iter().cloned().map(|row| {
                let report = row.report.as_deref().map(|requirements|
                    map(DependencyCallOutput::AbnormalReport, requirements).map(sorted_unique_shared_slice)
                ).transpose()?;

                Ok(super::CapturedFailureOutcome { report, ..row })
            }).collect::<Result<Vec<_>, E>>()?;

            result = result.with_failure_outcomes(Some(rows));
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
                self.captured_values.iter().flat_map(|row| row.report.iter().flat_map(|requirements| requirements.iter())),
            )
            .chain(self.captured_values.iter().flat_map(|row| row.report_payloads.iter())
                .flat_map(|payload| payload.requirements.iter()))
            .chain(
                self.failure_outcomes().into_iter().flatten().filter_map(|row| row.report.as_deref()).flatten(),
            )
            .chain(self.synchronous_cleanup.iter())
    }

    /// Enumerates paths needed to prepare references, effects and stored-value bounds.
    /// A stored-value path does not itself establish reference authority.
    pub fn reference_subjects(&self) -> impl Iterator<Item = &DependencySubject> {
        self.captured_values().iter().flat_map(|row| row.references.iter())
            .flat_map(|relation| {
                relation
                    .source_subject()
                    .into_iter()
                    .chain(std::iter::once(relation.exit()))
            })
            .chain(self.effects().iter().chain(self.cell_changes()).flat_map(DependencyStorageEffect::subjects))
            .chain(self.captured_values().iter().flat_map(|row| row.stored_values.iter()).flat_map(|stored| {
                std::iter::once(stored.destination().subject()).chain(stored.sources().iter().map(DependencyStoredValuePath::subject))
            }))
            .chain(self.captured_values().iter().flat_map(|outcome| {
                outcome.bindings.iter().map(|(subject, _)| subject).chain(outcome.inactive_payloads.iter())
            }))
            .chain(self.failure_outcomes().into_iter().flatten().flat_map(|row| row.writes.iter().map(|(subject, _)| subject)))
            .chain(self.captured_values().iter().flat_map(|row| row.report_payloads.iter())
                .map(|payload| &payload.destination))
            .chain(self.captured_values().iter().flat_map(|row| row.carriers.iter())
                .filter_map(DependencyWitnessCarrier::support).flat_map(super::DependencyWitnessSupport::owner_subjects))
    }

    /// Visits the exact cell changes present in a guarded normal or abnormal outcome.
    pub fn cell_changes(&self) -> impl Iterator<Item = &DependencyStorageEffect> {
        self.captured_values().iter().flat_map(|row| row.cell_changes.as_deref().into_iter().flatten())
            .chain(self.failure_outcomes().into_iter().flatten()
                .flat_map(|row| row.cell_changes.as_deref().into_iter().flatten()))
    }

    /// Visits the declared subjects of phase requirements, including their guards.
    pub fn requirement_subjects(&self) -> impl Iterator<Item = &DependencySubject> {
        let mut pending = self.requirements().iter().rev().collect::<Vec<_>>();

        std::iter::from_fn(move || loop {
            match pending.pop()? {
                DependencyRequirement::Direct { subject, .. } => return Some(subject),
                DependencyRequirement::Guarded(guarded) => {
                    pending.extend(guarded.requirements().iter().rev());

                    return Some(match guarded.guard() {
                        DependencyGuard::NullablePresent(subject) | DependencyGuard::ActiveUnionVariant { subject, .. } => subject,
                    });
                }
                _ => {}
            }
        })
    }

    /// Returns requirements for one selected output.
    /// Absence and an independent report both have no lifetime requirements.
    pub fn output_requirements(&self, output: DependencyCallOutput) -> Arc<[DependencyRequirement]> {
        match output {
            DependencyCallOutput::Value => Arc::clone(&self.requirements),
            DependencyCallOutput::CompletionReport => self.completion_reports().unwrap_or_default(),
            DependencyCallOutput::AbnormalReport => self.abnormal_reports().unwrap_or_default(),
            DependencyCallOutput::SynchronousCleanup => Arc::clone(&self.synchronous_cleanup),
        }
    }

    /// Creates a template after deterministic sorting and deduplication.
    pub fn new(requirements: impl IntoIterator<Item = DependencyRequirement>) -> Self {
        Self {
            requirements: sorted_unique_shared_slice(requirements),
            failure_outcomes: None,
            captured_values: Arc::new([]),
            effects: super::DependencyEffectOutcomes::new(DependencyStorageEffect::opaque_call()),
            synchronous_cleanup: Arc::new([]),
            normal_completion: true,
        }
    }

    /// Replaces requirements while retaining the checked reference, effect and completion summary.
    pub fn with_requirements(
        &self,
        requirements: impl IntoIterator<Item = DependencyRequirement>,
    ) -> Self {
        Self {
            requirements: sorted_unique_shared_slice(requirements),
            failure_outcomes: self.failure_outcomes.as_ref().map(Arc::clone),
            // Transformations own a new contract while retaining the immutable checked summary.
            captured_values: Arc::clone(&self.captured_values),
            effects: self.effects.clone(),
            synchronous_cleanup: Arc::clone(&self.synchronous_cleanup),
            normal_completion: self.normal_completion,
        }
    }

    /// Guarantees supported by the same returned owner on every normal exit.
    pub fn common_carriers(&self) -> Arc<[DependencyWitnessCarrier]> {
        super::CapturedValueOutcome::common_carriers(&self.captured_values)
    }

    /// Records whether the checked body can produce a normal result.
    pub fn with_normal_completion(mut self, possible: bool) -> Self {
        self.normal_completion = possible;

        self.refresh_normal_effects()
    }

    /// False proves that execution has no normal result. Unknown callees default to true.
    pub const fn normal_completion(&self) -> bool {
        self.normal_completion
    }

    fn refresh_normal_effects(mut self) -> Self {
        let mut effects = self.effects.with_unselected_outcomes();

        if !self.normal_completion {
            effects = effects.with_normal([]);
        } else if !self.captured_values.is_empty() {
            effects = effects.with_normal(self.captured_values.iter().flat_map(|row| row.effects.iter().cloned()));
        }

        if let Some(rows) = self.failure_outcomes()
            && rows.iter().all(|row| row.effects.is_some())
        {
            effects = effects.with_abnormal(rows.iter().filter_map(|row| row.effects.as_deref()).flatten().cloned());
        }

        self.effects = effects;

        self
    }

    /// Whether normal return preserves storage, independently of input ownership transfers.
    pub fn preserves_storage_on_normal_return(&self) -> bool {
        self.effects.preserves_storage_on_normal_return()
    }

    /// Derives established origins before selecting a normal outcome.
    pub fn borrows(&self) -> Arc<[DependencyBorrowRelation]> {
        super::CapturedValueOutcome::reference_origins(self.captured_values.iter(), DependencySubject::contains)
    }

    /// Attaches the conservative transition envelope, retaining every normal row.
    pub fn with_possible_effects(mut self, effects: impl IntoIterator<Item = DependencyStorageEffect>) -> Self {
        self.effects = super::DependencyEffectOutcomes::new(DependencyStorageEffect::normalize(effects).iter().cloned());

        self.refresh_normal_effects()
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
    pub fn effects(&self) -> &[DependencyStorageEffect] {
        self.effects.possible()
    }

    /// Returns possible transitions on normal completion.
    pub fn normal_effects(&self) -> &[DependencyStorageEffect] { self.effects.normal() }

    /// Returns the connected effect contract for every completion boundary.
    pub const fn effect_outcomes(&self) -> &super::DependencyEffectOutcomes<DependencyStorageEffect> { &self.effects }

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
            .expect("fixture map requirements must succeed");

        assert_eq!(mapped, template);
        assert_eq!(mapped.all_requirements().collect::<Vec<_>>(), [&input]);

        assert_ne!(
            DependencyContractTemplateData::new([]),
            DependencyContractTemplateData::new([]).with_completion_reports([])
        );

        assert!(
            DependencyContractTemplateData::new([])
                .try_map_requirements::<()>(|_, _| Ok(Vec::new()))
                .expect("fixture map requirements must succeed")
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

        let write = crate::DependencyStorageEffect::Direct { addressed: true,
            subject: subject.clone(),
            kind: crate::DependencyMutationKind::Write,
            access_type: None,
        };

        let template = DependencyContractTemplateData::new([])
            .with_possible_effects([write.clone()])
            .with_captured_values([crate::CapturedValueOutcome::new(None, [])
                .with_effects([write]).with_storage([relation], [])])
            .with_completion_reports([])
            .with_abnormal_reports([DependencyRequirement::direct(
                subject.clone(),
                DependencyRequirementKind::StorageAlive,
            )]);

        let requirement =
            DependencyRequirement::direct(subject, DependencyRequirementKind::StorageAlive);

        let changed = template.with_requirements([requirement.clone(), requirement.clone()]);

        assert_eq!(changed.requirements(), &[requirement]);
        assert_eq!(changed.borrows(), template.borrows());
        assert_eq!(changed.effects(), template.effects());
        assert_eq!(changed.completion_reports().as_deref(), Some([].as_slice()));
        assert_eq!(changed.abnormal_reports(), template.abnormal_reports());
        assert!(changed.normal_completion());
        assert!(template.requirements().is_empty());
    }
}
