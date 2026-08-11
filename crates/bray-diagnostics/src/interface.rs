mod inventory;
mod problem;

pub use inventory::{DiagnosticInterfaceLimit, DiagnosticInterfaceSection};
pub use problem::{
    DiagnosticCheckedTemplateProblem, DiagnosticInterfaceDeclarationIdentity,
    DiagnosticInterfaceRelationshipKind, DiagnosticInterfaceSemanticProblem,
    DiagnosticInterfaceSymbolGraphProblem, DiagnosticInterfaceSymbolIdentity,
    DiagnosticInterfaceSymbolKind, DiagnosticInterfaceSymbolReference,
    DiagnosticInterfaceSynthesizedIdentity, DiagnosticSemanticContentProblem,
    DiagnosticSemanticValueKind,
};
