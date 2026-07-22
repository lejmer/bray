use std::sync::Arc;

use bray_bound_tree::BoundSourceAnchor;
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{
    Diagnostic, DiagnosticAlignmentKind, DiagnosticArg, DiagnosticBag, DiagnosticCallableAbi,
    DiagnosticId, DiagnosticKind, DiagnosticTargetRepresentation, SeverityKind,
};
use bray_symbols::CallableAbi;
use bray_target::{
    TargetForeignAbiFacts, TargetLayoutContract, TargetProfile, TargetScalarKind, TargetValueLayout,
};

use crate::{CheckerInfrastructureError, CheckerOutcome, CheckerSource};

/// Context required to validate an already selected target-dependent requirement.
pub trait TargetValidityContext {
    /// Returns the selected target profile.
    fn selected_target(&self) -> &TargetProfile;

    /// Resolves a source anchor when a diagnostic is required.
    fn source(
        &self,
        anchor: BoundSourceAnchor,
    ) -> Result<CheckerSource<'_>, CheckerInfrastructureError>;

    /// Returns request cancellation state.
    fn cancellation(&self) -> &dyn bray_base::Cancellation;
}

impl<C> TargetValidityContext for C
where
    C: crate::CheckerRequestContext + ?Sized,
{
    fn selected_target(&self) -> &TargetProfile {
        crate::CheckerRequestContext::selected_target(self)
    }

    fn source(
        &self,
        anchor: BoundSourceAnchor,
    ) -> Result<CheckerSource<'_>, CheckerInfrastructureError> {
        crate::CheckerRequestContext::source(self, anchor)
    }

    fn cancellation(&self) -> &dyn bray_base::Cancellation {
        crate::CheckerRequestContext::cancellation(self)
    }
}

/// One value representation crossing a selected foreign ABI boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TargetAbiValue {
    /// A compiler-known scalar representation.
    Scalar(TargetScalarKind),
    /// A raw pointer value.
    RawPointer,
    /// A callable value with its own selected ABI.
    Callable(CallableAbi),
    /// A product or union with its selected physical layout.
    Aggregate(TargetValueLayout),
}

/// The complete selected callable ABI surface that must be valid for a call boundary.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetCallableAbiRequirement {
    abi: CallableAbi,
    parameters: Arc<[TargetAbiValue]>,
    result: Option<TargetAbiValue>,
}

impl TargetCallableAbiRequirement {
    /// Creates an ABI requirement from every by-value parameter and the optional value result.
    pub fn new(
        abi: CallableAbi,
        parameters: impl IntoIterator<Item = TargetAbiValue>,
        result: Option<TargetAbiValue>,
    ) -> Self {
        Self {
            abi,
            parameters: parameters.into_iter().collect(),
            result,
        }
    }

    /// Returns the selected callable ABI.
    pub const fn abi(&self) -> CallableAbi {
        self.abi
    }

    /// Returns by-value parameter representations in declaration order.
    pub fn parameters(&self) -> &[TargetAbiValue] {
        &self.parameters
    }

    /// Returns the by-value result representation, or `None` for a unit result.
    pub const fn result(&self) -> Option<TargetAbiValue> {
        self.result
    }
}

/// The target surface on which a selected layout must be representable.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TargetLayoutUse {
    /// Ordinary value storage.
    Storage,
    /// Dynamic allocation.
    Allocation,
    /// A foreign callable ABI boundary.
    CallableAbi(CallableAbi),
}

/// One selected physical layout and the target surface that must accept it.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetLayoutRequirement {
    layout: TargetValueLayout,
    usage: TargetLayoutUse,
}

impl TargetLayoutRequirement {
    /// Creates a selected layout requirement.
    pub const fn new(layout: TargetValueLayout, usage: TargetLayoutUse) -> Self {
        Self { layout, usage }
    }

    /// Returns the selected physical layout.
    pub const fn layout(self) -> TargetValueLayout {
        self.layout
    }

    /// Returns the target surface that must accept the layout.
    pub const fn usage(self) -> TargetLayoutUse {
        self.usage
    }
}

/// One target requirement established after semantic selection.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TargetValidityRequirement {
    /// Availability of one selected compiler-known representation.
    Representation(RepresentationRole),
    /// Availability and by-value acceptance of one selected callable ABI.
    CallableAbi(TargetCallableAbiRequirement),
    /// Representability of one selected physical layout.
    Layout(TargetLayoutRequirement),
}

/// A source-correlated post-selection target-validity request.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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
    pub const fn source(&self) -> BoundSourceAnchor {
        self.source
    }

    /// Returns the exact selected target requirement.
    pub const fn requirement(&self) -> &TargetValidityRequirement {
        &self.requirement
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
    request: &TargetValidityRequest,
) -> CheckerOutcome<TargetValidity>
where
    C: TargetValidityContext + ?Sized,
{
    if context.cancellation().is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    if requirement_is_unconditional(request.requirement()) {
        return CheckerOutcome::without_diagnostics(TargetValidity::Valid);
    }

    let violation = requirement_violation(context.selected_target(), request.requirement());

    let Some(violation) = violation else {
        return CheckerOutcome::without_diagnostics(TargetValidity::Valid);
    };

    let source = match context.source(request.source()) {
        Ok(source) => source,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    let diagnostic = violation
        .diagnostic(DiagnosticId::new(0))
        .with_primary_span(source.span());

    CheckerOutcome::complete(TargetValidity::Invalid, DiagnosticBag::single(diagnostic))
}

fn requirement_is_unconditional(requirement: &TargetValidityRequirement) -> bool {
    match requirement {
        TargetValidityRequirement::Representation(role) => scalar_kind(*role).is_none_or(|kind| {
            !matches!(
                kind,
                TargetScalarKind::R16
                    | TargetScalarKind::R128
                    | TargetScalarKind::C32
                    | TargetScalarKind::C256
            )
        }),
        TargetValidityRequirement::CallableAbi(requirement) => {
            requirement.abi() == CallableAbi::Bray
        }
        TargetValidityRequirement::Layout(_) => false,
    }
}

fn requirement_violation(
    target: &TargetProfile,
    requirement: &TargetValidityRequirement,
) -> Option<TargetViolation> {
    match requirement {
        TargetValidityRequirement::Representation(role) => {
            let scalar = scalar_kind(*role)?;

            (!target.facts().scalars().supports(scalar))
                .then_some(TargetViolation::Representation(scalar))
        }
        TargetValidityRequirement::CallableAbi(requirement) => {
            callable_abi_violation(target, requirement)
        }
        TargetValidityRequirement::Layout(requirement) => layout_violation(target, *requirement),
    }
}

fn callable_abi_violation(
    target: &TargetProfile,
    requirement: &TargetCallableAbiRequirement,
) -> Option<TargetViolation> {
    let Some(contract) = foreign_abi_contract(target, requirement.abi()) else {
        return Some(TargetViolation::CallableAbi(requirement.abi()));
    };

    requirement
        .parameters()
        .iter()
        .copied()
        .chain(requirement.result())
        .find_map(|value| {
            if let TargetAbiValue::Aggregate(layout) = value
                && layout.alignment().get() > contract.max_alignment().get()
            {
                return Some(TargetViolation::Alignment {
                    kind: DiagnosticAlignmentKind::CallableAbi,
                    required: layout.alignment().get(),
                    maximum: contract.max_alignment().get(),
                });
            }

            (!abi_accepts_value(contract, requirement.abi(), value)).then_some(
                TargetViolation::AbiRepresentation {
                    abi: requirement.abi(),
                    value,
                },
            )
        })
}

fn foreign_abi_contract(target: &TargetProfile, abi: CallableAbi) -> Option<TargetForeignAbiFacts> {
    match abi {
        CallableAbi::Bray => None,
        CallableAbi::C => target.facts().abis().c_contract(),
        CallableAbi::System => target.facts().abis().system_contract(),
    }
}

fn abi_accepts_value(
    contract: TargetForeignAbiFacts,
    abi: CallableAbi,
    value: TargetAbiValue,
) -> bool {
    match value {
        TargetAbiValue::Scalar(scalar) => contract.scalars().supports(scalar),
        TargetAbiValue::RawPointer => contract.raw_pointers(),
        TargetAbiValue::Callable(value_abi) => contract.qualified_callables() && value_abi == abi,
        TargetAbiValue::Aggregate(layout) => {
            layout.alignment().get() <= contract.max_alignment().get()
                && match layout.contract() {
                    TargetLayoutContract::C => contract.c_layout(),
                    TargetLayoutContract::Transparent => contract.transparent_layout(),
                    TargetLayoutContract::Default | TargetLayoutContract::Stable => false,
                }
        }
    }
}

fn layout_violation(
    target: &TargetProfile,
    requirement: TargetLayoutRequirement,
) -> Option<TargetViolation> {
    let alignment = requirement.layout().alignment().get();

    let (kind, maximum) = match requirement.usage() {
        TargetLayoutUse::Storage => (
            DiagnosticAlignmentKind::Storage,
            target.facts().alignments().max_storage().get(),
        ),
        TargetLayoutUse::Allocation => (
            DiagnosticAlignmentKind::Allocation,
            target.facts().alignments().max_allocation().get(),
        ),
        TargetLayoutUse::CallableAbi(abi) => {
            let Some(contract) = foreign_abi_contract(target, abi) else {
                return Some(TargetViolation::CallableAbi(abi));
            };

            if !abi_accepts_value(
                contract,
                abi,
                TargetAbiValue::Aggregate(requirement.layout()),
            ) {
                return Some(TargetViolation::AbiRepresentation {
                    abi,
                    value: TargetAbiValue::Aggregate(requirement.layout()),
                });
            }

            (
                DiagnosticAlignmentKind::CallableAbi,
                contract.max_alignment().get(),
            )
        }
    };

    (alignment > maximum).then_some(TargetViolation::Alignment {
        kind,
        required: alignment,
        maximum,
    })
}

#[derive(Clone, Copy)]
enum TargetViolation {
    Representation(TargetScalarKind),
    CallableAbi(CallableAbi),
    AbiRepresentation {
        abi: CallableAbi,
        value: TargetAbiValue,
    },
    Alignment {
        kind: DiagnosticAlignmentKind,
        required: u64,
        maximum: u64,
    },
}

impl TargetViolation {
    fn diagnostic(self, id: DiagnosticId) -> Diagnostic {
        match self {
            Self::Representation(scalar) => Diagnostic::new(
                id,
                DiagnosticKind::CheckingTargetRepresentationUnavailable,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::target_representation(diagnostic_scalar(
                scalar,
            ))),
            Self::CallableAbi(abi) => Diagnostic::new(
                id,
                DiagnosticKind::CheckingTargetCallableAbiUnavailable,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::callable_abi(diagnostic_abi(abi))),
            Self::AbiRepresentation { abi, value } => Diagnostic::new(
                id,
                DiagnosticKind::CheckingTargetAbiRepresentationUnsupported,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::callable_abi(diagnostic_abi(abi)))
            .with_arg(DiagnosticArg::target_representation(diagnostic_abi_value(
                value,
            ))),
            Self::Alignment {
                kind,
                required,
                maximum,
            } => Diagnostic::new(
                id,
                DiagnosticKind::CheckingTargetAlignmentUnsupported,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::alignment_kind(kind))
            .with_arg(DiagnosticArg::required_alignment(required))
            .with_arg(DiagnosticArg::maximum_alignment(maximum)),
        }
    }
}

fn scalar_kind(role: RepresentationRole) -> Option<TargetScalarKind> {
    use RepresentationRole as Role;
    use TargetScalarKind as Scalar;

    Some(match role {
        Role::ScalarBool => Scalar::Bool,
        Role::ScalarChar => Scalar::Char,
        Role::ScalarI8 => Scalar::I8,
        Role::ScalarI16 => Scalar::I16,
        Role::ScalarI32 => Scalar::I32,
        Role::ScalarI64 => Scalar::I64,
        Role::ScalarI128 => Scalar::I128,
        Role::ScalarU8 => Scalar::U8,
        Role::ScalarU16 => Scalar::U16,
        Role::ScalarU32 => Scalar::U32,
        Role::ScalarU64 => Scalar::U64,
        Role::ScalarU128 => Scalar::U128,
        Role::ScalarIsize => Scalar::Isize,
        Role::ScalarUsize => Scalar::Usize,
        Role::ScalarR16 => Scalar::R16,
        Role::ScalarR32 => Scalar::R32,
        Role::ScalarR64 => Scalar::R64,
        Role::ScalarR128 => Scalar::R128,
        Role::ScalarC32 => Scalar::C32,
        Role::ScalarC64 => Scalar::C64,
        Role::ScalarC128 => Scalar::C128,
        Role::ScalarC256 => Scalar::C256,
        _ => return None,
    })
}

fn diagnostic_abi(abi: CallableAbi) -> DiagnosticCallableAbi {
    match abi {
        CallableAbi::C => DiagnosticCallableAbi::C,
        CallableAbi::System => DiagnosticCallableAbi::System,
        CallableAbi::Bray => unreachable!("Bray ABI requirements are unconditionally valid"),
    }
}

fn diagnostic_abi_value(value: TargetAbiValue) -> DiagnosticTargetRepresentation {
    match value {
        TargetAbiValue::Scalar(scalar) => diagnostic_scalar(scalar),
        TargetAbiValue::RawPointer => DiagnosticTargetRepresentation::RawPointer,
        TargetAbiValue::Callable(_) => DiagnosticTargetRepresentation::AbiQualifiedCallable,
        TargetAbiValue::Aggregate(layout) => match layout.contract() {
            TargetLayoutContract::Default => DiagnosticTargetRepresentation::DefaultLayoutAggregate,
            TargetLayoutContract::Stable => DiagnosticTargetRepresentation::StableLayoutAggregate,
            TargetLayoutContract::C => DiagnosticTargetRepresentation::CLayoutAggregate,
            TargetLayoutContract::Transparent => {
                DiagnosticTargetRepresentation::TransparentLayoutAggregate
            }
        },
    }
}

fn diagnostic_scalar(scalar: TargetScalarKind) -> DiagnosticTargetRepresentation {
    use DiagnosticTargetRepresentation as Diagnostic;
    use TargetScalarKind as Scalar;

    match scalar {
        Scalar::Bool => Diagnostic::Bool,
        Scalar::Char => Diagnostic::Char,
        Scalar::I8 => Diagnostic::I8,
        Scalar::I16 => Diagnostic::I16,
        Scalar::I32 => Diagnostic::I32,
        Scalar::I64 => Diagnostic::I64,
        Scalar::I128 => Diagnostic::I128,
        Scalar::U8 => Diagnostic::U8,
        Scalar::U16 => Diagnostic::U16,
        Scalar::U32 => Diagnostic::U32,
        Scalar::U64 => Diagnostic::U64,
        Scalar::U128 => Diagnostic::U128,
        Scalar::Isize => Diagnostic::Isize,
        Scalar::Usize => Diagnostic::Usize,
        Scalar::R16 => Diagnostic::R16,
        Scalar::R32 => Diagnostic::R32,
        Scalar::R64 => Diagnostic::R64,
        Scalar::R128 => Diagnostic::R128,
        Scalar::C32 => Diagnostic::C32,
        Scalar::C64 => Diagnostic::C64,
        Scalar::C128 => Diagnostic::C128,
        Scalar::C256 => Diagnostic::C256,
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use bray_compiler_known::RepresentationRole;
    use bray_diagnostics::{DiagnosticArg, DiagnosticKind, DiagnosticTargetRepresentation};
    use bray_symbols::CallableAbi;
    use bray_target::{TargetLayoutContract, TargetValueLayout};

    use super::{
        TargetAbiValue, TargetCallableAbiRequirement, TargetLayoutRequirement, TargetLayoutUse,
        TargetValidity, TargetValidityRequest, TargetValidityRequirement,
    };
    use crate::CheckerOutcome;
    use crate::service::{DefaultTargetValidityChecker, TargetValidityChecker};
    use crate::test_support::{TestCheckerContext, callable_key};

    #[test]
    fn target_independent_abi_completes_without_diagnostics() {
        let context = TestCheckerContext::new(false);
        let requirement = TargetCallableAbiRequirement::new(CallableAbi::Bray, [], None);

        let request = TargetValidityRequest::new(
            callable_key().source(),
            TargetValidityRequirement::CallableAbi(requirement),
        );

        let outcome = DefaultTargetValidityChecker.check_target_validity(&context, &request);

        let CheckerOutcome::Complete(result) = outcome else {
            panic!("Bray ABI validity must complete");
        };

        assert_eq!(*result.value(), TargetValidity::Valid);
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn unavailable_representations_report_typed_requirements_and_source() {
        let context = TestCheckerContext::new(false);

        let request = TargetValidityRequest::new(
            callable_key().source(),
            TargetValidityRequirement::Representation(RepresentationRole::ScalarR16),
        );

        let outcome = DefaultTargetValidityChecker.check_target_validity(&context, &request);

        let CheckerOutcome::Complete(result) = outcome else {
            panic!("target representation validity must complete");
        };

        let [diagnostic] = result.diagnostics().diagnostics() else {
            panic!("an unavailable selected representation must report one diagnostic");
        };

        assert_eq!(*result.value(), TargetValidity::Invalid);

        assert_eq!(
            diagnostic.kind(),
            DiagnosticKind::CheckingTargetRepresentationUnavailable
        );

        assert_eq!(
            diagnostic.args(),
            [DiagnosticArg::target_representation(
                DiagnosticTargetRepresentation::R16
            )]
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
    fn foreign_abi_checks_validate_selected_by_value_representations() {
        let context = TestCheckerContext::new(false);

        let layout = TargetValueLayout::new(
            8,
            NonZeroU64::new(8).unwrap_or(NonZeroU64::MIN),
            TargetLayoutContract::Default,
        );

        let requirement = TargetCallableAbiRequirement::new(
            CallableAbi::C,
            [TargetAbiValue::Aggregate(layout)],
            None,
        );

        let request = TargetValidityRequest::new(
            callable_key().source(),
            TargetValidityRequirement::CallableAbi(requirement),
        );

        let outcome = DefaultTargetValidityChecker.check_target_validity(&context, &request);

        let CheckerOutcome::Complete(result) = outcome else {
            panic!("foreign ABI validity must complete");
        };

        assert_eq!(*result.value(), TargetValidity::Invalid);

        assert_eq!(
            result.diagnostics().diagnostics()[0].kind(),
            DiagnosticKind::CheckingTargetAbiRepresentationUnsupported
        );
    }

    #[test]
    fn selected_layout_alignment_is_checked_against_the_exact_surface() {
        let context = TestCheckerContext::new(false);
        let alignment = NonZeroU64::new(1 << 30).unwrap_or(NonZeroU64::MIN);
        let layout = TargetValueLayout::new(16, alignment, TargetLayoutContract::Stable);

        let request = TargetValidityRequest::new(
            callable_key().source(),
            TargetValidityRequirement::Layout(TargetLayoutRequirement::new(
                layout,
                TargetLayoutUse::Storage,
            )),
        );

        let outcome = DefaultTargetValidityChecker.check_target_validity(&context, &request);

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
