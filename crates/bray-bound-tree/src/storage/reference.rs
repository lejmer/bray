use std::sync::Arc;

use bray_symbols::{DependencyReferenceKind, StructFieldSymbolId, TypeId};

use crate::StorageProjection;

/// A typed operation between ordinary component selections. These steps describe
/// reads, never ownership, initialization or allocation authority.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageReferenceStep {
    /// Read the selected descriptor, then follow that evaluated pointer value.
    Read { index: usize, target: TypeId, kind: DependencyReferenceKind },
    /// Select an owning allocation's backing. A represented buffer reads its
    /// prepared pointer field. Other owning indirections have structural backing.
    Backing { index: usize, target: TypeId, pointer: Option<StructFieldSymbolId> },
}

impl StorageReferenceStep {
    /// The structural projection preceding this operation.
    pub fn index(self) -> usize {
        match self { Self::Read { index, .. } | Self::Backing { index, .. } => index }
    }

    fn shifted(self, prefix: usize) -> Option<Self> {
        Some(match self {
            Self::Read { index, target, kind } => Self::Read { index: index.checked_sub(prefix)?, target, kind },
            Self::Backing { index, target, pointer } => Self::Backing { index: index.checked_sub(prefix)?, target, pointer },
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
                StorageReferenceStep::Read { kind: DependencyReferenceKind::Borrow, .. } => None,
                StorageReferenceStep::Read { kind: DependencyReferenceKind::Address, .. } => Some(StorageProjection::AddressTarget),
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
            StorageReferenceStep::Read { index, target, kind } => Some((step, index, target, kind)),
            StorageReferenceStep::Backing { .. } => None,
        })
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
    pub fn with_terminal_borrows(&self, targets: &[TypeId]) -> Self {
        if targets.is_empty() { return self.clone(); }

        Self { projections: Arc::clone(&self.projections), steps: self.steps.iter().copied().chain(targets.iter().map(|target| {
            StorageReferenceStep::Read { index: self.projections.len(), target: *target, kind: DependencyReferenceKind::Borrow }
        })).collect::<Vec<_>>().into() }
    }
}
