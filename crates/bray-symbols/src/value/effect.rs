use std::sync::Arc;

use super::{DependencyFailureKind, DependencyProjection, DependencySubject};

/// Possible transitions and the prefixes that can reach each completion channel.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DependencyEffectOutcomes<Effect> {
    possible: Arc<[Effect]>,
    normal: Arc<[Effect]>,
    abnormal: Option<Arc<[Effect]>>,
    failures: [Option<Arc<[Effect]>>; 2],
}

impl<Effect: Ord + Clone> DependencyEffectOutcomes<Effect> {
    /// Creates a conservative contract in which every transition can occur on either outcome.
    pub fn new(effects: impl IntoIterator<Item = Effect>) -> Self {
        Self::from_envelope(bray_base::sorted_unique_shared_slice(effects))
    }

    fn from_envelope(possible: Arc<[Effect]>) -> Self {
        Self { normal: Arc::clone(&possible), possible, abnormal: None, failures: [None, None] }
    }

    pub(super) fn with_unselected_outcomes(&self) -> Self {
        Self::from_envelope(Arc::clone(&self.possible))
    }

    /// Records normal transitions, retaining them among all possible transitions.
    pub fn with_normal(mut self, effects: impl IntoIterator<Item = Effect>) -> Self {
        self.normal = bray_base::sorted_unique_shared_slice(effects);

        if self.normal == self.possible {
            self.normal = Arc::clone(&self.possible);
        } else if self.normal.iter().any(|effect| self.possible.binary_search(effect).is_err()) {
            // The outcomes own descriptors and share their immutable identities and paths.
            self.possible = bray_base::sorted_unique_shared_slice(
                self.possible.iter().chain(self.normal.iter()).cloned(),
            );
        }

        self
    }

    /// Records a bound for the prefix that can reach abnormal completion.
    pub fn with_abnormal(mut self, effects: impl IntoIterator<Item = Effect>) -> Self {
        let effects = bray_base::sorted_unique_shared_slice(effects);

        if effects.iter().any(|effect| self.possible.binary_search(effect).is_err()) {
            self.possible = bray_base::sorted_unique_shared_slice(self.possible.iter().chain(&*effects).cloned());
        }

        self.abnormal = Some(effects);
        self.failures = [None, None];

        self
    }

    /// Installs distinct resolved prefixes and derives their common envelope.
    pub fn with_failure_prefixes(self, failures: [Arc<[Effect]>; 2]) -> Self {
        let combined = failures.iter().flat_map(|effects| effects.iter().cloned());
        let mut outcomes = self.with_abnormal(combined);

        outcomes.failures = failures.map(Some);

        outcomes
    }

    /// Joins possible execution paths without changing their outcome guards.
    pub fn merge(&mut self, incoming: &Self) -> bool {
        if self == incoming { return false; }

        let mut merged = Self::new(self.possible.iter().chain(incoming.possible.iter()).cloned())
            .with_normal(self.normal.iter().chain(incoming.normal.iter()).cloned());

        if let (Some(left), Some(right)) = (&self.abnormal, &incoming.abnormal) {
            merged = merged.with_abnormal(left.iter().chain(&**right).cloned());
        }

        if self.failures.iter().chain(&incoming.failures).any(Option::is_some) {
            merged = merged.with_failure_prefixes(DependencyFailureKind::ALL.map(|kind| {
                bray_base::sorted_unique_shared_slice(self.failure(kind).iter().chain(incoming.failure(kind)).cloned())
            }));
        }

        let changed = *self != merged;

        *self = merged;

        changed
    }

    /// Translates identities while preserving the outcome of every transition.
    pub fn try_map<Mapped: Ord + Clone, Error>(
        &self,
        mut map: impl FnMut(&Effect) -> Result<Mapped, Error>,
    ) -> Result<DependencyEffectOutcomes<Mapped>, Error> {
        self.try_flat_map(|effect| map(effect).map(std::iter::once))
    }

    /// Binds each transition once, retaining its outcome guards on every resolved alternative.
    pub fn try_flat_map<Mapped: Ord + Clone, Produced: IntoIterator<Item = Mapped>, Error>(
        &self,
        mut map: impl FnMut(&Effect) -> Result<Produced, Error>,
    ) -> Result<DependencyEffectOutcomes<Mapped>, Error> {
        self.try_flat_map_selected(false, std::iter::empty(), |effect, _| Ok((true, [false; 2], map(effect)?)))
    }

    /// Binds the possible envelope once while selecting each transition's reached channels.
    ///
    /// When the caller has a complete failure table, its two selections replace the
    /// template's unfiltered failure channels. An opaque body uses those channels directly.
    pub fn try_flat_map_selected<'effect, Mapped: Ord + Clone, Produced: IntoIterator<Item = Mapped>, Error>(
        &'effect self,
        select_failures: bool,
        additional_failure_effects: impl IntoIterator<Item = &'effect Effect>,
        mut bind: impl FnMut(&Effect, bool) -> Result<(bool, [bool; 2], Produced), Error>,
    ) -> Result<DependencyEffectOutcomes<Mapped>, Error> {
        let mut possible = Vec::with_capacity(self.possible.len());
        let mut normal = Vec::with_capacity(self.normal.len());
        let mut abnormal = (select_failures || self.abnormal.is_some()).then(Vec::new);

        let mut failures = self.failures.each_ref().map(|effects| {
            (select_failures || effects.is_some()).then(Vec::new)
        });

        let selected_effects = self.possible.iter().map(|effect| (effect, true)).chain(
            additional_failure_effects.into_iter().filter(|effect| self.possible.binary_search(effect).is_err())
                .map(|effect| (effect, false)),
        );

        for (effect, in_envelope) in selected_effects {
            let (admitted_normal, admitted_failures, resolved) = bind(effect, in_envelope)?;

            let on_normal_path = admitted_normal && in_envelope
                && (Arc::ptr_eq(&self.normal, &self.possible) || self.normal.binary_search(effect).is_ok());

            let on_failure_paths = if select_failures {
                admitted_failures
            } else {
                self.failures.each_ref().map(|effects| effects.as_ref().is_some_and(|effects| effects.binary_search(effect).is_ok()))
            };

            let on_abnormal_path = if select_failures {
                on_failure_paths.into_iter().any(|selected| selected)
            } else {
                self.abnormal.as_ref().is_some_and(|effects| effects.binary_search(effect).is_ok())
            };

            for mapped in resolved {
                if on_normal_path {
                    normal.push(mapped.clone());
                }

                if on_abnormal_path {
                    abnormal.as_mut().expect("a selected failure has an abnormal envelope").push(mapped.clone());
                }

                for (selected, output) in on_failure_paths.into_iter().zip(&mut failures) {
                    if selected {
                        output.as_mut().expect("a selected failure has a channel").push(mapped.clone());
                    }
                }

                possible.push(mapped);
            }
        }

        let possible = bray_base::sorted_unique_shared_slice(possible);
        let normal = bray_base::sorted_unique_shared_slice(normal);
        let normal = if normal == possible { Arc::clone(&possible) } else { normal };

        Ok(DependencyEffectOutcomes {
            possible,
            normal,
            abnormal: abnormal.map(bray_base::sorted_unique_shared_slice),
            failures: failures.map(|effects| effects.map(bray_base::sorted_unique_shared_slice)),
        })
    }
}

impl<Effect> DependencyEffectOutcomes<Effect> {
    /// Transitions that may occur before any completion, including a failing prefix.
    pub fn possible(&self) -> &[Effect] { &self.possible }

    /// Transitions that may occur on a path reaching normal completion.
    pub fn normal(&self) -> &[Effect] { &self.normal }

    /// Transitions that may have executed before failure, using the envelope for an opaque body.
    pub fn abnormal(&self) -> &[Effect] { self.abnormal.as_deref().unwrap_or(&self.possible) }

    /// Transitions that can reach the selected abnormal exit.
    pub fn failure(&self, kind: DependencyFailureKind) -> &[Effect] {
        self.failure_transitions(kind).as_ref()
    }

    /// Shares the resolved prefix with a consumer that keeps it through the failure edge.
    pub fn shared_failure(&self, kind: DependencyFailureKind) -> Arc<[Effect]> {
        Arc::clone(self.failure_transitions(kind))
    }

    fn failure_transitions(&self, kind: DependencyFailureKind) -> &Arc<[Effect]> {
        self.failures[kind.index()].as_ref().or(self.abnormal.as_ref()).unwrap_or(&self.possible)
    }
}

impl<Subject, Projection, Type> DependencyEffectOutcomes<DependencyStorageEffect<Subject, Projection, Type>> {
    /// Whether normal execution changes no storage beyond admitted input ownership.
    pub fn preserves_storage_on_normal_return(&self) -> bool {
        self.normal.iter().all(DependencyStorageEffect::preserves_storage)
    }
}

/// The storage transition a callable may perform at an invocation or deferred phase.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyMutationKind<Type = super::TypeId> {
    /// Mutate protected atomic payload without replacing any containing value.
    Atomic,
    /// Publish an address or references reachable through the named storage.
    Escape,
    /// Replace contents while retaining the storage's initialized lifetime.
    Write,
    /// Install a value of the stated type, including its owned graceful-cleanup obligations.
    Replace(Type),
    /// Install an absent payload while retaining initialized containing storage.
    Clear,
    /// Dispose a value at this location, whose layout is the stated type.
    Dispose(Type),
    /// Conditional effects of disposing this type on a region with no corresponding layout claim.
    Cleanup(Type),
    /// Cleanup at a typed captured route with no definite mutation operand.
    DisposeCaptured {
        /// Type of the selected cleanup value.
        ty: Type,
        /// An in-place child reached through the capture can change its own inline storage.
        include_inline: bool,
    },
    /// Remove initialized contents without retiring the storage.
    Consume,
    /// A selected cell update has transferred the complete stored value of this type.
    Discharge(Type),
    /// End the storage lifetime and revoke dependencies on that lifetime.
    Retire,
}

impl<Type> DependencyMutationKind<Type> {
    /// The type whose lifecycle determines the effects of this operation.
    pub const fn cleanup_type(&self) -> Option<&Type> {
        match self {
            Self::Dispose(ty) | Self::Cleanup(ty) | Self::DisposeCaptured { ty, .. } => Some(ty),
            _ => None,
        }
    }

    /// These transitions cannot install a new initialized value in the cell.
    pub fn preserves_discharge(&self) -> bool {
        matches!(self, Self::Atomic | Self::Escape | Self::Consume | Self::Discharge(_) | Self::Retire)
    }

    /// Removes the layout claim of a mutation known only through its footprint.
    pub fn region_kind(self) -> Self {
        match self {
            Self::Dispose(ty) => Self::Cleanup(ty),
            Self::Discharge(_) => Self::Consume,
            other => other,
        }
    }

    /// Removes transitions that require a typed route when no route is available.
    pub fn unlocated_kind(self) -> Self {
        match self {
            Self::DisposeCaptured { ty, .. } => Self::Cleanup(ty),
            Self::Discharge(_) => Self::Consume,
            other => other,
        }
    }
}

/// A possible mutation expressed over entry values, separately from their reference targets.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyStorageEffect<
    Subject = DependencySubject,
    Projection = DependencyProjection,
    Type = super::TypeId,
> {
    /// A retirement and the typed discharges that precede it.
    OrderedRetirement {
        /// Actual captured operand, using the same route as other effects.
        retirement: Box<Self>,
        /// Exact typed cell discharges performed before this retirement.
        discharged: Arc<[Self]>,
    },
    /// Protected atomic payloads may change through unresolved aliases.
    Atomic,
    /// Mutate externally reachable storage without exposing invocation-local inputs.
    Ambient,
    /// No finite target set has been established.
    Unknown,
    /// Addresses reachable through any invocation input may be retained externally.
    Escape,
    /// A known transition whose affected region has no finite entry-relative description.
    UnboundedMutation {
        /// Possible transition.
        kind: DependencyMutationKind<Type>,
        /// Original checked access type, independent of the unknown region.
        access_type: Option<Type>,
        /// Only externally reachable storage can be affected.
        ambient: bool,
    },
    /// Mutate the value slot named by the formal subject.
    Direct {
        /// Whether the complete route supplies the evaluated operand as well as its footprint.
        addressed: bool,
        /// Entry value slot.
        subject: Subject,
        /// Original checked access type, independent of the region that bounds aliases.
        /// Forwarding a raw access through a borrowed owner preserves this type.
        access_type: Option<Type>,
        /// Possible transition.
        kind: DependencyMutationKind<Type>,
    },
    /// Mutate storage reached by the reference value held in an entry slot.
    Indirect {
        /// Whether the complete route supplies the evaluated operand as well as its footprint.
        addressed: bool,
        /// Slot containing the entry reference. Reassigning this slot does not change that identity.
        reference: Subject,
        /// Projection positions at which execution reads another stored descriptor.
        /// Equal positions denote successive pointers without an intervening component.
        reads: Arc<[u32]>,
        /// Checked components selected between reads and after the final read.
        projections: Arc<[Projection]>,
        /// Original checked access type, independent of the region that bounds aliases.
        /// Forwarding a raw access through a borrowed owner preserves this type.
        access_type: Option<Type>,
        /// Possible transition.
        kind: DependencyMutationKind<Type>,
    },
}

impl<Subject, Projection, Type> DependencyStorageEffect<Subject, Projection, Type> {
    /// Returns the actual mutation and any discharges established before it.
    pub fn retirement_parts(&self) -> (&Self, &[Self]) {
        match self {
            Self::OrderedRetirement { retirement, discharged } => (retirement, discharged),
            _ => (self, &[]),
        }
    }

    /// Whether the effect can carry ordered discharge facts.
    pub fn is_addressed_retirement(&self) -> bool {
        matches!(self,
            Self::Direct { addressed: true, kind: DependencyMutationKind::Retire, .. }
            | Self::Indirect { addressed: true, kind: DependencyMutationKind::Retire, .. }
        )
    }

    /// Attaches definite discharges to the exact retirement operand.
    pub fn with_retirement_discharges(self, discharged: Arc<[Self]>) -> Self {
        if discharged.is_empty() {
            return self;
        }

        assert!(self.is_addressed_retirement(), "a discharge prefix requires an addressed retirement");
        assert!(discharged.iter().all(Self::is_addressed_discharge), "a discharge prefix requires exact typed discharges");

        Self::OrderedRetirement { retirement: Box::new(self), discharged }
    }

    /// Translates an effect without changing its captured reads, precision or transition kind.
    pub fn try_map<C, S, P, T, E>(
        &self,
        context: &mut C,
        mut subject: impl FnMut(&mut C, &Subject) -> Result<S, E>,
        mut projections: impl FnMut(&mut C, &[Projection]) -> Result<Arc<[P]>, E>,
        mut ty: impl FnMut(&mut C, Type) -> Result<T, E>,
    ) -> Result<DependencyStorageEffect<S, P, T>, E>
    where Type: Copy {
        let (effect, discharged) = self.retirement_parts();

        let mut map = |effect: &Self| effect.try_map_route(context, &mut subject, &mut projections, &mut ty);
        let mapped = map(effect)?;
        let discharged = discharged.iter().map(map).collect::<Result<Vec<_>, E>>()?;

        Ok(mapped.with_retirement_discharges(discharged.into()))
    }

    fn try_map_route<C, S, P, T, E>(
        &self,
        context: &mut C,
        subject: &mut impl FnMut(&mut C, &Subject) -> Result<S, E>,
        projections: &mut impl FnMut(&mut C, &[Projection]) -> Result<Arc<[P]>, E>,
        ty: &mut impl FnMut(&mut C, Type) -> Result<T, E>,
    ) -> Result<DependencyStorageEffect<S, P, T>, E>
    where Type: Copy {
        use DependencyStorageEffect as Effect;

        Ok(match self {
            Self::OrderedRetirement { .. } => unreachable!("retirement has only one discharge prefix"),
            Self::Atomic => Effect::Atomic,
            Self::Ambient => Effect::Ambient,
            Self::Unknown => Effect::Unknown,
            Self::Escape => Effect::Escape,
            Self::UnboundedMutation { kind, access_type, ambient } => Effect::UnboundedMutation {
                kind: kind.try_map_type(|value| ty(context, value))?,
                access_type: access_type.map(|value| ty(context, value)).transpose()?,
                ambient: *ambient,
            },
            Self::Direct { subject: root, addressed, kind, access_type } => Effect::Direct {
                subject: subject(context, root)?,
                addressed: *addressed,
                kind: kind.try_map_type(|value| ty(context, value))?,
                access_type: access_type.map(|value| ty(context, value)).transpose()?,
            },
            Self::Indirect { reference, addressed, reads, projections: path, kind, access_type } => Effect::Indirect {
                reference: subject(context, reference)?,
                addressed: *addressed,
                reads: Arc::clone(reads),
                projections: projections(context, path)?,
                kind: kind.try_map_type(|value| ty(context, value))?,
                access_type: access_type.map(|value| ty(context, value)).transpose()?,
            },
        })
    }

    /// Keeps this route as a footprint after losing the actual operand.
    pub fn into_region(self) -> Self {
        match self {
            Self::OrderedRetirement { retirement, .. } => (*retirement).into_region(),
            Self::Direct { subject, kind, access_type, .. } => Self::Direct {
                subject, addressed: false, kind: kind.region_kind(), access_type,
            },
            Self::Indirect { reference, reads, projections, kind, access_type, .. } => Self::Indirect {
                reference, reads, projections, addressed: false, kind: kind.region_kind(), access_type,
            },
            Self::UnboundedMutation { kind, access_type, ambient } => Self::UnboundedMutation {
                kind: kind.region_kind().unlocated_kind(), access_type, ambient,
            },
            effect => effect,
        }
    }

    /// Whether the effect only transfers an admitted input value.
    pub fn preserves_storage(&self) -> bool {
        matches!(self, Self::Direct { kind: DependencyMutationKind::Consume, .. })
    }

    /// Whether this update records a definite typed discharge.
    pub fn is_discharge(&self) -> bool {
        matches!(self, Self::Direct { kind: DependencyMutationKind::Discharge(_), .. }
            | Self::Indirect { kind: DependencyMutationKind::Discharge(_), .. }
            | Self::UnboundedMutation { kind: DependencyMutationKind::Discharge(_), .. })
    }

    /// Whether a definite discharge has a route to the actual typed cell.
    pub fn is_addressed_discharge(&self) -> bool {
        self.is_cell_change() && self.is_discharge()
    }

    /// Whether this update names a complete typed cell.
    pub fn is_cell_change(&self) -> bool {
        matches!(self,
            Self::Direct { addressed: true, kind: DependencyMutationKind::Replace(_) | DependencyMutationKind::Discharge(_), .. }
            | Self::Indirect { addressed: true, kind: DependencyMutationKind::Replace(_) | DependencyMutationKind::Discharge(_), .. }
        )
    }

    /// Whether the route can be executed in order and its precision admits the requested layout.
    pub fn is_valid_route(&self) -> bool {
        match self {
            Self::OrderedRetirement { retirement, discharged } => !discharged.is_empty()
                && retirement.is_addressed_retirement()
                && retirement.is_valid_route()
                && discharged.iter().all(|effect| effect.is_addressed_discharge() && effect.is_valid_route()),
            Self::Direct { addressed: false, kind: DependencyMutationKind::Dispose(_), .. }
            | Self::Indirect { addressed: false, kind: DependencyMutationKind::Dispose(_) | DependencyMutationKind::Discharge(_), .. }
            | Self::Direct { addressed: false, kind: DependencyMutationKind::Discharge(_), .. }
            | Self::Direct { addressed: true, kind: DependencyMutationKind::DisposeCaptured { .. }, .. }
            | Self::Indirect { addressed: true, kind: DependencyMutationKind::DisposeCaptured { .. }, .. }
            | Self::UnboundedMutation { kind: DependencyMutationKind::DisposeCaptured { .. }, .. }
            | Self::UnboundedMutation { kind: DependencyMutationKind::Discharge(_), .. } => false,
            Self::Indirect { reads, projections, .. } => reads.is_sorted()
                && reads.last().is_none_or(|index| (*index as usize) <= projections.len()),
            _ => true,
        }
    }

    /// The effect's route and any exact typed discharges carried with it.
    pub fn routes(&self) -> impl Iterator<Item = &Self> {
        let (effect, discharged) = self.retirement_parts();

        std::iter::once(effect).chain(discharged)
    }

    /// Entry slots needed to bind the effect and its definite discharges.
    pub fn subjects(&self) -> impl Iterator<Item = &Subject> {
        self.routes().filter_map(|effect| match effect {
            Self::Direct { subject, .. } => Some(subject),
            Self::Indirect { reference, .. } => Some(reference),
            _ => None,
        })
    }

    /// Projection routes carried by the mutation and its ordered discharges.
    pub fn projection_paths(&self) -> impl Iterator<Item = &[Projection]> {
        self.routes().filter_map(|effect| match effect {
            Self::Indirect { projections, .. } => Some(projections.as_ref()),
            _ => None,
        })
    }

    /// Types required to retain and interpret this effect across semantic boundaries.
    pub fn referenced_types(&self) -> impl Iterator<Item = &Type> {
        self.routes().flat_map(|effect| match effect {
            Self::Direct { access_type, kind, .. }
            | Self::Indirect { access_type, kind, .. }
            | Self::UnboundedMutation { access_type, kind, .. } => [
                access_type.as_ref(),
                match kind {
                    DependencyMutationKind::Replace(ty) | DependencyMutationKind::Discharge(ty) => Some(ty),
                    other => other.cleanup_type(),
                },
            ],
            _ => [None, None],
        }.into_iter().flatten())
    }

    /// Conservative effects when the callable can have an unknown captured environment.
    pub const fn opaque_call() -> [Self; 2] {
        [Self::Unknown, Self::Escape]
    }

    /// Conservative effects when all inputs are explicit and no captured environment exists.
    pub const fn capture_free_call() -> [Self; 2] {
        [Self::Ambient, Self::Escape]
    }

    /// The broad fallback for a callable without checked normal effect rows.
    pub const fn is_opaque_call_fallback(&self) -> bool {
        matches!(self, Self::Unknown | Self::Ambient | Self::Escape)
    }

    /// Describes a transition whose particular storage target is unresolved.
    /// Atomic updates and address escape retain their distinct meanings.
    pub fn unresolved(kind: DependencyMutationKind<Type>, access_type: Option<Type>) -> Self {
        match kind {
            DependencyMutationKind::Atomic => Self::Atomic,
            DependencyMutationKind::Escape => Self::Escape,
            kind => Self::UnboundedMutation { kind: kind.unlocated_kind(), access_type, ambient: false },
        }
    }
}

impl<Type> DependencyMutationKind<Type> {
    /// Whether contents change without ending the containing storage lifetime.
    pub const fn is_write(&self) -> bool {
        matches!(self, Self::Write | Self::Replace(_) | Self::Clear)
    }

    /// Translates the payload type while preserving the storage transition.
    pub fn try_map_type<Other, Error>(
        self,
        map: impl FnOnce(Type) -> Result<Other, Error>,
    ) -> Result<DependencyMutationKind<Other>, Error> {
        Ok(match self {
            Self::Atomic => DependencyMutationKind::Atomic,
            Self::Escape => DependencyMutationKind::Escape,
            Self::Write => DependencyMutationKind::Write,
            Self::Clear => DependencyMutationKind::Clear,
            Self::Replace(ty) => DependencyMutationKind::Replace(map(ty)?),
            Self::Dispose(ty) => DependencyMutationKind::Dispose(map(ty)?),
            Self::Cleanup(ty) => DependencyMutationKind::Cleanup(map(ty)?),
            Self::DisposeCaptured { ty, include_inline } => DependencyMutationKind::DisposeCaptured { ty: map(ty)?, include_inline },
            Self::Consume => DependencyMutationKind::Consume,
            Self::Discharge(ty) => DependencyMutationKind::Discharge(map(ty)?),
            Self::Retire => DependencyMutationKind::Retire,
        })
    }
}

impl<Subject: Ord, Projection: Ord, Type: Ord> DependencyStorageEffect<Subject, Projection, Type> {
    /// Normalizes possible effects while retaining known transitions beside opaque effects.
    pub fn normalize(effects: impl IntoIterator<Item = Self>) -> Arc<[Self]> {
        let mut effects = effects.into_iter().collect::<Vec<_>>();

        if effects.iter().any(|effect| matches!(effect, Self::Atomic)) {
            effects.retain(|effect| {
                !matches!(
                    effect,
                    Self::Direct {
                        kind: DependencyMutationKind::Atomic,
                        ..
                    } | Self::Indirect {
                        kind: DependencyMutationKind::Atomic,
                        ..
                    }
                )
            });
        }

        bray_base::sorted_unique_shared_slice(effects)
    }
}

#[cfg(test)]
mod tests {
    use super::{DependencyMutationKind, DependencyStorageEffect, DependencySubject};

    #[test]
    fn mapping_and_joining_transitions_preserves_normal_and_abnormal_outcomes() {
        let left = super::DependencyEffectOutcomes::new([1, 2]).with_normal([1]);
        let right = super::DependencyEffectOutcomes::new([2, 3]).with_normal([3]);

        let mut next = 10;

        let mut joined = left.try_map(|_| {
            next += 1;

            Ok::<_, std::convert::Infallible>(next)
        })
            .expect("fixture translation is total");

        let right = right.try_map(|effect| Ok::<_, std::convert::Infallible>(effect + 10))
            .expect("fixture translation is total");

        assert!(joined.merge(&right));
        assert_eq!(joined, super::DependencyEffectOutcomes::new([11, 12, 13]).with_normal([11, 13]));
        assert!(!joined.merge(&right), "revisiting the same program point must stabilize");
    }
    #[test]
    fn unresolved_atomic_effect_absorbs_only_narrower_atomic_effects() {
        let subject = DependencySubject::root(super::super::DependencySubjectRoot::Receiver);

        let write: DependencyStorageEffect = DependencyStorageEffect::Direct { addressed: true,
            subject: subject.clone(),
            kind: DependencyMutationKind::Write,
            access_type: None,
        };

        let atomic = DependencyStorageEffect::Direct { addressed: true,
            subject,
            kind: DependencyMutationKind::Atomic,
            access_type: None,
        };

        assert_eq!(
            DependencyStorageEffect::normalize([
                atomic,
                write.clone(),
                DependencyStorageEffect::Atomic,
            ])
            .as_ref(),
            &[DependencyStorageEffect::Atomic, write]
        );
    }

    #[test]
    fn absent_summary_is_not_a_proof_of_no_mutation() {
        let missing = super::super::DependencyContractTemplateData::new([]);
        let unchanged = missing.clone().with_possible_effects([]);

        assert_eq!(
            missing.effects(),
            &super::DependencyStorageEffect::opaque_call()
        );

        assert!(unchanged.effects().is_empty());

        let write: DependencyStorageEffect = DependencyStorageEffect::Direct { addressed: true,
            subject: DependencySubject::root(super::super::DependencySubjectRoot::Receiver),
            kind: DependencyMutationKind::Write,
            access_type: None,
        };

        assert_eq!(
            DependencyStorageEffect::normalize([write.clone(), write.clone()]).as_ref(),
            std::slice::from_ref(&write)
        );

        assert_eq!(
            DependencyStorageEffect::normalize([write.clone(), DependencyStorageEffect::Unknown])
                .as_ref(),
            &[DependencyStorageEffect::Unknown, write]
        );
    }
    #[test]
    fn unresolved_targets_preserve_known_transitions() {
        type Effect = DependencyStorageEffect<(), (), ()>;

        for kind in [
            DependencyMutationKind::Atomic,
            DependencyMutationKind::Escape,
            DependencyMutationKind::Write,
            DependencyMutationKind::Replace(()),
            DependencyMutationKind::Dispose(()),
            DependencyMutationKind::Cleanup(()),
            DependencyMutationKind::Clear,
            DependencyMutationKind::Consume,
            DependencyMutationKind::Retire,
        ] {
            let expected = match kind {
                DependencyMutationKind::Atomic => Effect::Atomic,
                DependencyMutationKind::Escape => Effect::Escape,
                _ => Effect::UnboundedMutation { kind, access_type: Some(()), ambient: false },
            };

            assert_eq!(Effect::unresolved(kind, Some(())), expected);
        }
    }

    #[test]
    fn an_unbounded_cleanup_region_cannot_publish_a_typed_discharge() {
        type Effect = DependencyStorageEffect<(), (), ()>;

        let scope = Effect::UnboundedMutation {
            kind: DependencyMutationKind::Discharge(()), access_type: Some(()), ambient: true,
        }.into_region();

        assert_eq!(scope, Effect::UnboundedMutation {
            kind: DependencyMutationKind::Consume, access_type: Some(()), ambient: true,
        });

        assert!(scope.is_valid_route());
    }
}
