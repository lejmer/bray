use bray_bound_tree::BoundUnitId;

macro_rules! define_analysis_ids {
    ($($name:ident),+ $(,)?) => {
        $(
            #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
            pub(crate) struct $name {
                unit: BoundUnitId,
                slot: u32,
            }

            impl $name {
                pub(crate) const fn from_slot(unit: BoundUnitId, slot: u32) -> Self {
                    Self { unit, slot }
                }

                pub(crate) const fn unit(self) -> BoundUnitId {
                    self.unit
                }

                pub(crate) fn to_index(self) -> Option<usize> {
                    usize::try_from(self.slot).ok()
                }

                pub(crate) fn try_from_index(unit: BoundUnitId, index: usize) -> Option<Self> {
                    u32::try_from(index)
                        .ok()
                        .map(|slot| Self::from_slot(unit, slot))
                }
            }
        )+
    };
}

define_analysis_ids! {
    AnalysisBlockId,
    AnalysisEdgeId,
    AnalysisOperationId,
    ProgramPointId,
}

/// A distinct observation epoch after an effect or a control-flow merge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AnalysisObservationSite {
    Operation(AnalysisOperationId),
    BlockEntry(AnalysisBlockId),
}

impl AnalysisObservationSite {
    pub(crate) fn to_index(self) -> Option<usize> {
        match self {
            Self::Operation(operation) => operation.to_index()?.checked_mul(2),
            Self::BlockEntry(block) => block.to_index()?.checked_mul(2)?.checked_add(1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AnalysisBlockId, AnalysisObservationSite, AnalysisOperationId};

    #[test]
    fn operation_and_join_observations_have_disjoint_identities() {
        let unit = bray_bound_tree::BoundUnitId::new(1);
        let mut identities = std::collections::BTreeSet::new();

        for slot in 0..8 {
            for site in [
                AnalysisObservationSite::Operation(AnalysisOperationId::from_slot(unit, slot)),
                AnalysisObservationSite::BlockEntry(AnalysisBlockId::from_slot(unit, slot)),
            ] {
                assert!(identities.insert(site.to_index().unwrap()));
            }
        }

        assert_eq!(identities.len(), 16);
    }
}
