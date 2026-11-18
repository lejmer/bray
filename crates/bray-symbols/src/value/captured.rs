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
    /// Whether capture zero holds the receiver.
    pub has_receiver: bool,
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
pub enum CapturedValueTemplate<Input = ConstantTermId, Projection = DependencyProjection, Static = crate::StaticReferenceSelection, Callee = CapturedCallable> {
    /// The actual value bound to a receiver-first formal term.
    Input(Input),
    /// Ordinary scalar data, with no ownership or cleanup duty from its inputs.
    Data(Input),
    /// The exact selected static instance, including its substitution, witnesses and target.
    Static(Static),
    /// Feasible lazy invocations, each retaining its own guard and actual environment.
    Deferred {
        /// Complete selected environments. Callees do not supply runtime allocation identity.
        alternatives: Arc<[CapturedDeferredInvocation<Input, Projection, Static, Callee>]>,
        /// An opaque producer is also feasible. This does not assert an empty capture set.
        unknown: bool,
    },
    /// An independently produced value with no asserted relation to an input.
    /// Unknown contents do not mean an empty initialized prefix.
    Unknown,
    /// Evaluated whole-value storage operation.
    Operation(CapturedValueOperation<Projection>, Arc<[Self]>),
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
pub struct CapturedDeferredInvocation<Input = ConstantTermId, Projection = DependencyProjection, Static = crate::StaticReferenceSelection, Callee = CapturedCallable> {
    /// Ordinary selection facts in the containing value's entry scope.
    pub guard: Option<Input>,
    /// Exact semantic callee and witnesses. Formal arguments index this environment.
    pub call: Option<Arc<Callee>>,
    /// Complete captured inputs in receiver-first order.
    pub captures: Arc<[CapturedValueTemplate<Input, Projection, Static, Callee>]>,
    /// Captures whose ownership and cleanup duties belong to this frame.
    pub owned_inputs: Arc<[crate::SymbolOrdinal]>,
}

impl<Input, Projection, Static, Callee> CapturedValueTemplate<Input, Projection, Static, Callee> {
    /// Visits shared expression structure in deterministic operand order.
    pub fn nodes(&self) -> impl Iterator<Item = &Self> {
        let mut pending = vec![self];

        std::iter::from_fn(move || {
            let node = pending.pop()?;

            match node {
                Self::Operation(_, values) => pending.extend(values.iter().rev()),
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

    /// Remaps portable identities while preserving the complete value expression.
    pub fn try_map<E, OtherInput, OtherProjection, OtherStatic, OtherCallee>(&self, input: &mut impl FnMut(&Input) -> Result<OtherInput, E>,
        projection: &mut impl FnMut(&Projection) -> Result<OtherProjection, E>,
        static_root: &mut impl FnMut(&Static) -> Result<OtherStatic, E>,
        callee: &mut impl FnMut(&Callee) -> Result<OtherCallee, E>,
    ) -> Result<CapturedValueTemplate<OtherInput, OtherProjection, OtherStatic, OtherCallee>, E> {
        use CapturedValueTemplate as Value;

        Ok(match self {
            Self::Input(value) => Value::Input(input(value)?),
            Self::Data(value) => Value::Data(input(value)?),
            Self::Static(value) => Value::Static(static_root(value)?),
            Self::Deferred { alternatives, unknown } => Value::Deferred {
                alternatives: alternatives.iter().map(|alternative| Ok(CapturedDeferredInvocation {
                    guard: alternative.guard.as_ref().map(&mut *input).transpose()?,
                    call: alternative.call.as_deref().map(&mut *callee).transpose()?.map(Arc::new),
                    captures: alternative.captures.iter().map(|value| value.try_map(input, projection, static_root, callee)).collect::<Result<_, E>>()?,
                    owned_inputs: Arc::clone(&alternative.owned_inputs),
                })).collect::<Result<_, E>>()?,
                unknown: *unknown,
            },
            Self::Unknown => Value::Unknown,
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
            }, values.iter().map(|value| value.try_map(input, projection, static_root, callee)).collect::<Result<_, _>>()?),
            Self::Aggregate { base, fields } => Value::Aggregate {
                base: Arc::new(base.try_map(input, projection, static_root, callee)?),
                fields: fields.iter().map(|(field, value)|
                    Ok((projection(field)?, value.try_map(input, projection, static_root, callee)?))).collect::<Result<_, E>>()?,
            },
        })
    }
}

/// Values produced together on one feasible normal exit.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CapturedValueOutcome<Subject = super::DependencySubject, Input = ConstantTermId, Projection = DependencyProjection,
    Static = crate::StaticReferenceSelection, Guard = Input, OwnedProjection = super::ConstantProjectionKind, Callee = CapturedCallable> {
    /// Checked guarantees and the returned owners supporting them on this exit.
    pub carriers: Arc<[super::DependencyWitnessCarrier<Projection, Guard>]>,
    /// Transfers of existing input ownership on this exit, sharing its guard and result.
    pub ownership: super::DependencyOwnedResult<OwnedProjection, Guard>,
    /// Ordinary entry or result-shape condition. An absent guard is feasible unconditionally.
    pub guard: Option<Guard>,
    /// Writes established on this exit. An unknown value still establishes a write.
    /// An omitted destination supplies no write guarantee.
    pub bindings: Arc<[(Subject, CapturedValueTemplate<Input, Projection, Static, Callee>)]>,
}

impl<Subject, Input, Projection, Static, Guard, OwnedProjection, Callee: Eq> CapturedValueOutcome<Subject, Input, Projection, Static, Guard, OwnedProjection, Callee>
where Subject: Ord, Input: Eq, Projection: Eq, Static: Eq {
    /// Attaches only guarantees established by this exit.
    pub fn with_carriers(mut self, carriers: impl IntoIterator<Item = super::DependencyWitnessCarrier<Projection, Guard>>) -> Self
    where Projection: Ord, Guard: Ord {
        self.carriers = bray_base::sorted_unique_shared_slice(carriers);

        self
    }

    /// Attaches the complete ownership layout established by this exit.
    pub fn with_ownership(mut self, ownership: super::DependencyOwnedResult<OwnedProjection, Guard>) -> Self {
        self.ownership = ownership;

        self
    }

    /// Binds all writes of one exit. Aliased destinations retain only a common value.
    pub fn new(guard: Option<Guard>, bindings: impl IntoIterator<Item = (Subject, CapturedValueTemplate<Input, Projection, Static, Callee>)>) -> Self {
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

        Self { guard, carriers: Arc::default(), bindings: writes.into_iter().collect(), ownership: Default::default() }
    }
}

impl<Subject: Ord + Clone, Input: Ord + Clone, Projection: Ord + Clone, Static: Ord + Clone, Guard: Ord + Clone, OwnedProjection: Ord + Clone, Callee: Ord + Clone>
    CapturedValueOutcome<Subject, Input, Projection, Static, Guard, OwnedProjection, Callee>
{
    /// Guarantees every listed exit establishes on the same owner.
    pub fn common_carriers(rows: &[Self]) -> Arc<[super::DependencyWitnessCarrier<Projection, Guard>]> {
        super::DependencyWitnessCarrier::common(rows.iter().map(|row| &row.carriers))
    }

    /// Returns ownership across all normal exits for consumers operating before exit selection.
    pub fn ownership_layout(rows: &[Self]) -> super::DependencyOwnedResult<OwnedProjection, Guard> {
        super::DependencyOwnedResult::choice(rows.iter().map(|row| match &row.guard {
            Some(guard) => super::DependencyOwnedResult::case(guard.clone(), row.ownership.clone()),
            None => row.ownership.clone(),
        }))
    }

    /// Widens successive approximations of reachable exits at a fixed point.
    /// An unspecified normal outcome supplies no guaranteed writes.
    pub fn join_reachable(left: &[Self], right: &[Self]) -> Arc<[Self]> {
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
                let ownership = super::DependencyOwnedResult::choice(old.into_iter().map(|row| row.ownership));

                result.push(Self { guard, bindings, ownership, carriers });
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
    fn recursive_exits_widen_only_writes_established_by_every_alternative() {
        let first = Row::new(None, [(0, Value::Input(1)), (1, Value::Input(2))]);
        let second = Row::new(None, [(0, Value::Input(2)), (2, Value::Input(1))]);
        let other = Row::new(Some(1), [(1, Value::Input(3))]);
        let rows = [first.clone(), second.clone()];

        assert_eq!(&*Row::join_reachable(&rows, &rows), &rows);

        let widened = Row::join_reachable(&[first.clone(), other.clone()], &[second.clone()]);

        assert_eq!(&*widened, &[Row::new(None, [(0, Value::Unknown)]), other]);
        assert_eq!(Row::join_reachable(&widened, &[second]), widened);
        assert_eq!(&*Row::join_reachable(&[], &[first]), &[Row::new(None, [])]);
    }
}
