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

/// Values produced together on one feasible normal exit.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CapturedValueOutcome<Subject = super::DependencySubject, Input = ConstantTermId, Projection = DependencyProjection,
    Static = crate::StaticReferenceSelection, Guard = Input, OwnedProjection = super::ConstantProjectionKind, Callee = CapturedCallable, Variant = crate::UnionVariantSymbolId,
    Report = super::DependencyRequirement, Effect = super::DependencyStorageEffect, WitnessSubject = super::DependencySubject> {
    /// The report produced on this exit, including any storage dependencies it carries.
    pub report: Option<Arc<[Report]>>,
    /// Possible transitions on paths reaching this exit.
    pub effects: Arc<[Effect]>,
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

impl<Subject, Input, Projection, Static, Guard, OwnedProjection, Callee: Eq, Variant: Eq, Report, Effect, WitnessSubject> CapturedValueOutcome<Subject, Input, Projection, Static, Guard, OwnedProjection, Callee, Variant, Report, Effect, WitnessSubject>
where Subject: Ord, Input: Eq, Projection: Eq, Static: Eq {
    /// Retains the report accompanying this normal result.
    pub fn with_report(mut self, report: Option<Arc<[Report]>>) -> Self {
        self.report = report;

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
        self.references = bray_base::sorted_unique_shared_slice(references);
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
            effects: Arc::default(),
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
        use std::collections::{BTreeMap, BTreeSet};

        let rows = rows.into_iter().collect::<Vec<_>>();
        let mut destinations = BTreeMap::<_, (usize, BTreeSet<_>)>::new();

        for row in &rows {
            let mut established = BTreeSet::new();

            for reference in row.references.iter() {
                let group = destinations.entry(reference.exit()).or_default();

                group.0 += usize::from(established.insert(reference.exit()));
                group.1.insert(reference.clone());
            }
        }

        bray_base::sorted_unique_shared_slice(destinations.into_iter().flat_map(|(destination, (established, mut relations))| {
            if established != rows.len() && rows.iter().any(|row| {
                !row.references.iter().any(|relation| relation.exit() == destination)
                    && !row.inactive_payloads.iter().any(|payload| contains(payload, destination))
            }) {
                // An unobserved slot in a feasible exit is unknown, even when other exits have bounds.
                relations.clear();
            } else if relations.iter().any(|relation| !relation.is_no_payload()) {
                relations.retain(|relation| !relation.is_no_payload());
            }

            let exact = relations.len() == 1;

            relations.into_iter().map(move |relation| {
                let exact = exact && relation.is_exact();

                relation.with_exactness(exact)
            })
        }))
    }

    /// Joins known stored dependencies without treating a missing active slot as independent.
    pub fn stored_dependencies<'a>(rows: impl IntoIterator<Item = &'a Self>, contains: impl Fn(&Subject, &Subject) -> bool) -> Arc<[super::DependencyStoredValue<Subject>]>
    where Self: 'a {
        let rows = rows.into_iter().collect::<Vec<_>>();

        let merged = super::DependencyStoredValue::normalize(
            rows.iter().flat_map(|row| row.stored_values.iter().cloned()),
        );

        merged.iter().cloned().map(|value| {
            let destination = value.destination();

            if rows.iter().all(|row| row.stored_values.binary_search_by(|incoming|
                incoming.destination().cmp(destination)).is_ok()
                || destination.dereferences() == 0 && row.references.iter().any(|reference|
                    reference.exit() == destination.subject() && reference.is_no_payload())
                || row.inactive_payloads.iter().any(|payload| contains(payload, destination.subject())))
            {
                value
            } else {
                value.incomplete()
            }
        }).collect()
    }

    /// Retains inactive subtrees established by every feasible normal exit.
    pub fn common_inactive_payloads<'a>(rows: impl IntoIterator<Item = &'a Self>, contains: impl Fn(&Subject, &Subject) -> bool) -> Arc<[Subject]>
    where Self: 'a {
        let rows = rows.into_iter().collect::<Vec<_>>();

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

                let bindings = old[0].bindings.iter().filter_map(|(destination, value)| {
                    let mut common = value;

                    for row in &old[1..] {
                        let index = row.bindings.binary_search_by(|(other, _)| other.cmp(destination)).ok()?;

                        if &row.bindings[index].1 != value { common = &CapturedValueTemplate::Unknown; }
                    }

                    Some((destination.clone(), common.clone()))
                }).collect();

                let carriers = Self::common_carriers(&old);
                let references = Self::reference_origins(&old, &contains);
                let stored_values = Self::stored_dependencies(&old, &contains);

                let inactive_payloads = Self::common_inactive_payloads(&old, &contains);
                let report = Self::possible_report(&old);
                let effects = bray_base::sorted_unique_shared_slice(old.iter().flat_map(|row| row.effects.iter().cloned()));
                let ownership = super::DependencyOwnedResult::choice(old.into_iter().map(|row| row.ownership));

                result.push(Self { guard, report, effects, bindings, ownership, carriers, references, stored_values, inactive_payloads });
            }
        }

        result.into()
    }
}

#[cfg(test)]
mod tests {
    use super::{CapturedValueOutcome as Outcome, CapturedValueTemplate as Value};
    type Row = Outcome<u8, u8, (), (), u8>;

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
