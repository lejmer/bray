use std::sync::Arc;

use super::{ConstantTermId, DependencyProjection};

/// A value operation whose operands are evaluated snapshots, not possible regions.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CapturedValueOperation<Projection = DependencyProjection> {
    /// Select current data from an already captured value.
    Select(Projection),
    /// An initialized prefix owned through the checked descriptor field.
    OwningStorage(Projection),
    /// Read current contents through an admitted reference at the execution boundary.
    Read,
    /// A captured reference descriptor and its current reached observation.
    Reference,
    /// Current data belonging to an actual owned value identity.
    Owned,
    /// Copied data observed independently of ownership of its source.
    Scalar,
    /// Retain ordinary inline data while reached and protected observations remain independent.
    Inline,
    /// An explicitly present nullable containing the same captured payload value.
    Present,
    /// Ordinary checked scalar addition.
    Add,
    /// Ordinary checked scalar subtraction.
    Subtract,
    /// An initialized prefix containing no values or cleanup duties.
    EmptyPrefix,
    /// Transfer one complete value into the end of an initialized prefix.
    Append,
    /// Transfer the last initialized value out of a nonempty prefix.
    Last,
    /// Retain the prefix after its last value has been transferred out.
    Remainder,
}

impl<Projection> CapturedValueOperation<Projection> {
    /// Number of actual evaluated operands required by this operation.
    pub const fn operand_count(&self) -> usize {
        match self {
            Self::EmptyPrefix => 0,
            Self::Append | Self::Reference | Self::Owned | Self::Scalar | Self::Add | Self::Subtract => 2,
            Self::Select(_) | Self::OwningStorage(_) | Self::Read | Self::Inline | Self::Present | Self::Last | Self::Remainder => 1,
        }
    }
}

/// The checked dispatch selection held by a deferred invocation, independent of its evaluated arguments.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CapturedCallable<Callable = crate::CallableInstanceId, Type = crate::TypeId,
    Requirement = crate::ImplementationRequirementKey, Witness = crate::ImplementationInstanceId> {
    /// Selected declaration and generic substitution.
    pub instance: Callable,
    /// Instantiated callable signature of the selected declaration.
    pub signature: Type,
    /// The checked receiver type when capture zero holds a receiver.
    pub receiver: Option<Type>,
    /// Generic constraint providing dispatch, when applicable.
    pub requirement: Option<Requirement>,
    /// Implementation selected for this call.
    pub dispatch: Option<Witness>,
    /// Other implementations required by the substituted body.
    pub witnesses: Arc<[Witness]>,
}

/// An immutable portable expression for a whole value and its owning children.
/// Input nodes name actual entry values. They never denote may-dependency bounds.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CapturedValueTemplate<Input = ConstantTermId, Projection = DependencyProjection, Static = crate::StaticReferenceSelection,
    Callee = CapturedCallable, Variant = crate::UnionVariantSymbolId> {
    /// The actual value bound to a receiver-first formal term.
    Input(Input),
    /// Ordinary scalar data, with no ownership or cleanup duty from its inputs.
    Data(Input),
    /// The exact selected static instance, including its substitution, witnesses and target.
    Static(Static),
    /// Feasible lazy invocations, each retaining its own guard and actual environment.
    Deferred {
        /// Complete selected environments. Callees do not supply runtime allocation identity.
        alternatives: Arc<[CapturedDeferredInvocation<Input, Projection, Static, Callee, Variant>]>,
        /// An opaque producer is also feasible. This does not assert an empty capture set.
        unknown: bool,
    },
    /// An independently produced value with no asserted relation to an input.
    /// Unknown contents do not mean an empty initialized prefix.
    Unknown,
    /// Evaluated whole-value storage operation.
    Operation(CapturedValueOperation<Projection>, Arc<[Self]>),
    /// A constructed union carries its actual discriminant with its payload.
    Tagged(Variant, Arc<Self>),
    /// Replace actual children while retaining the rest of the captured base.
    Aggregate {
        /// The evaluated base value, or explicit unknown for a fresh aggregate.
        base: Arc<Self>,
        /// Actual replacement children.
        fields: Arc<[(Projection, Self)]>,
    },
}

/// One guarded latent invocation over the complete values captured at formation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CapturedDeferredInvocation<Input = ConstantTermId, Projection = DependencyProjection, Static = crate::StaticReferenceSelection,
    Callee = CapturedCallable, Variant = crate::UnionVariantSymbolId> {
    /// Ordinary selection facts in the containing value's entry scope.
    pub guard: Option<Input>,
    /// Exact semantic callee and witnesses. Formal arguments index this environment.
    pub call: Option<Arc<Callee>>,
    /// Complete captured inputs in receiver-first order.
    pub captures: Arc<[CapturedValueTemplate<Input, Projection, Static, Callee, Variant>]>,
    /// Captures whose ownership and cleanup duties belong to this frame.
    pub owned_inputs: Arc<[crate::SymbolOrdinal]>,
}

impl<Input, Projection, Static, Callee, Variant> CapturedValueTemplate<Input, Projection, Static, Callee, Variant> {
    /// Visits shared expression structure in deterministic operand order.
    pub fn nodes(&self) -> impl Iterator<Item = &Self> {
        let mut pending = vec![self];

        std::iter::from_fn(move || {
            let node = pending.pop()?;

            match node {
                Self::Operation(_, values) => pending.extend(values.iter().rev()),
                Self::Tagged(_, value) => pending.push(value),
                Self::Aggregate { base, fields } => {
                    pending.extend(fields.iter().rev().map(|(_, value)| value));
                    pending.push(base);
                }
                Self::Deferred { alternatives, .. } => pending.extend(alternatives.iter().rev()
                    .flat_map(|alternative| alternative.captures.iter().rev())),
                Self::Input(_) | Self::Data(_) | Self::Static(_) | Self::Unknown => {}
            }

            Some(node)
        })
    }

    /// Visits selectors retained by this actual value expression.
    pub fn projections(&self) -> impl Iterator<Item = &Projection> {
        self.nodes().flat_map(|node| {
            let (selected, fields) = match node {
                Self::Operation(CapturedValueOperation::Select(field) | CapturedValueOperation::OwningStorage(field), _) => (Some(field), &[][..]),
                Self::Aggregate { fields, .. } => (None, fields.as_ref()),
                _ => (None, &[][..]),
            };

            selected.into_iter().chain(fields.iter().map(|(field, _)| field))
        })
    }

    /// Visits the formal terms needed to bind this immutable expression.
    pub fn inputs(&self) -> impl Iterator<Item = &Input> {
        self.nodes().filter_map(|node| match node { Self::Input(input) => Some(input), _ => None })
    }

    /// Visits all term references, retaining the distinction between values and scalar data.
    pub fn terms(&self) -> impl Iterator<Item = &Input> {
        self.nodes().flat_map(|node| {
            let (term, alternatives) = match node {
                Self::Input(input) | Self::Data(input) => (Some(input), &[][..]),
                Self::Deferred { alternatives, .. } => (None, alternatives.as_ref()),
                _ => (None, &[][..]),
            };

            term.into_iter().chain(alternatives.iter().filter_map(|alternative| alternative.guard.as_ref()))
        })
    }

    /// Visits selected deferred callables independently from the captured argument terms.
    pub fn callees(&self) -> impl Iterator<Item = &Callee> {
        self.nodes().flat_map(|node| match node {
            Self::Deferred { alternatives, .. } => alternatives.as_ref(),
            _ => &[],
        }).filter_map(|alternative| alternative.call.as_deref())
    }

    /// Visits exact selected static roots without reducing them to declaration regions.
    pub fn static_roots(&self) -> impl Iterator<Item = &Static> {
        self.nodes().filter_map(|node| match node { Self::Static(root) => Some(root), _ => None })
    }

    /// Visits the actual union discriminants carried by this value expression.
    pub fn variants(&self) -> impl Iterator<Item = &Variant> {
        self.nodes().filter_map(|node| match node { Self::Tagged(variant, _) => Some(variant), _ => None })
    }

    /// Remaps portable identities while preserving the complete value expression.
    pub fn try_map<E, OtherInput, OtherProjection, OtherStatic, OtherCallee, OtherVariant>(&self, input: &mut impl FnMut(&Input) -> Result<OtherInput, E>,
        projection: &mut impl FnMut(&Projection) -> Result<OtherProjection, E>,
        static_root: &mut impl FnMut(&Static) -> Result<OtherStatic, E>,
        callee: &mut impl FnMut(&Callee) -> Result<OtherCallee, E>,
        variant: &mut impl FnMut(&Variant) -> Result<OtherVariant, E>,
    ) -> Result<CapturedValueTemplate<OtherInput, OtherProjection, OtherStatic, OtherCallee, OtherVariant>, E> {
        use CapturedValueTemplate as Value;

        Ok(match self {
            Self::Input(value) => Value::Input(input(value)?),
            Self::Data(value) => Value::Data(input(value)?),
            Self::Static(value) => Value::Static(static_root(value)?),
            Self::Deferred { alternatives, unknown } => Value::Deferred {
                alternatives: alternatives.iter().map(|alternative| Ok(CapturedDeferredInvocation {
                    guard: alternative.guard.as_ref().map(&mut *input).transpose()?,
                    call: alternative.call.as_deref().map(&mut *callee).transpose()?.map(Arc::new),
                    captures: alternative.captures.iter().map(|value| value.try_map(input, projection, static_root, callee, variant)).collect::<Result<_, E>>()?,
                    owned_inputs: Arc::clone(&alternative.owned_inputs),
                })).collect::<Result<_, E>>()?,
                unknown: *unknown,
            },
            Self::Unknown => Value::Unknown,
            Self::Tagged(tag, value) => Value::Tagged(variant(tag)?,
                Arc::new(value.try_map(input, projection, static_root, callee, variant)?)),
            Self::Operation(operation, values) => Value::Operation(match operation {
                CapturedValueOperation::Select(field) => CapturedValueOperation::Select(projection(field)?),
                CapturedValueOperation::OwningStorage(field) => CapturedValueOperation::OwningStorage(projection(field)?),
                CapturedValueOperation::Read => CapturedValueOperation::Read,
                CapturedValueOperation::Reference => CapturedValueOperation::Reference,
                CapturedValueOperation::Owned => CapturedValueOperation::Owned,
                CapturedValueOperation::Scalar => CapturedValueOperation::Scalar,
                CapturedValueOperation::Inline => CapturedValueOperation::Inline,
                CapturedValueOperation::Present => CapturedValueOperation::Present,
                CapturedValueOperation::Add => CapturedValueOperation::Add,
                CapturedValueOperation::Subtract => CapturedValueOperation::Subtract,
                CapturedValueOperation::EmptyPrefix => CapturedValueOperation::EmptyPrefix,
                CapturedValueOperation::Append => CapturedValueOperation::Append,
                CapturedValueOperation::Last => CapturedValueOperation::Last,
                CapturedValueOperation::Remainder => CapturedValueOperation::Remainder,
            }, values.iter().map(|value| value.try_map(input, projection, static_root, callee, variant)).collect::<Result<_, _>>()?),
            Self::Aggregate { base, fields } => Value::Aggregate {
                base: Arc::new(base.try_map(input, projection, static_root, callee, variant)?),
                fields: fields.iter().map(|(field, value)|
                    Ok((projection(field)?, value.try_map(input, projection, static_root, callee, variant)?))).collect::<Result<_, E>>()?,
            },
        })
    }
}

/// The transfer that ends a callable without publishing its normal result.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum DependencyFailureKind {
    /// Propagation of a panic report.
    Panic = 1,
    /// Completion of current-run cancellation.
    Cancellation = 2,
}

impl DependencyFailureKind {
    /// Both ways of leaving execution without a normal result.
    pub const ALL: [Self; 2] = [Self::Panic, Self::Cancellation];

    /// Its position in the complete set of failure channels.
    pub const fn index(self) -> usize {
        match self {
            Self::Panic => 0,
            Self::Cancellation => 1,
        }
    }
}

/// Performed effects and the report produced together on one abnormal exit.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CapturedFailureOutcome<Guard = ConstantTermId, Report = Arc<[super::DependencyRequirement]>,
    Effect = super::DependencyStorageEffect, Write = (super::DependencySubject, CapturedValueTemplate),
    Subject = super::DependencySubject, Projection = super::DependencyProjection> {
    /// The actual exit kind, or an opaque summary covering either kind.
    pub kind: Option<DependencyFailureKind>,
    /// Entry condition for this exit, independent of any normal result.
    pub guard: Option<Guard>,
    /// Performed effect bound. Absence uses the callable's conservative envelope.
    pub effects: Option<Arc<[Effect]>>,
    /// Exact typed cell updates established on this exit.
    pub cell_changes: Option<Arc<[Effect]>>,
    /// Values installed in caller storage before this failure. No normal result is published.
    pub writes: Arc<[Write]>,
    /// Reference descriptors installed by the same abnormal writes.
    pub references: Arc<[super::DependencyBorrowRelation<Subject, Projection>]>,
    /// Remaining physical dependencies installed by the same abnormal writes.
    pub stored_values: Arc<[super::DependencyStoredValue<Subject>]>,
    /// Report dependencies. An empty present list describes an independent report.
    pub report: Option<Report>,
}

impl<Guard, Report, Effect, Write, Subject, Projection>
    CapturedFailureOutcome<Guard, Report, Effect, Write, Subject, Projection>
{
    /// An opaque outcome conservatively covers any abnormal exit.
    pub fn covers(&self, kind: DependencyFailureKind) -> bool {
        self.kind.is_none_or(|actual| actual == kind)
    }

    /// Attaches the physical values established by this abnormal exit.
    pub fn with_storage(
        mut self,
        references: impl IntoIterator<Item = super::DependencyBorrowRelation<Subject, Projection>>,
        stored_values: impl IntoIterator<Item = super::DependencyStoredValue<Subject>>,
    ) -> Self
    where
        Subject: Clone + Ord,
        Projection: Ord,
    {
        self.references = normalize_references(references);
        self.stored_values = super::DependencyStoredValue::normalize(stored_values);

        self
    }

    /// Joins the established abnormal reference origins before one failure row is selected.
    pub fn reference_origins<'a>(
        rows: impl IntoIterator<Item = &'a Self>,
        contains: impl Fn(&Subject, &Subject) -> bool,
    ) -> Arc<[super::DependencyBorrowRelation<Subject, Projection>]>
    where
        Self: 'a,
        Subject: Clone + Ord + 'a,
        Projection: Clone + Ord + 'a,
    {
        reference_origins(rows, contains)
    }

    /// Joins physical dependencies installed by the selected abnormal writes.
    pub fn stored_dependencies<'a>(
        rows: impl IntoIterator<Item = &'a Self>,
        contains: impl Fn(&Subject, &Subject) -> bool,
    ) -> Arc<[super::DependencyStoredValue<Subject>]>
    where
        Self: 'a,
        Subject: Clone + Ord + 'a,
        Projection: 'a,
    {
        stored_dependencies(rows, contains)
    }
}

impl CapturedFailureOutcome {
    /// Widens recursive failure writes while preserving distinct performed prefixes and reports.
    pub fn join_reachable(left: &[Self], right: &[Self]) -> Vec<Self> {
        if left == right { return left.to_vec(); }

        let mut outcomes = std::collections::BTreeMap::new();

        for row in left.iter().chain(right) {
            let key = (row.kind, row.guard, row.effects.clone(), row.cell_changes.clone(), row.report.clone());

            outcomes.entry(key).and_modify(|known: &mut Self| {
                if known.writes != row.writes {
                    known.writes = common_bindings([known.writes.as_ref(), row.writes.as_ref()].into_iter(),
                        &CapturedValueTemplate::Unknown);
                }

                if known.references != row.references {
                    known.references = reference_origins([&*known, row], PartialEq::eq);
                }

                if known.stored_values != row.stored_values {
                    known.stored_values = stored_dependencies([&*known, row], PartialEq::eq);
                }
            }).or_insert_with(|| row.clone());
        }

        outcomes.into_values().collect()
    }

}

/// The dependencies of a report held in the produced value at a normal exit.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CapturedReportPayload<Subject = super::DependencySubject, Report = super::DependencyRequirement> {
    /// Checked storage path of the report in the produced value.
    pub destination: Subject,
    /// Portable requirements retained by that payload.
    pub requirements: Arc<[Report]>,
    /// Every possible dependency of this payload is represented.
    pub complete: bool,
}

impl<Subject, Report> CapturedReportPayload<Subject, Report> {
    /// Remaps the produced storage path and its portable requirements together.
    pub fn try_map<Context, OtherSubject, OtherReport, E>(
        &self,
        context: &mut Context,
        destination: impl FnOnce(&mut Context, &Subject) -> Result<OtherSubject, E>,
        requirements: impl FnOnce(&mut Context, &[Report]) -> Result<Arc<[OtherReport]>, E>,
    ) -> Result<CapturedReportPayload<OtherSubject, OtherReport>, E> {
        Ok(CapturedReportPayload {
            destination: destination(context, &self.destination)?,
            requirements: requirements(context, &self.requirements)?,
            complete: self.complete,
        })
    }
}

/// Values produced together on one feasible normal exit.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CapturedValueOutcome<Subject = super::DependencySubject, Input = ConstantTermId, Projection = DependencyProjection,
    Static = crate::StaticReferenceSelection, Guard = Input, OwnedProjection = super::ConstantProjectionKind, Callee = CapturedCallable, Variant = crate::UnionVariantSymbolId,
    Report = super::DependencyRequirement, Effect = super::DependencyStorageEffect, WitnessSubject = super::DependencySubject> {
    /// The report produced on this exit, including any storage dependencies it carries.
    pub report: Option<Arc<[Report]>>,
    /// Explicit report payloads held in this exit's produced values.
    pub report_payloads: Arc<[CapturedReportPayload<Subject, Report>]>,
    /// Possible transitions on paths reaching this exit.
    pub effects: Arc<[Effect]>,
    /// Exact typed cell updates established on this exit.
    pub cell_changes: Option<Arc<[Effect]>>,
    /// Checked guarantees and the returned owners supporting them on this exit.
    pub carriers: Arc<[super::DependencyWitnessCarrier<Projection, Guard, WitnessSubject>]>,
    /// Transfers of existing input ownership on this exit, sharing its guard and result.
    pub ownership: super::DependencyOwnedResult<OwnedProjection, Guard>,
    /// Reference descriptors and their actual origins on this exit.
    pub references: Arc<[super::DependencyBorrowRelation<Subject, Projection>]>,
    /// Remaining storage dependencies and their completeness on this exit.
    pub stored_values: Arc<[super::DependencyStoredValue<Subject>]>,
    /// Payload subtrees known to be inactive on this exit.
    pub inactive_payloads: Arc<[Subject]>,
    /// Ordinary entry or result-shape condition. An absent guard is feasible unconditionally.
    pub guard: Option<Guard>,
    /// Writes established on this exit. An unknown value still establishes a write.
    /// An omitted destination supplies no write guarantee.
    pub bindings: Arc<[(Subject, CapturedValueTemplate<Input, Projection, Static, Callee, Variant>)]>,
}

impl CapturedValueOutcome {
    /// Whole contents established for a callable root by this outcome.
    pub fn root_binding(&self, root: super::DependencySubjectRoot) -> Option<&CapturedValueTemplate> {
        self.bindings.iter().find_map(|(subject, value)| {
            (subject.subject_root() == root && subject.projections().is_empty()).then_some(value)
        })
    }

    /// Exact tag carried by a produced union value, when the row establishes it.
    pub fn result_variant(&self) -> Option<crate::UnionVariantSymbolId> {
        let mut value = self.root_binding(super::DependencySubjectRoot::Result)?;

        loop {
            match value {
                CapturedValueTemplate::Tagged(variant, _) => return Some(*variant),
                CapturedValueTemplate::Aggregate { base, .. } => value = base,
                CapturedValueTemplate::Operation(CapturedValueOperation::Owned | CapturedValueOperation::Scalar, inputs) => value = &inputs[1],
                CapturedValueTemplate::Operation(CapturedValueOperation::Inline, inputs) => value = &inputs[0],
                _ => return None,
            }
        }
    }
}

impl<Subject, Input, Projection, Static, Guard, OwnedProjection, Callee: Eq, Variant: Eq, Report, Effect, WitnessSubject> CapturedValueOutcome<Subject, Input, Projection, Static, Guard, OwnedProjection, Callee, Variant, Report, Effect, WitnessSubject>
where Subject: Ord, Input: Eq, Projection: Eq, Static: Eq {
    /// Retains the report accompanying this normal result.
    pub fn with_report(mut self, report: Option<Arc<[Report]>>) -> Self {
        self.report = report;

        self
    }

    /// Retains the selected result's actual report payloads.
    pub fn with_report_payloads(mut self, payloads: impl IntoIterator<Item = CapturedReportPayload<Subject, Report>>) -> Self
    where Report: Clone + Ord {
        let mut payloads = payloads.into_iter().collect::<Vec<_>>();

        payloads.sort_unstable_by(|left, right| left.destination.cmp(&right.destination));

        payloads.dedup_by(|next, current| {
            if next.destination != current.destination {
                return false;
            }

            current.requirements = bray_base::sorted_unique_shared_slice(
                current.requirements.iter().chain(next.requirements.iter()).cloned(),
            );

            current.complete &= next.complete;

            true
        });

        self.report_payloads = payloads.into();

        self
    }

    /// Records exact typed cell changes that survive this outcome.
    pub fn with_cell_changes(mut self, cell_changes: Option<Arc<[Effect]>>) -> Self {
        self.cell_changes = cell_changes;

        self
    }

    /// Records the possible transition prefix of this exit.
    pub fn with_effects(mut self, effects: impl IntoIterator<Item = Effect>) -> Self
    where Effect: Ord {
        self.effects = bray_base::sorted_unique_shared_slice(effects);

        self
    }

    /// Attaches only guarantees established by this exit.
    pub fn with_carriers(mut self, carriers: impl IntoIterator<Item = super::DependencyWitnessCarrier<Projection, Guard, WitnessSubject>>) -> Self
    where Projection: Ord, Guard: Ord, WitnessSubject: Ord {
        self.carriers = bray_base::sorted_unique_shared_slice(carriers);

        self
    }

    /// Attaches the complete ownership layout established by this exit.
    pub fn with_ownership(mut self, ownership: super::DependencyOwnedResult<OwnedProjection, Guard>) -> Self {
        self.ownership = ownership;

        self
    }

    /// Attaches the same exit's resolved physical observations.
    pub fn with_storage(
        mut self,
        references: impl IntoIterator<Item = super::DependencyBorrowRelation<Subject, Projection>>,
        stored_values: impl IntoIterator<Item = super::DependencyStoredValue<Subject>>,
    ) -> Self
    where
        Subject: Clone,
        Projection: Ord,
    {
        self.references = normalize_references(references);
        self.stored_values = super::DependencyStoredValue::normalize(stored_values);

        self
    }

    /// Records inactive payloads directly from the producing exit.
    pub fn with_inactive_payloads(mut self, payloads: impl IntoIterator<Item = Subject>) -> Self {
        self.inactive_payloads = bray_base::sorted_unique_shared_slice(payloads);

        self
    }

    /// Binds all writes of one exit. Aliased destinations retain only a common value.
    pub fn new(guard: Option<Guard>, bindings: impl IntoIterator<Item = (Subject, CapturedValueTemplate<Input, Projection, Static, Callee, Variant>)>) -> Self {
        let mut writes = std::collections::BTreeMap::new();

        for (destination, value) in bindings {
            match writes.entry(destination) {
                std::collections::btree_map::Entry::Vacant(entry) => { entry.insert(value); }
                std::collections::btree_map::Entry::Occupied(mut entry) if entry.get() != &value => {
                    entry.insert(CapturedValueTemplate::Unknown);
                }
                _ => {}
            }
        }

        Self {
            guard,
            report: None,
            report_payloads: Arc::default(),
            effects: Arc::default(),
            cell_changes: None,
            carriers: Arc::default(),
            bindings: writes.into_iter().collect(),
            ownership: Default::default(),
            references: Arc::default(),
            stored_values: Arc::default(),
            inactive_payloads: Arc::default(),
        }
    }
}

impl<Subject: Ord + Clone, Input: Ord + Clone, Projection: Ord + Clone, Static: Ord + Clone, Guard: Ord + Copy, OwnedProjection: Ord + Clone, Callee: Ord + Clone, Variant: Ord + Clone, Report: Ord + Clone, Effect: Ord + Clone, WitnessSubject: Ord + Clone>
    CapturedValueOutcome<Subject, Input, Projection, Static, Guard, OwnedProjection, Callee, Variant, Report, Effect, WitnessSubject>
{
    /// Guarantees every listed exit establishes on the same owner.
    pub fn common_carriers(rows: &[Self]) -> Arc<[super::DependencyWitnessCarrier<Projection, Guard, WitnessSubject>]> {
        super::DependencyWitnessCarrier::common(rows.iter().map(|row| &row.carriers))
    }

    /// Returns ownership across all normal exits for consumers operating before exit selection.
    pub fn ownership_layout(rows: &[Self]) -> super::DependencyOwnedResult<OwnedProjection, Guard> {
        super::DependencyOwnedResult::choice(rows.iter().map(|row| match &row.guard {
            Some(guard) => super::DependencyOwnedResult::case(guard.clone(), row.ownership.clone()),
            None => row.ownership.clone(),
        }))
    }

    /// Joins the established reference origins for callers that cannot select an exit.
    pub fn reference_origins<'a>(rows: impl IntoIterator<Item = &'a Self>, contains: impl Fn(&Subject, &Subject) -> bool) -> Arc<[super::DependencyBorrowRelation<Subject, Projection>]>
    where Self: 'a {
        reference_origins(rows, contains)
    }

    /// Joins known stored dependencies without treating a missing active slot as independent.
    pub fn stored_dependencies<'a>(rows: impl IntoIterator<Item = &'a Self>, contains: impl Fn(&Subject, &Subject) -> bool) -> Arc<[super::DependencyStoredValue<Subject>]>
    where Self: 'a {
        stored_dependencies(rows, contains)
    }

    /// Retains inactive subtrees established by every feasible normal exit.
    pub fn common_inactive_payloads<'a>(rows: impl IntoIterator<Item = &'a Self>, contains: impl Fn(&Subject, &Subject) -> bool) -> Arc<[Subject]>
    where Self: 'a {
        let mut rows = rows.into_iter();

        let Some(first) = rows.next() else { return Arc::new([]); };

        let Some(second) = rows.next() else { return Arc::clone(&first.inactive_payloads); };

        let rows = [first, second].into_iter().chain(rows).collect::<Vec<_>>();

        bray_base::sorted_unique_shared_slice(rows.iter().flat_map(|row| row.inactive_payloads.iter())
            .filter(|payload| rows.iter().all(|row| row.inactive_payloads.iter().any(|ancestor| contains(ancestor, payload))))
            .cloned())
    }

    /// Joins possible reports across normal exits, retaining independent reports.
    pub fn possible_report<'a>(rows: impl IntoIterator<Item = &'a Self>) -> Option<Arc<[Report]>>
    where Self: 'a {
        let mut reports = rows.into_iter().filter_map(|row| row.report.as_ref());
        let first = reports.next()?;

        if let Some(second) = reports.next() {
            Some(bray_base::sorted_unique_shared_slice(first.iter().chain(second.iter())
                .chain(reports.flat_map(|report| report.iter())).cloned()))
        } else {
            Some(Arc::clone(first))
        }
    }

    /// Whether this row accounts for all support of a report at the checked path.
    pub fn accounts_for_report_payload(&self, destination: &Subject, contains: impl Fn(&Subject, &Subject) -> bool) -> bool {
        match self.report_payloads.binary_search_by(|payload| payload.destination.cmp(destination)) {
            Ok(index) => self.report_payloads[index].complete,
            Err(_) => self.inactive_payloads.iter().any(|payload| contains(payload, destination)),
        }
    }

    fn joined_report_payloads(rows: &[Self], contains: impl Fn(&Subject, &Subject) -> bool)
        -> Arc<[CapturedReportPayload<Subject, Report>]> {
        use std::collections::{BTreeMap, BTreeSet};

        let mut payloads = BTreeMap::<_, BTreeSet<_>>::new();

        for row in rows {
            for payload in &*row.report_payloads {
                payloads.entry(&payload.destination).or_default()
                    .extend(payload.requirements.iter().cloned());
            }
        }

        payloads.into_iter().map(|(destination, requirements)| {
            let complete = rows.iter().all(|row| row.accounts_for_report_payload(destination, &contains));

            CapturedReportPayload { destination: destination.clone(), requirements: requirements.into_iter().collect(), complete }
        }).collect()
    }

    /// Widens successive approximations of reachable exits at a fixed point.
    /// An unspecified normal outcome supplies no guaranteed writes.
    pub fn join_reachable(left: &[Self], right: &[Self], contains: impl Fn(&Subject, &Subject) -> bool) -> Arc<[Self]> {
        if left == right { return left.into(); }

        let opaque = Self::new(None, []);
        let rows = |side: &[Self]| if side.is_empty() { vec![opaque.clone()] } else { side.to_vec() };
        let mut groups = std::collections::BTreeMap::<_, (Vec<Self>, Vec<Self>)>::new();

        for row in rows(left) { groups.entry(row.guard.clone()).or_default().0.push(row); }
        for row in rows(right) { groups.entry(row.guard.clone()).or_default().1.push(row); }

        let mut result = Vec::new();

        for (guard, (mut old, new)) in groups {
            if old.is_empty() { result.extend(new); }
            else if new.is_empty() || old == new { result.extend(old); }
            else {
                old.extend(new);

                let bindings = common_bindings(old.iter().map(|row| row.bindings.as_ref()), &CapturedValueTemplate::Unknown);

                let carriers = Self::common_carriers(&old);
                let references = Self::reference_origins(&old, &contains);
                let stored_values = Self::stored_dependencies(&old, &contains);

                let inactive_payloads = Self::common_inactive_payloads(&old, &contains);
                let report = Self::possible_report(&old);
                let report_payloads = Self::joined_report_payloads(&old, &contains);
                let effects = bray_base::sorted_unique_shared_slice(old.iter().flat_map(|row| row.effects.iter().cloned()));
                let cell_changes = common_cell_changes(old.iter().map(|row| row.cell_changes.as_ref()));
                let ownership = super::DependencyOwnedResult::choice(old.into_iter().map(|row| row.ownership));

                result.push(Self { guard, report, report_payloads, effects, cell_changes, bindings, ownership, carriers, references, stored_values, inactive_payloads });
            }
        }

        result.into()
    }
}

trait CapturedStorageRow<Subject, Projection> {
    fn references(&self) -> &[super::DependencyBorrowRelation<Subject, Projection>];
    fn stored_values(&self) -> &[super::DependencyStoredValue<Subject>];
    fn inactive_payloads(&self) -> &[Subject];
}

impl<Subject, Input, Projection, Static, Guard, OwnedProjection, Callee, Variant, Report, Effect, WitnessSubject>
    CapturedStorageRow<Subject, Projection>
    for CapturedValueOutcome<Subject, Input, Projection, Static, Guard, OwnedProjection, Callee, Variant, Report, Effect, WitnessSubject>
{
    fn references(&self) -> &[super::DependencyBorrowRelation<Subject, Projection>] {
        &self.references
    }

    fn stored_values(&self) -> &[super::DependencyStoredValue<Subject>] {
        &self.stored_values
    }

    fn inactive_payloads(&self) -> &[Subject] {
        &self.inactive_payloads
    }
}

impl<Guard, Report, Effect, Write, Subject, Projection> CapturedStorageRow<Subject, Projection>
    for CapturedFailureOutcome<Guard, Report, Effect, Write, Subject, Projection>
{
    fn references(&self) -> &[super::DependencyBorrowRelation<Subject, Projection>] {
        &self.references
    }

    fn stored_values(&self) -> &[super::DependencyStoredValue<Subject>] {
        &self.stored_values
    }

    fn inactive_payloads(&self) -> &[Subject] {
        &[]
    }
}

fn normalize_references<Subject: Ord, Projection: Ord>(
    references: impl IntoIterator<Item = super::DependencyBorrowRelation<Subject, Projection>>,
) -> Arc<[super::DependencyBorrowRelation<Subject, Projection>]> {
    let mut strongest = std::collections::BTreeMap::new();

    for relation in references {
        let provenance = relation.provenance();
        let key = relation.with_provenance(super::DependencyReferenceProvenance::Alternatives);

        strongest.entry(key).and_modify(|current: &mut super::DependencyReferenceProvenance| {
            *current = (*current).min(provenance);
        }).or_insert(provenance);
    }

    bray_base::sorted_unique_shared_slice(strongest.into_iter().map(|(relation, provenance)|
        relation.with_provenance(provenance)))
}

fn reference_origins<'a, Row, Subject, Projection>(
    rows: impl IntoIterator<Item = &'a Row>,
    contains: impl Fn(&Subject, &Subject) -> bool,
) -> Arc<[super::DependencyBorrowRelation<Subject, Projection>]>
where
    Row: CapturedStorageRow<Subject, Projection> + 'a,
    Subject: Clone + Ord + 'a,
    Projection: Clone + Ord + 'a,
{
    use std::collections::{BTreeMap, BTreeSet};

    let rows = rows.into_iter().collect::<Vec<_>>();
    let mut destinations = BTreeMap::<_, (usize, BTreeSet<_>)>::new();

    for row in &rows {
        let mut established = BTreeSet::new();

        for reference in row.references() {
            let group = destinations.entry(reference.exit()).or_default();

            group.0 += usize::from(established.insert(reference.exit()));
            group.1.insert(reference.clone());
        }
    }

    bray_base::sorted_unique_shared_slice(destinations.into_iter().flat_map(
        |(destination, (established, mut relations))| {
            if established != rows.len()
                && rows.iter().any(|row| {
                    !row.references().iter().any(|relation| relation.exit() == destination)
                        && !row.inactive_payloads().iter().any(|payload| contains(payload, destination))
                })
            {
                // An unobserved slot in a feasible exit is unknown, even when other exits have bounds.
                relations.clear();
            } else if relations.iter().any(|relation| !relation.is_no_payload()) {
                relations.retain(|relation| !relation.is_no_payload());
            }

            let mut joined = BTreeMap::new();

            for relation in relations {
                let provenance = relation.provenance();
                let key = relation.with_provenance(super::DependencyReferenceProvenance::Alternatives);

                joined.entry(key).and_modify(|previous: &mut super::DependencyReferenceProvenance| {
                    *previous = (*previous).max(provenance);
                }).or_insert(provenance);
            }

            let exact = joined.len() == 1;

            joined.into_iter().map(move |(relation, provenance)| {
                relation.with_provenance(provenance)
                    .with_exactness(exact && provenance == super::DependencyReferenceProvenance::Exact)
            })
        },
    ))
}

fn stored_dependencies<'a, Row, Subject, Projection>(
    rows: impl IntoIterator<Item = &'a Row>,
    contains: impl Fn(&Subject, &Subject) -> bool,
) -> Arc<[super::DependencyStoredValue<Subject>]>
where
    Row: CapturedStorageRow<Subject, Projection> + 'a,
    Subject: Clone + Ord + 'a,
    Projection: 'a,
{
    let mut rows = rows.into_iter();

    let Some(first) = rows.next() else { return Arc::new([]); };

    let Some(second) = rows.next() else { return first.stored_values().into(); };

    let rows = [first, second].into_iter().chain(rows).collect::<Vec<_>>();

    let merged = super::DependencyStoredValue::normalize(
        rows.iter().flat_map(|row| row.stored_values().iter().cloned()),
    );

    merged.iter().cloned().map(|value| {
        let destination = value.destination();

        if rows.iter().all(|row| {
            row.stored_values().binary_search_by(|incoming| incoming.destination().cmp(destination)).is_ok()
                || destination.dereferences() == 0
                    && row.references().iter().any(|reference| {
                        reference.exit() == destination.subject() && reference.is_no_payload()
                    })
                || row.inactive_payloads().iter().any(|payload| contains(payload, destination.subject()))
        }) {
            value
        } else {
            value.incomplete()
        }
    }).collect()
}

fn common_bindings<'a, Subject: Clone + Ord + 'a, Value: Clone + Eq + 'a>(
    mut rows: impl Iterator<Item = &'a [(Subject, Value)]> + Clone,
    unknown: &Value,
) -> Arc<[(Subject, Value)]> {
    let Some(first) = rows.next() else { return Arc::new([]); };

    first.iter().filter_map(|(destination, value)| {
        let mut matches = true;

        for row in rows.clone() {
            let index = row.binary_search_by(|(other, _)| other.cmp(destination)).ok()?;

            matches &= value == &row[index].1;
        }

        Some((destination.clone(), if matches { value.clone() } else { unknown.clone() }))
    }).collect()
}

/// Intersects the definite cell transfers supplied by every feasible outcome.
pub fn common_cell_changes<'a, Effect: Clone + Ord + 'a>(
    mut rows: impl Iterator<Item = Option<&'a Arc<[Effect]>>> + Clone,
) -> Option<Arc<[Effect]>> {
    let first = rows.next()??;

    if rows.clone().any(|row| row.is_none()) { return None; }

    if rows.clone().all(|row| row.is_some_and(|row| row == first)) {
        return Some(Arc::clone(first));
    }

    let common = first.iter().filter(|effect| rows.clone().all(|row| {
        row.is_some_and(|row| row.binary_search(effect).is_ok())
    })).cloned().collect::<Vec<_>>();

    Some(common.into())
}

#[cfg(test)]
mod tests {
    use super::{CapturedFailureOutcome as FailureOutcome, CapturedValueOutcome as Outcome, CapturedValueTemplate as Value};
    type Row = Outcome<u8, u8, (), (), u8>;
    type Failure = FailureOutcome<u8, (), (), (), u8, ()>;

    fn failure(
        references: impl IntoIterator<Item = super::super::DependencyBorrowRelation<u8, ()>>,
        stored_values: impl IntoIterator<Item = super::super::DependencyStoredValue<u8>>,
    ) -> Failure {
        Failure {
            kind: None,
            guard: None,
            effects: None,
            cell_changes: None,
            writes: std::sync::Arc::new([]),
            references: references.into_iter().collect::<Vec<_>>().into(),
            stored_values: stored_values.into_iter().collect::<Vec<_>>().into(),
            report: None,
        }
    }

    #[test]
    fn inactive_payloads_and_unknown_observations_have_distinct_joins() {
        use super::super::{DependencyBorrowRelation, DependencyReferenceKind, DependencyStoredValue, DependencyStoredValuePath};

        let input = DependencyStoredValuePath::new(1, 0);
        let destination = DependencyStoredValuePath::new(2, 0);

        let origin = DependencyBorrowRelation::new(Some(1), 2, DependencyReferenceKind::Address);
        let dependencies = DependencyStoredValue::new(destination, [input], true);

        let active = Row::new(None, []).with_storage([origin.clone()], [dependencies.clone()]);

        let inactive = Row::new(None, []).with_storage(
            [DependencyBorrowRelation::new_no_payload(2, DependencyReferenceKind::Address)],
            [],
        );

        let contains = |parent: &u8, child: &u8| parent == child || *parent == 3 && *child == 2;

        for inactive in [inactive, Row::new(None, []).with_inactive_payloads([3])] {
            let feasible = [active.clone(), inactive];

            assert_eq!(Row::reference_origins(&feasible, contains).as_ref(), std::slice::from_ref(&origin));
            assert_eq!(Row::stored_dependencies(&feasible, contains).as_ref(), std::slice::from_ref(&dependencies));
        }

        let feasible = [active, Row::new(None, [])];

        assert!(Row::reference_origins(&feasible, PartialEq::eq).is_empty());
        assert_eq!(Row::stored_dependencies(&feasible, PartialEq::eq).as_ref(), [dependencies.incomplete()]);
    }

    #[test]
    fn exact_reference_dominates_its_own_bound_but_not_another_exit() {
        use super::super::{DependencyBorrowRelation, DependencyReferenceKind, DependencyReferenceProvenance};

        let exact = DependencyBorrowRelation::new(Some(1), 2, DependencyReferenceKind::Borrow);
        let bound = exact.clone().with_provenance(DependencyReferenceProvenance::InputRegions);
        let one_exit = Row::new(None, []).with_storage([bound.clone(), exact.clone()], []);

        assert_eq!(one_exit.references.as_ref(), std::slice::from_ref(&exact));
        assert_eq!(Row::reference_origins([&one_exit], PartialEq::eq).as_ref(), std::slice::from_ref(&exact));

        let other_exit = Row::new(None, []).with_storage([bound.clone()], []);

        assert_eq!(Row::reference_origins([&one_exit, &other_exit], PartialEq::eq).as_ref(), std::slice::from_ref(&bound));
    }

    #[test]
    fn abnormal_rows_join_logical_and_physical_writes_at_the_same_selection() {
        use super::super::{
            DependencyBorrowRelation, DependencyReferenceKind, DependencyStoredValue,
            DependencyStoredValuePath,
        };

        let input = DependencyStoredValuePath::new(1, 0);
        let destination = DependencyStoredValuePath::new(2, 0);

        let origin = DependencyBorrowRelation::new(
            Some(1),
            2,
            DependencyReferenceKind::Address,
        );

        let dependency = DependencyStoredValue::new(destination, [input], true);
        let active = failure([origin.clone()], [dependency.clone()]);

        assert_eq!(
            Failure::reference_origins([&active], PartialEq::eq).as_ref(),
            std::slice::from_ref(&origin),
        );

        assert_eq!(
            Failure::stored_dependencies([&active], PartialEq::eq).as_ref(),
            std::slice::from_ref(&dependency),
        );

        let rows = [active, failure([], [])];

        assert!(Failure::reference_origins(&rows, PartialEq::eq).is_empty());

        assert_eq!(
            Failure::stored_dependencies(&rows, PartialEq::eq).as_ref(),
            &[dependency.incomplete()],
        );
    }

    #[test]
    fn recursive_exits_widen_only_writes_established_by_every_alternative() {
        let first = Row::new(None, [(0, Value::Input(1)), (1, Value::Input(2))]);
        let second = Row::new(None, [(0, Value::Input(2)), (2, Value::Input(1))]);
        let other = Row::new(Some(1), [(1, Value::Input(3))]);
        let rows = [first.clone(), second.clone()];

        assert_eq!(&*Row::join_reachable(&rows, &rows, PartialEq::eq), &rows);

        let widened = Row::join_reachable(&[first.clone(), other.clone()], &[second.clone()], PartialEq::eq);

        assert_eq!(&*widened, &[Row::new(None, [(0, Value::Unknown)]), other]);
        assert_eq!(Row::join_reachable(&widened, &[second], PartialEq::eq), widened);
        assert_eq!(&*Row::join_reachable(&[], &[first], PartialEq::eq), &[Row::new(None, [])]);
    }
}
