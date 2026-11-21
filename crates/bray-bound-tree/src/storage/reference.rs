use std::sync::Arc;

use bray_symbols::{BorrowKind, DependencyReferenceKind, StructFieldSymbolId, TypeId};

use crate::StorageProjection;

/// A typed operation between ordinary component selections. These steps describe
/// reads, never ownership, initialization or allocation authority.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageReferenceStep {
    /// Follow a language borrow with its checked access mode.
    ReadBorrow { index: usize, target: TypeId, mode: BorrowKind },
    /// Follow an evaluated raw address.
    ReadAddress { index: usize, target: TypeId },
    /// Select an owning allocation's backing. A represented buffer reads its
    /// prepared pointer field. Other owning indirections have structural backing.
    Backing { index: usize, target: TypeId, pointer: Option<StructFieldSymbolId> },
}

impl StorageReferenceStep {
    /// The structural projection preceding this operation.
    pub fn index(self) -> usize {
        match self {
            Self::ReadBorrow { index, .. } | Self::ReadAddress { index, .. } | Self::Backing { index, .. } => index,
        }
    }

    fn shifted(self, prefix: usize) -> Option<Self> {
        let index = self.index().checked_sub(prefix)?;

        Some(match self {
            Self::ReadBorrow { target, mode, .. } => Self::ReadBorrow { index, target, mode },
            Self::ReadAddress { target, .. } => Self::ReadAddress { index, target },
            Self::Backing { target, pointer, .. } => Self::Backing { index, target, pointer },
        })
    }
}

/// A prepared storage route. Structural projections retain liveness and entry
/// identities. Typed steps determine how execution crosses storage boundaries.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageReferencePath {
    projections: Arc<[StorageProjection]>,
    steps: Arc<[StorageReferenceStep]>,
}

impl StorageReferencePath {
    /// Returns the mode of the first language borrow read along the path.
    pub fn first_borrow_kind(&self) -> Option<BorrowKind> {
        self.steps.iter().find_map(|step| match step {
            StorageReferenceStep::ReadBorrow { mode, .. } => Some(*mode),
            _ => None,
        })
    }

    pub(crate) fn is_valid_for(&self, unit: crate::BoundUnitId) -> bool {
        self.projections.iter().all(|projection| projection.is_valid_for(unit))
    }

    /// Retains ordered operations prepared by type analysis. Repeated read
    /// positions represent nested pointers. A raw read consumes AddressTarget.
    pub fn new(projections: Arc<[StorageProjection]>, steps: Arc<[StorageReferenceStep]>) -> Self {
        let mut selected = 0;

        for step in steps.iter().copied() {
            let index = step.index();

            assert!(index >= selected && index <= projections.len(), "typed storage steps are ordered within their structural route: {step:?}");

            assert!(StorageProjection::remains_in_allocation(&projections[selected..index])
                && !projections[selected..index].contains(&StorageProjection::AddressTarget),
                "crossing storage requires an explicit typed route step: {projections:?}");

            let marker = match step {
                StorageReferenceStep::ReadBorrow { .. } => None,
                StorageReferenceStep::ReadAddress { .. } => Some(StorageProjection::AddressTarget),
                StorageReferenceStep::Backing { .. } => Some(StorageProjection::OwnedTarget),
            };

            if let Some(marker) = marker {
                assert_eq!(projections.get(index), Some(&marker), "typed route step consumes its structural boundary: {step:?}");
            }

            selected = index + usize::from(marker.is_some());
        }

        assert!(StorageProjection::remains_in_allocation(&projections[selected..])
            && !projections[selected..].contains(&StorageProjection::AddressTarget),
            "terminal selections cannot implicitly cross storage: {projections:?}");

        Self { projections, steps }
    }

    /// The structural component path, independently of its execution.
    pub fn projections(&self) -> &[StorageProjection] { &self.projections }

    /// The checked reads and backing selections, in execution order.
    pub fn steps(&self) -> &[StorageReferenceStep] { &self.steps }

    /// Pointer reads with their operation ordinal, structural index and reached type.
    pub fn reads(&self) -> impl DoubleEndedIterator<Item = (usize, usize, TypeId, DependencyReferenceKind)> + '_ {
        self.steps.iter().enumerate().filter_map(|(step, operation)| match *operation {
            StorageReferenceStep::ReadBorrow { index, target, .. } => Some((step, index, target, DependencyReferenceKind::Borrow)),
            StorageReferenceStep::ReadAddress { index, target } => Some((step, index, target, DependencyReferenceKind::Address)),
            StorageReferenceStep::Backing { .. } => None,
        })
    }

    /// The typed buffer descriptors which this route reads to reach their backing.
    pub fn backing_descriptors(&self) -> impl Iterator<Item = (&[StorageProjection], StructFieldSymbolId)> + '_ {
        self.steps.iter().filter_map(|step| match *step {
            StorageReferenceStep::Backing { index, pointer: Some(pointer), .. } => Some((&self.projections[..index], pointer)),
            _ => None,
        })
    }

    /// Binds selectors while keeping the prepared read positions, kinds and types.
    pub fn bind_selectors(&self, bind: impl FnMut(StorageProjection) -> StorageProjection) -> std::borrow::Cow<'_, Self> {
        use std::borrow::Cow;

        match StorageProjection::bind_selectors(&self.projections, bind) {
            Cow::Borrowed(_) => Cow::Borrowed(self),
            Cow::Owned(projections) => Cow::Owned(Self {
                projections: projections.into(),
                steps: Arc::clone(&self.steps),
            }),
        }
    }

    /// Retains a prepared prefix without reinterpreting any descriptor reads.
    pub fn prefix(&self, projections: usize, steps: usize) -> Self {
        Self { projections: self.projections[..projections].into(), steps: self.steps[..steps].into() }
    }

    /// Removes inline wrappers introduced by an argument conversion.
    pub fn strip_prefix(&self, prefix: &[StorageProjection]) -> Option<Self> {
        Some(Self { projections: self.projections.strip_prefix(prefix)?.into(),
            steps: self.steps.iter().map(|step| step.shifted(prefix.len())).collect::<Option<Vec<_>>>()?.into() })
    }

    /// Appends checked terminal language-borrow reads.
    pub fn with_terminal_borrows(&self, targets: &[(BorrowKind, TypeId)]) -> Self {
        if targets.is_empty() { return self.clone(); }

        Self { projections: Arc::clone(&self.projections), steps: self.steps.iter().copied().chain(targets.iter().map(|&(mode, target)| {
            StorageReferenceStep::ReadBorrow { index: self.projections.len(), target, mode }
        })).collect::<Vec<_>>().into() }
    }
}

impl StorageProjection {
    /// Binds an inline sequence selector through the reached input or local value.
    pub fn bind_selectors(projections: &[Self], mut bind: impl FnMut(Self) -> Self) -> std::borrow::Cow<'_, [Self]> {
        let mut bound = None;

        for (index, &projection) in projections.iter().enumerate() {
            if !matches!(projection, Self::Element(_) | Self::ElementFromInput(_)) {
                continue;
            }

            let reached = bind(projection);

            if reached != projection {
                assert!(matches!(reached, Self::ElementFromInput(_) | Self::ElementFromStart(_) | Self::AnyElement),
                    "a bound index retains the same inline sequence projection");

                bound.get_or_insert_with(|| projections.to_vec())[index] = reached;
            }
        }

        bound.map_or(std::borrow::Cow::Borrowed(projections), std::borrow::Cow::Owned)
    }
}
