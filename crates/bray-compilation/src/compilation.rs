mod behavior;
mod binder;
mod boundary;
mod checker;
mod codegen;
mod configuration;
mod constant;
mod contribution_gate;
mod declaration_body;
mod diagnostics;
mod directive;
mod emission;
mod execution_guarantees;
mod export;
mod foreign;
mod generic_constraint;
mod implementation;
mod imported;
mod input;
mod iteration;
mod limits;
mod linking;
mod load;
mod lowering;
mod lowering_diagnostic;
mod operation;
mod overlap;
mod overload;
mod pattern;
mod product;
mod product_emission;
mod representation;
mod semantic_error;
mod snapshot;
mod source_graph;
mod source_module;
mod standard_library;
mod state;
mod substitution;
mod symbol_surface;
mod testing;
mod tooling;
mod type_representation;
mod type_surface;
mod unit;

pub use codegen::CodegenPreparationError;
pub use configuration::BuildConfiguration;
pub use emission::{EmissionCodegenError, EmissionCodegenErrorKind};
pub use export::{
    PackageInterfaceExportContract, PackageInterfaceExportError,
    PackageInterfaceInvalidCompilationCause,
};
pub(crate) use foreign::{
    ForeignDataKind, ForeignIntegerWidth, ForeignQueryContext, ForeignQueryFailure,
    ForeignSourceRole, ForeignTypeKind,
};
pub use foreign::{ForeignQueryError, ForeignQueryErrorKind};
pub(crate) use implementation::ImplementationMatchError;
pub use linking::LinkedProductEmissionError;
pub use load::CompilationLoadError;
#[cfg(test)]
pub(crate) use product::ProductSynchronizationComponent;
pub use product::{
    NativeProductPlan, NativeProductPlanningError, ProductQueryError, ProductQueryErrorKind,
};
pub(crate) use product::{
    ProductDataKind, ProductQueryContext, ProductQueryFailure, ProductTestCatalogFailureKind,
    ProductValueKind,
};
pub(crate) use product_emission::diagnostics::diagnostic_evaluation_failure;
pub use product_emission::{ProductEmissionError, ProductEmissionErrorKind, ProductEmissionInputs};
pub use semantic_error::{SemanticQueryError, SemanticQueryErrorKind};
pub use source_graph::ProductSourceGraph;
pub use state::Compilation;
pub use testing::TestDiscovery;
pub use tooling::{CompletionCandidate, SemanticAvailability};
pub use unit::{
    AsyncAnalysisView, DependencyContractsView, ExpressionTypesView, LiteralValuesView,
    LivenessView, RefinementsView, SemanticSelectionsView, StorageFlowView,
};

pub(crate) use semantic_error::{
    SemanticDataKind, SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation,
    SemanticSymbolCategory,
};
