use crate::{DiagnosticInterfaceSymbolIdentity, DiagnosticReceiverMode, DiagnosticType};

/// Source-facing identity of one candidate considered by semantic selection.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticSelectionCandidateIdentity {
    /// The language-defined built-in operation for the selected operand types.
    BuiltIn,
    /// An unnamed source, imported, or compiler-provided declaration.
    Declaration(DiagnosticInterfaceSymbolIdentity),
    /// A source, imported, or compiler-provided declaration with its user-facing name.
    NamedDeclaration {
        /// Stable recursive declaration identity.
        identity: DiagnosticInterfaceSymbolIdentity,
        /// Name used to select this declaration.
        name: String,
    },
    /// One iterable and iterator implementation pair.
    Iteration {
        /// Implementation supplying the iterable protocol.
        iterable: DiagnosticInterfaceSymbolIdentity,
        /// Implementation supplying the iterator protocol.
        iterator: DiagnosticInterfaceSymbolIdentity,
    },
    /// One source expression whose value is callable.
    ExpressionValue,
    /// One source pattern whose introduced value is callable.
    PatternValue,
    /// One body-local callable value.
    LocalValue,
    /// One compilation-wide callable value.
    SurfaceValue(DiagnosticInterfaceSymbolIdentity),
    /// One named compilation-wide callable value.
    NamedSurfaceValue {
        /// Stable recursive declaration identity.
        identity: DiagnosticInterfaceSymbolIdentity,
        /// Name used to select this value.
        name: String,
    },
}

/// Checked type surface of one applicable semantic-selection candidate.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticSelectionCandidateSignature {
    /// Callable candidate with its ordered parameter and result types.
    Callable {
        /// Ordinary parameter types in declaration order.
        parameter_types: Box<[DiagnosticType]>,
        /// Result type before asynchronous wrapping.
        result_type: DiagnosticType,
    },
    /// Non-call operation with source-ordered operands and an optional result.
    Operation {
        /// Operand types in source order.
        operand_types: Box<[DiagnosticType]>,
        /// Result type when the operation produces a value.
        result_type: Option<DiagnosticType>,
    },
    /// Iteration protocol with its source, cursor, and element types.
    Iteration {
        /// Iterated source type.
        source_type: DiagnosticType,
        /// Selected cursor type.
        cursor_type: DiagnosticType,
        /// Produced element type.
        element_type: DiagnosticType,
    },
}

/// One exact deterministic candidate retained by a selection diagnostic.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticSelectionCandidate {
    identity: DiagnosticSelectionCandidateIdentity,
    signature: DiagnosticSelectionCandidateSignature,
}

impl DiagnosticSelectionCandidate {
    /// Creates a candidate from its semantic identity and checked type surface.
    pub const fn new(
        identity: DiagnosticSelectionCandidateIdentity,
        signature: DiagnosticSelectionCandidateSignature,
    ) -> Self {
        Self {
            identity,
            signature,
        }
    }

    /// Returns the source-facing candidate identity.
    pub const fn identity(&self) -> &DiagnosticSelectionCandidateIdentity {
        &self.identity
    }

    /// Returns the checked type surface that made the candidate applicable.
    pub const fn signature(&self) -> &DiagnosticSelectionCandidateSignature {
        &self.signature
    }
}

/// Deterministic bounded candidate set retained by an ambiguous or inaccessible selection.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticSelectionCandidates {
    candidates: Box<[DiagnosticSelectionCandidate]>,
    omitted_count: u64,
}

impl DiagnosticSelectionCandidates {
    /// Maximum candidate records rendered and serialized for one diagnostic.
    pub const MAXIMUM: usize = 8;

    /// Creates a bounded candidate set in the checker's canonical selection order.
    pub fn new<I>(candidates: I) -> Self
    where
        I: IntoIterator<Item = DiagnosticSelectionCandidate>,
        I::IntoIter: ExactSizeIterator,
    {
        let candidates = candidates.into_iter();
        let omitted_count = candidates.len().saturating_sub(Self::MAXIMUM);

        let retained = candidates
            .take(Self::MAXIMUM)
            .collect::<Vec<_>>()
            .into_boxed_slice();

        let omitted_count = u64::try_from(omitted_count).unwrap_or(u64::MAX);

        Self {
            candidates: retained,
            omitted_count,
        }
    }

    /// Builds a bounded prefix while preserving the exact complete candidate count.
    pub fn try_from_prefix(
        candidates: impl IntoIterator<Item = DiagnosticSelectionCandidate>,
        total_count: usize,
    ) -> Result<Self, DiagnosticSelectionCandidatesBuildError> {
        let candidates = candidates.into_iter().collect::<Box<[_]>>();

        if candidates.len() > Self::MAXIMUM || total_count < candidates.len() {
            return Err(DiagnosticSelectionCandidatesBuildError);
        }

        let omitted_count = total_count - candidates.len();

        let omitted_count =
            u64::try_from(omitted_count).map_err(|_| DiagnosticSelectionCandidatesBuildError)?;

        Ok(Self {
            candidates,
            omitted_count,
        })
    }

    /// Returns every applicable candidate in deterministic order.
    pub fn candidates(&self) -> &[DiagnosticSelectionCandidate] {
        &self.candidates
    }

    /// Returns the number of additional candidates omitted from the bounded list.
    pub const fn omitted_count(&self) -> u64 {
        self.omitted_count
    }
}

/// Exact mismatch between source construction inputs and one candidate surface.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticConstructionInputRejection {
    /// A positional input follows a named input.
    PositionalAfterNamed,
    /// A named input does not occur in the candidate surface.
    UnknownName {
        /// Name supplied by the source expression.
        provided: String,
        /// Accepted candidate input names in declaration order.
        accepted: Box<[String]>,
    },
    /// A positional input maps to no positional candidate input.
    PositionalUnavailable {
        /// Zero-based source input ordinal.
        ordinal: u64,
    },
    /// The same candidate input is supplied more than once.
    Duplicate {
        /// Candidate input name when supplied by name.
        name: Option<String>,
        /// Zero-based source input ordinal of the duplicate.
        ordinal: u64,
    },
    /// A source input has the wrong value type.
    Type {
        /// Candidate input name when supplied by name.
        name: Option<String>,
        /// Zero-based source input ordinal.
        ordinal: u64,
        /// Candidate input type.
        expected: DiagnosticType,
        /// Source expression type.
        actual: DiagnosticType,
    },
    /// A required candidate input was not supplied.
    Missing {
        /// Missing candidate input name.
        name: String,
        /// Zero-based candidate input ordinal.
        ordinal: u64,
        /// Required input type.
        expected: DiagnosticType,
    },
}

/// Source receiver ownership capability participating in candidate rejection.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticReceiverCapability {
    /// Shared observation only.
    Shared,
    /// Exclusive mutation without ownership.
    Mutable,
    /// Owned value without mutable local authority.
    Owned,
    /// Owned value with mutable local authority.
    OwnedMutable,
}

/// Exact mismatch between source call arguments and one callable surface.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCallableArgumentRejection {
    /// A positional argument follows a named argument.
    PositionalAfterNamed {
        /// Zero-based source argument ordinal.
        ordinal: u64,
    },
    /// A named argument does not occur in the callable surface.
    UnknownName {
        /// Name supplied by the source expression.
        provided: String,
        /// Accepted parameter names in declaration order.
        accepted: Box<[String]>,
    },
    /// A positional argument maps to no positional parameter.
    PositionalUnavailable {
        /// Zero-based source argument ordinal.
        ordinal: u64,
    },
    /// The same parameter is supplied more than once.
    Duplicate {
        /// Parameter name when supplied by name.
        name: Option<String>,
        /// Zero-based source argument ordinal of the duplicate.
        ordinal: u64,
    },
    /// One argument has the wrong value type.
    Type {
        /// Parameter name when supplied by name.
        name: Option<String>,
        /// Zero-based source argument ordinal.
        ordinal: u64,
        /// Candidate parameter type.
        expected: DiagnosticType,
        /// Source expression type.
        actual: DiagnosticType,
    },
    /// A required parameter was not supplied.
    Missing {
        /// Missing parameter name.
        name: String,
        /// Zero-based parameter ordinal.
        ordinal: u64,
        /// Required parameter type.
        expected: DiagnosticType,
    },
}

/// Exact source-facing reason one otherwise available candidate rejected a request.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticSelectionRejectionReason {
    /// Too many explicit generic arguments were supplied.
    GenericArgumentCount {
        /// Number supplied by the source expression.
        provided: u64,
        /// Maximum accepted by the candidate.
        maximum: u64,
    },
    /// Receiver presence differs from the candidate surface.
    ReceiverPresence {
        /// Whether the source call supplies a receiver.
        provided: bool,
        /// Whether the candidate requires a receiver.
        required: bool,
    },
    /// Receiver type differs from the candidate surface.
    ReceiverType {
        /// Source receiver type.
        provided: DiagnosticType,
        /// Candidate receiver type.
        required: DiagnosticType,
    },
    /// Receiver ownership capability cannot satisfy the candidate mode.
    ReceiverCapability {
        /// Capability of the source receiver.
        provided: DiagnosticReceiverCapability,
        /// Ownership mode required by the candidate.
        required: DiagnosticReceiverMode,
    },
    /// One call argument does not match the callable surface.
    CallableArgument(DiagnosticCallableArgumentRejection),
    /// One or more supplied operand types differ from the candidate surface.
    OperandTypes {
        /// Operand types supplied by the source expression.
        provided: Box<[DiagnosticType]>,
    },
    /// One construction input does not match the candidate surface.
    ConstructionInput(DiagnosticConstructionInputRejection),
    /// The candidate operation does not match the source expression form.
    ExpressionForm,
    /// A required implementation is unavailable for the candidate.
    RequiredImplementation,
    /// A language-defined operation required by the candidate is unavailable.
    RequiredLanguageOperation,
}

/// Exact malformed named-argument shape shared by native boundary directives.

/// One deterministic rejected candidate and its exact mismatch with the request.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticRejectedSelectionCandidate {
    candidate: DiagnosticSelectionCandidate,
    reason: DiagnosticSelectionRejectionReason,
}

impl DiagnosticRejectedSelectionCandidate {
    /// Creates one rejected candidate from its identity, surface, and mismatch.
    pub const fn new(
        candidate: DiagnosticSelectionCandidate,
        reason: DiagnosticSelectionRejectionReason,
    ) -> Self {
        Self { candidate, reason }
    }

    /// Returns the rejected candidate identity and checked surface.
    pub const fn candidate(&self) -> &DiagnosticSelectionCandidate {
        &self.candidate
    }

    /// Returns the exact mismatch between the request and candidate.
    pub const fn reason(&self) -> &DiagnosticSelectionRejectionReason {
        &self.reason
    }
}

/// Deterministic bounded rejected-candidate set retained by incompatible selection.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticSelectionRejections {
    rejections: Box<[DiagnosticRejectedSelectionCandidate]>,
    omitted_count: u64,
}

impl DiagnosticSelectionRejections {
    /// Maximum rejected candidates rendered and serialized for one diagnostic.
    pub const MAXIMUM: usize = DiagnosticSelectionCandidates::MAXIMUM;

    /// Builds a bounded prefix while preserving the exact complete rejection count.
    pub fn try_from_prefix(
        rejections: impl IntoIterator<Item = DiagnosticRejectedSelectionCandidate>,
        total_count: usize,
    ) -> Result<Self, DiagnosticSelectionCandidatesBuildError> {
        let rejections = rejections.into_iter().collect::<Box<[_]>>();

        if rejections.len() > Self::MAXIMUM || rejections.len() > total_count {
            return Err(DiagnosticSelectionCandidatesBuildError);
        }

        let omitted_count = total_count - rejections.len();

        let omitted_count =
            u64::try_from(omitted_count).map_err(|_| DiagnosticSelectionCandidatesBuildError)?;

        Ok(Self {
            rejections,
            omitted_count,
        })
    }

    /// Returns the retained rejected candidates in canonical selection order.
    pub fn rejections(&self) -> &[DiagnosticRejectedSelectionCandidate] {
        &self.rejections
    }

    /// Returns the exact number of additional rejected candidates omitted by the bound.
    pub const fn omitted_count(&self) -> u64 {
        self.omitted_count
    }
}

/// Invalid bounded selection-candidate diagnostic construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticSelectionCandidatesBuildError;
