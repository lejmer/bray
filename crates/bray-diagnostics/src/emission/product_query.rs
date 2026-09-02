mod failure;
mod kind;

pub use failure::DiagnosticProductQueryFailure;
pub use kind::{
    DiagnosticProductDataKind, DiagnosticProductQueryContext, DiagnosticProductQueryContextKind,
    DiagnosticProductValueKind,
};
