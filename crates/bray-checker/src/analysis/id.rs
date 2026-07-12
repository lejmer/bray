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
