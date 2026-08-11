use std::sync::Arc;

use bray_bound_tree::DeclaredValueTypeTerm;
use bray_symbols::{ReceiverMode, SymbolKey, TypeId};

use super::super::ReceiverCapability;

/// Stable identity of one candidate participating in deterministic selection.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SelectionCandidateKey {
    /// The single compiler-defined rule for the request category and operand types.
    BuiltIn,
    /// A source, imported, or compiler-known declaration candidate.
    Symbol(SymbolKey),
    /// One exact iterable and iterator implementation pair.
    Iteration {
        /// The iterable implementation key.
        iterable: SymbolKey,
        /// The iterator implementation key.
        iterator: SymbolKey,
    },
    /// One local or source-correlated callable value.
    Value(DeclaredValueTypeTerm),
}

/// Checked type surface that identifies why one candidate remained applicable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectionCandidateSignature {
    /// A callable value or declaration with its callable and result types.
    Callable {
        /// Canonical callable type.
        callable_type: TypeId,
        /// Result type before asynchronous wrapping.
        result_type: TypeId,
    },
    /// A non-call semantic operation with its operand and optional result types.
    Operation {
        /// Operand types in source order.
        operand_types: Arc<[TypeId]>,
        /// Result type when the operation produces a value.
        result_type: Option<TypeId>,
    },
    /// An iteration protocol pair with its source, cursor, and element types.
    Iteration {
        /// Iterated source type.
        source_type: TypeId,
        /// Selected iterator cursor type.
        cursor_type: TypeId,
        /// Produced element type.
        element_type: TypeId,
    },
}

/// One deterministic applicable candidate retained for failure diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectionFailureCandidate {
    key: SelectionCandidateKey,
    signature: SelectionCandidateSignature,
}

/// Exact source-facing reason one otherwise available candidate rejected a request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectionCandidateRejectionReason {
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
        provided: TypeId,
        /// Candidate receiver type.
        required: TypeId,
    },
    /// Receiver ownership capability cannot satisfy the candidate mode.
    ReceiverCapability {
        /// Capability of the source receiver.
        provided: ReceiverCapability,
        /// Ownership mode required by the candidate.
        required: ReceiverMode,
    },
    /// One callable argument does not match the candidate surface.
    CallableArgument(SelectionCallableArgumentRejection),
    /// One or more operation operand types differ from the candidate surface.
    OperandTypes {
        /// Operand types supplied by the source expression.
        provided: Arc<[TypeId]>,
    },
    /// A construction input does not match the candidate's declared input surface.
    ConstructionInput(SelectionConstructionInputRejection),
    /// The selected operation does not match the source expression form.
    ExpressionForm,
    /// A required implementation is unavailable for this candidate.
    RequiredImplementation,
    /// A language-defined operation required by this candidate is unavailable.
    RequiredLanguageOperation,
}

/// Exact mismatch between source call arguments and one callable surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectionCallableArgumentRejection {
    /// A positional argument follows a named argument.
    PositionalAfterNamed { ordinal: u64 },
    /// A named argument does not occur in the callable surface.
    UnknownName {
        provided: String,
        accepted: Arc<[String]>,
    },
    /// A positional argument maps to no positional parameter.
    PositionalUnavailable { ordinal: u64 },
    /// The same parameter is supplied more than once.
    Duplicate { name: Option<String>, ordinal: u64 },
    /// One argument has the wrong value type.
    Type {
        name: Option<String>,
        ordinal: u64,
        expected: TypeId,
        actual: TypeId,
    },
    /// A required parameter was not supplied.
    Missing {
        name: String,
        ordinal: u64,
        expected: TypeId,
    },
}

/// Exact mismatch between source construction inputs and one candidate surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectionConstructionInputRejection {
    /// A positional input follows a named input.
    PositionalAfterNamed,
    /// A named input does not occur in the candidate surface.
    UnknownName {
        /// Name supplied by the source expression.
        provided: String,
        /// Accepted names in declaration order.
        accepted: Arc<[String]>,
    },
    /// A positional input maps to no positional candidate input.
    PositionalUnavailable {
        /// Zero-based source input ordinal.
        ordinal: u64,
    },
    /// The same candidate input is supplied more than once.
    Duplicate {
        /// Candidate input name when the input is named.
        name: Option<String>,
        /// Zero-based source input ordinal of the duplicate.
        ordinal: u64,
    },
    /// A source input has the wrong value type.
    Type {
        /// Candidate input name when the input is named.
        name: Option<String>,
        /// Zero-based source input ordinal.
        ordinal: u64,
        /// Candidate input type.
        expected: TypeId,
        /// Source expression type.
        actual: TypeId,
    },
    /// A required candidate input was not supplied.
    Missing {
        /// Missing candidate input name.
        name: String,
        /// Zero-based candidate input ordinal.
        ordinal: u64,
        /// Required input type.
        expected: TypeId,
    },
}

/// One deterministic rejected candidate and its exact mismatch with the request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectionRejectedCandidate {
    candidate: SelectionFailureCandidate,
    reason: SelectionCandidateRejectionReason,
}

impl SelectionRejectedCandidate {
    /// Creates one rejected candidate from its identity, surface, and exact mismatch.
    pub const fn new(
        candidate: SelectionFailureCandidate,
        reason: SelectionCandidateRejectionReason,
    ) -> Self {
        Self { candidate, reason }
    }

    /// Returns the rejected candidate identity and checked surface.
    pub const fn candidate(&self) -> &SelectionFailureCandidate {
        &self.candidate
    }

    /// Returns the exact mismatch between the request and candidate.
    pub const fn reason(&self) -> &SelectionCandidateRejectionReason {
        &self.reason
    }
}

impl SelectionFailureCandidate {
    /// Creates a candidate from its stable identity and checked signature.
    pub const fn new(key: SelectionCandidateKey, signature: SelectionCandidateSignature) -> Self {
        Self { key, signature }
    }

    /// Returns the stable candidate identity.
    pub const fn key(&self) -> &SelectionCandidateKey {
        &self.key
    }

    /// Returns the candidate's checked source-facing type surface.
    pub const fn signature(&self) -> &SelectionCandidateSignature {
        &self.signature
    }
}

/// Exact visibility failure shared by otherwise applicable candidates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionInaccessibility {
    /// Candidate declarations are not visible from the requesting source context.
    NotVisibleFromRequestingContext,
}

impl From<SymbolKey> for SelectionCandidateKey {
    fn from(key: SymbolKey) -> Self {
        Self::Symbol(key)
    }
}

/// Why semantic selection did not produce one exact operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectionFailure {
    /// No available candidate matched the request.
    Unavailable,
    /// Several applicable candidates remain after exact applicability checking.
    Ambiguous(Arc<[SelectionFailureCandidate]>),
    /// Matching candidates exist but are inaccessible in the current context.
    Inaccessible {
        /// Every deterministic otherwise-applicable candidate.
        candidates: Arc<[SelectionFailureCandidate]>,
        /// Closed reason those candidates cannot be selected.
        reason: SelectionInaccessibility,
    },
    /// Available candidates reject the supplied arguments or operands.
    Incompatible(Arc<[SelectionRejectedCandidate]>),
    /// Recovery in an input prevents a sound semantic choice.
    Recovered,
}

/// The deterministic result of selecting exactly one semantic operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CandidateSelection<T> {
    /// One exact operation was selected.
    Selected(T),
    /// Selection completed with a source-facing failure.
    Failed(SelectionFailure),
}
