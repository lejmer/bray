use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

/// One ordered parameter in a diagnostic callable signature.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticCallableParameter {
    /// The source-visible parameter name, including `_` for an unnamed input.
    pub name: String,
    /// Whether the parameter accepts positional arguments.
    pub position: crate::DiagnosticCallablePosition,
    /// The owned parameter binding's mutation authority.
    pub mode: crate::DiagnosticCallableParameterMode,
    /// The parameter type retained for rendering.
    pub ty: super::DiagnosticType,
}

impl DiagnosticArg {
    /// Creates an exact invalid native-link directive problem argument.
    pub const fn native_link_directive_problem(
        problem: crate::DiagnosticNativeLinkDirectiveProblem,
    ) -> Self {
        Self::new(
            DiagnosticArgName::NativeLinkDirectiveProblem,
            DiagnosticArgValue::NativeLinkDirectiveProblem(problem),
        )
    }

    /// Creates an exact invalid native-symbol directive problem argument.
    pub const fn native_symbol_directive_problem(
        problem: crate::DiagnosticNativeSymbolDirectiveProblem,
    ) -> Self {
        Self::new(
            DiagnosticArgName::NativeSymbolDirectiveProblem,
            DiagnosticArgValue::NativeSymbolDirectiveProblem(problem),
        )
    }

    /// Creates an exact platform-service signature mismatch argument.
    pub const fn platform_service_signature_problem(
        problem: crate::DiagnosticPlatformServiceSignatureProblem,
    ) -> Self {
        Self::new(
            DiagnosticArgName::PlatformServiceSignatureProblem,
            DiagnosticArgValue::PlatformServiceSignatureProblem(problem),
        )
    }

    /// Creates an exact trait-fulfillment mismatch argument.
    pub const fn trait_fulfillment_mismatch(
        mismatch: crate::DiagnosticTraitFulfillmentMismatch,
    ) -> Self {
        Self::new(
            DiagnosticArgName::TraitFulfillmentMismatch,
            DiagnosticArgValue::TraitFulfillmentMismatch(mismatch),
        )
    }

    /// Creates an exact invalid implementation-overload problem argument.
    pub const fn implementation_overload_problem(
        problem: crate::DiagnosticImplementationOverloadProblem,
    ) -> Self {
        Self::new(
            DiagnosticArgName::ImplementationOverloadProblem,
            DiagnosticArgValue::ImplementationOverloadProblem(problem),
        )
    }

    /// Creates an exact callable-overload arm or family problem argument.
    pub const fn callable_overload_problem(
        problem: crate::DiagnosticCallableOverloadProblem,
    ) -> Self {
        Self::new(
            DiagnosticArgName::CallableOverloadProblem,
            DiagnosticArgValue::CallableOverloadProblem(problem),
        )
    }

    /// Creates a callable ABI argument.
    pub const fn callable_abi(abi: DiagnosticCallableAbi) -> Self {
        Self::new(
            DiagnosticArgName::CallableAbi,
            DiagnosticArgValue::CallableAbi(abi),
        )
    }
}

/// Locale-neutral callable ABI modes used by target diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCallableAbi {
    /// Bray's default compiler-defined ABI.
    Bray,
    /// The target's C ABI.
    C,
    /// The target's system ABI.
    System,
}

impl DiagnosticCallableAbi {
    /// Returns the stable machine key for this ABI mode.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bray => "bray",
            Self::C => "c",
            Self::System => "system",
        }
    }
}
