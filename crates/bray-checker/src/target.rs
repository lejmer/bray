use std::num::NonZeroU64;

use bray_bound_tree::BoundSourceAnchor;
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};
use bray_symbols::CallableAbi;
use bray_target::TargetProfile;

use crate::{CheckerOutcome, CheckerRequestContext};

/// One target-dependent requirement established after semantic selection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TargetValidityRequirement {
    /// Availability of one selected compiler-known representation.
    Representation(RepresentationRole),
    /// Availability of one selected callable ABI.
    CallableAbi(CallableAbi),
    /// Maximum storage alignment required by a selected layout.
    StorageAlignment(NonZeroU64),
    /// Maximum allocation alignment required by a selected layout.
    AllocationAlignment(NonZeroU64),
}

/// A source-correlated post-selection target-validity request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TargetValidityRequest {
    source: BoundSourceAnchor,
    requirement: TargetValidityRequirement,
}

impl TargetValidityRequest {
    /// Creates a target-validity request for one selected semantic requirement.
    pub const fn new(source: BoundSourceAnchor, requirement: TargetValidityRequirement) -> Self {
        Self {
            source,
            requirement,
        }
    }

    /// Returns the source construct that introduced the requirement.
    pub const fn source(self) -> BoundSourceAnchor {
        self.source
    }

    /// Returns the exact selected target requirement.
    pub const fn requirement(self) -> TargetValidityRequirement {
        self.requirement
    }
}

/// Whether one selected semantic requirement is valid for the compilation target.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TargetValidity {
    /// The selected target satisfies the requirement.
    Valid,
    /// The selected target does not satisfy the requirement.
    Invalid,
}

pub(crate) fn check_target_validity<C>(
    context: &C,
    request: TargetValidityRequest,
) -> CheckerOutcome<TargetValidity>
where
    C: CheckerRequestContext + ?Sized,
{
    if context.cancellation().is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    let source = match context.source(request.source()) {
        Ok(source) => source,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    let target = context.selected_target();

    if requirement_is_valid(target, request.requirement()) {
        return CheckerOutcome::without_diagnostics(TargetValidity::Valid);
    }

    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        diagnostic_kind(request.requirement()),
        SeverityKind::Error,
    )
    .with_primary_span(source.span());

    CheckerOutcome::complete(TargetValidity::Invalid, DiagnosticBag::single(diagnostic))
}

fn requirement_is_valid(target: &TargetProfile, requirement: TargetValidityRequirement) -> bool {
    match requirement {
        TargetValidityRequirement::Representation(role) => {
            representation_is_available(target, role)
        }
        TargetValidityRequirement::CallableAbi(CallableAbi::Bray) => true,
        TargetValidityRequirement::CallableAbi(CallableAbi::C) => target.facts().abis().c(),
        TargetValidityRequirement::CallableAbi(CallableAbi::System) => {
            target.facts().abis().system()
        }
        TargetValidityRequirement::StorageAlignment(alignment) => {
            alignment.get() <= target.facts().alignments().max_storage().get()
        }
        TargetValidityRequirement::AllocationAlignment(alignment) => {
            alignment.get() <= target.facts().alignments().max_allocation().get()
        }
    }
}

fn representation_is_available(target: &TargetProfile, role: RepresentationRole) -> bool {
    let scalars = target.facts().scalars();

    match role {
        RepresentationRole::ScalarR16 => scalars.real16(),
        RepresentationRole::ScalarR128 => scalars.real128(),
        RepresentationRole::ScalarC32 => scalars.complex32(),
        RepresentationRole::ScalarC256 => scalars.complex256(),
        _ => true,
    }
}

const fn diagnostic_kind(requirement: TargetValidityRequirement) -> DiagnosticKind {
    match requirement {
        TargetValidityRequirement::Representation(_) => {
            DiagnosticKind::CheckingTargetRepresentationUnavailable
        }
        TargetValidityRequirement::CallableAbi(_) => {
            DiagnosticKind::CheckingTargetCallableAbiUnavailable
        }
        TargetValidityRequirement::StorageAlignment(_)
        | TargetValidityRequirement::AllocationAlignment(_) => {
            DiagnosticKind::CheckingTargetAlignmentUnsupported
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use bray_compiler_known::RepresentationRole;
    use bray_diagnostics::DiagnosticKind;
    use bray_symbols::CallableAbi;

    use super::{TargetValidity, TargetValidityRequest, TargetValidityRequirement};
    use crate::CheckerOutcome;
    use crate::service::{DefaultTargetValidityChecker, TargetValidityChecker};
    use crate::test_support::{TestCheckerContext, callable_key};

    #[test]
    fn target_independent_abi_completes_without_diagnostics() {
        let context = TestCheckerContext::new(false);
        let request = TargetValidityRequest::new(
            callable_key().source(),
            TargetValidityRequirement::CallableAbi(CallableAbi::Bray),
        );

        let outcome = DefaultTargetValidityChecker.check_target_validity(&context, request);

        let CheckerOutcome::Complete(result) = outcome else {
            panic!("Bray ABI validity must complete");
        };

        assert_eq!(*result.value(), TargetValidity::Valid);
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn unavailable_selected_representations_report_their_source() {
        let context = TestCheckerContext::new(false);
        let request = TargetValidityRequest::new(
            callable_key().source(),
            TargetValidityRequirement::Representation(RepresentationRole::ScalarR16),
        );

        let outcome = DefaultTargetValidityChecker.check_target_validity(&context, request);

        let CheckerOutcome::Complete(result) = outcome else {
            panic!("target representation validity must complete");
        };

        assert_eq!(*result.value(), TargetValidity::Invalid);

        let [diagnostic] = result.diagnostics().diagnostics() else {
            panic!("an unavailable selected representation must report one diagnostic");
        };

        assert_eq!(
            diagnostic.kind(),
            DiagnosticKind::CheckingTargetRepresentationUnavailable
        );

        assert_eq!(
            diagnostic.primary_span(),
            Some(bray_source::SourceSpan::new(
                request.source().syntax().source_id(),
                request.source().syntax().full_range(),
            ))
        );
    }

    #[test]
    fn selected_layout_alignment_is_checked_against_the_target_maximum() {
        let context = TestCheckerContext::new(false);
        let alignment = NonZeroU64::new(1 << 30).unwrap_or(NonZeroU64::MIN);
        let request = TargetValidityRequest::new(
            callable_key().source(),
            TargetValidityRequirement::StorageAlignment(alignment),
        );

        let outcome = DefaultTargetValidityChecker.check_target_validity(&context, request);

        let CheckerOutcome::Complete(result) = outcome else {
            panic!("target layout validity must complete");
        };

        assert_eq!(*result.value(), TargetValidity::Invalid);
        assert_eq!(
            result.diagnostics().diagnostics()[0].kind(),
            DiagnosticKind::CheckingTargetAlignmentUnsupported
        );
    }
}
