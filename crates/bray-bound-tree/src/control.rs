mod completion;
mod facts;
mod iteration;
mod pattern;
mod transfer;
mod validation;

pub use completion::{ControlCompletion, ControlCompletionKind};
pub use facts::{
    CheckedControlFlowFacts, CheckedControlFlowFactsBuildError, CheckedControlFlowFactsBuilder,
};
pub use iteration::{
    CheckedForIterationFacts, CheckedForIterationProtocol, CheckedForIterationResolution,
    CheckedIterationProtocolError, IterationAccessMode,
};
pub use pattern::{
    CheckedMatchFacts, CheckedPatternChildProjection, CheckedPatternFacts,
    CheckedPatternResolution, MatchExhaustiveness, PatternProjection, PatternTest,
};
pub use transfer::{
    CheckedBlockResult, CheckedBlockResultRole, CheckedControlTransfer,
    CheckedControlTransferTarget,
};
