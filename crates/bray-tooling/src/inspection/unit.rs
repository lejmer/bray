use std::sync::Arc;

use bray_bound_tree::BoundUnit;
use bray_compilation::{CancellationToken, Compilation, FactQueryError, QueryPriority};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_source::SourceId;

use crate::InspectionTarget;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum UnitInspectionSelectionError {
    Evaluation(FactQueryError),
    Source,
    SourceOverflow(bray_source::TextSizeOverflow),
}

pub(crate) struct UnitInspectionSelection {
    diagnostics: DiagnosticBag,
    units: Vec<Arc<DiagnosticResult<BoundUnit>>>,
}

impl UnitInspectionSelection {
    pub(crate) fn into_parts(self) -> (DiagnosticBag, Vec<Arc<DiagnosticResult<BoundUnit>>>) {
        (self.diagnostics, self.units)
    }
}

pub(crate) fn select_units(
    compilation: &Compilation,
    target: InspectionTarget,
) -> Result<UnitInspectionSelection, UnitInspectionSelectionError> {
    let source_id =
        SourceId::stored(target.source_id()).ok_or(UnitInspectionSelectionError::Source)?;

    let source = compilation
        .source(source_id)
        .ok_or(UnitInspectionSelectionError::Source)?;

    let source_length = bray_source::TextSize::try_from(source.text().len())
        .map_err(UnitInspectionSelectionError::SourceOverflow)?;

    if target
        .position()
        .is_some_and(|position| position > source_length)
    {
        return Err(UnitInspectionSelectionError::Source);
    }

    let cancellation = CancellationToken::new();

    let diagnostics = compilation
        .diagnostics_for_source(source_id, &cancellation, QueryPriority::Interactive)
        .map_err(UnitInspectionSelectionError::Evaluation)?;

    let units = match target.position() {
        Some(position) => compilation
            .bound_unit_at(
                source_id,
                position,
                &cancellation,
                QueryPriority::Interactive,
            )
            .map(|unit| unit.into_iter().collect())
            .map_err(UnitInspectionSelectionError::Evaluation)?,
        None => compilation
            .bound_units_for_source(source_id, &cancellation, QueryPriority::Interactive)
            .map_err(UnitInspectionSelectionError::Evaluation)?,
    };

    Ok(UnitInspectionSelection { diagnostics, units })
}
