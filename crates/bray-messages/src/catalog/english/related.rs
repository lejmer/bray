use bray_diagnostics::DiagnosticRelatedLocationKind;

use crate::catalog::{MessageTemplate, MessageTemplatePart};

const FIRST_DECLARATION: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("first declared here")];
const FIRST_DIRECTIVE: &[MessageTemplatePart] = &[MessageTemplatePart::Text("first selected here")];
const CONFLICTING_DECLARATION: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("conflicting declaration")];
const REQUIREMENT_ORIGIN: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("requirement introduced here")];
const BORROW_ORIGIN: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "conflicting borrow established here",
)];
const DEPENDENCY_STORAGE_ORIGIN: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "referenced storage borrowed here",
)];
const MOVE_ORIGIN: &[MessageTemplatePart] = &[MessageTemplatePart::Text("storage moved here")];
const ALLOCATION_ORIGIN: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("allocation created here")];
const INITIALIZATION_ORIGIN: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("raw value initialized here")];
const DEALLOCATION_ORIGIN: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("allocation invalidated here")];
const CONFLICTING_DEPENDENCY: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "conflicting dependency selected here",
)];
const REPRESENTATION_MEMBER: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "member participating in this representation",
)];
const NON_COPYABLE_MEMBER: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("member without a copy contract")];
const REPRESENTATION_CYCLE_LOCATION: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "inline representation cycle continues here",
)];
const COVERED_BY_PATTERN: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "earlier pattern covers this case",
)];
const SELECTION_CANDIDATE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "applicable candidate declared here",
)];

pub(crate) const fn template(kind: DiagnosticRelatedLocationKind) -> MessageTemplate {
    let parts = match kind {
        DiagnosticRelatedLocationKind::FirstDeclaration => FIRST_DECLARATION,
        DiagnosticRelatedLocationKind::FirstDirective => FIRST_DIRECTIVE,
        DiagnosticRelatedLocationKind::ConflictingDeclaration => CONFLICTING_DECLARATION,
        DiagnosticRelatedLocationKind::RequirementOrigin => REQUIREMENT_ORIGIN,
        DiagnosticRelatedLocationKind::BorrowOrigin => BORROW_ORIGIN,
        DiagnosticRelatedLocationKind::DependencyStorageOrigin => DEPENDENCY_STORAGE_ORIGIN,
        DiagnosticRelatedLocationKind::MoveOrigin => MOVE_ORIGIN,
        DiagnosticRelatedLocationKind::AllocationOrigin => ALLOCATION_ORIGIN,
        DiagnosticRelatedLocationKind::InitializationOrigin => INITIALIZATION_ORIGIN,
        DiagnosticRelatedLocationKind::DeallocationOrigin => DEALLOCATION_ORIGIN,
        DiagnosticRelatedLocationKind::ConflictingDependency => CONFLICTING_DEPENDENCY,
        DiagnosticRelatedLocationKind::RepresentationMember => REPRESENTATION_MEMBER,
        DiagnosticRelatedLocationKind::NonCopyableMember => NON_COPYABLE_MEMBER,
        DiagnosticRelatedLocationKind::RepresentationCycleLocation => REPRESENTATION_CYCLE_LOCATION,
        DiagnosticRelatedLocationKind::CoveredByPattern => COVERED_BY_PATTERN,
        DiagnosticRelatedLocationKind::SelectionCandidate => SELECTION_CANDIDATE,
    };

    MessageTemplate::new(parts)
}
