use bray_diagnostics::DiagnosticBag;
use bray_emitter::{EmissionPlanningError, LinkPlanConstructionError, LinkStagingError};
use bray_package_interface::InterfaceValidationError;
use bray_symbols::ProductIdentity;
use bray_target::TargetIdentity;

use super::routing::product_emission_failure_diagnostics;
use crate::compilation::{EmissionCodegenError, PackageInterfaceExportError};
use crate::fact::FactQueryError;

/// Failure before a product reached emitter-owned publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductEmissionError {
    kind: ProductEmissionErrorKind,
    diagnostics: DiagnosticBag,
}

impl ProductEmissionError {
    pub(in crate::compilation) fn new(
        kind: ProductEmissionErrorKind,
        diagnostics: DiagnosticBag,
        product: &ProductIdentity,
        target: &TargetIdentity,
    ) -> Self {
        let diagnostics = diagnostics.merged(&product_emission_failure_diagnostics(
            &kind, product, target,
        ));

        Self { kind, diagnostics }
    }

    pub(in crate::compilation) fn cancelled() -> Self {
        Self {
            kind: ProductEmissionErrorKind::Cancelled,
            diagnostics: DiagnosticBag::new(),
        }
    }

    /// Returns the exact failed product-emission boundary.
    pub const fn kind(&self) -> &ProductEmissionErrorKind {
        &self.kind
    }

    /// Returns deterministic diagnostics completed before the failure.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }
}

/// Structured reason product emission could not reach publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProductEmissionErrorKind {
    /// Cancellation was observed before publication completed.
    Cancelled,
    /// The requested package product differs from the loaded compilation.
    ProductMismatch {
        /// Exact product requested by the emission operation.
        requested: ProductIdentity,
        /// Package loaded by the compilation.
        selected_package: bray_symbols::PackageIdentity,
        /// Product category loaded by the compilation.
        selected_kind: bray_symbols::ProductKind,
    },
    /// The request or output inputs differ from the compilation target.
    TargetMismatch {
        /// Target named by the emission request.
        requested: TargetIdentity,
        /// Target selected by the compilation.
        selected: TargetIdentity,
    },
    /// A package-interface artifact was requested without a configured export input.
    PackageInterfaceUnavailable,
    /// The configured package-interface input could not be completed.
    PackageInterface(PackageInterfaceExportError),
    /// The completed package-interface input could not be encoded.
    PackageInterfaceEncoding(InterfaceValidationError),
    /// The implementation payload companion could not be assembled.
    PackageImplementation(bray_package_interface::PackageImplementationArtifactBuildError),
    /// The implementation companion cannot be represented as artifact content.
    PackageImplementationContent(bray_codegen::ArtifactContentBuildError),
    /// The native index exceeds its bounded wire format.
    NativeIndexSizeLimitExceeded,
    /// The native library has no selected inspector toolchain.
    MissingNativeInspector,
    /// The selected LLVM inspector could not inspect a staged native unit.
    NativeInspection(super::super::native::NativeInspectionError),
    /// A requested test catalog has no matching planned artifact.
    MissingTestCatalogArtifact,
    /// The encoded test catalog cannot be represented as artifact content.
    TestCatalogContent(bray_codegen::ArtifactContentBuildError),
    /// Immutable emitter planning rejected the selected request and producer inputs.
    Planning(EmissionPlanningError),
    /// Compilation diagnostics prevent complete product publication.
    // rust-style: broad-failure
    InvalidCompilation,
    /// Planned lazy code generation could not produce complete contributions.
    Codegen(EmissionCodegenError),
    /// A linked plan has no linker and resolved product link inputs.
    MissingLinker,
    /// Link inputs were supplied for a plan without a linked product.
    UnexpectedLinker,
    /// Emitter-owned native input or output staging failed.
    Staging(LinkStagingError),
    /// Resolved product, runtime, staging, and target properties could not form a link plan.
    LinkPlan(LinkPlanConstructionError),
    /// Lazy input or bounded operation scheduling failed.
    Query(FactQueryError),
}
