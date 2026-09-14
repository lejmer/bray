use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use bray_bound_tree::{
    BoundUnit, CheckedBodyBehavior, CheckedBodySemantics, CheckedControlFlow,
    CheckedExpressionSemantics, CheckedMemoryOperations, CheckedPatterns,
    DeclaredValueTypeTemplates, SelectedIterationSource, StoragePlan,
};
use bray_checker::{TargetValidity, TargetValidityRequest};
use bray_codegen::{CodegenConfiguration, CodegenOutcome};
use bray_declarations::{DeclarationChunkResult, DeclarationTableResult, ModulePartId};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_package_interface::{
    ImportedSemanticRecord, ImportedSemantics, PackageInterfaceExportBundle,
};
use bray_parser::{SourceUnitSyntaxResult, SyntaxTreeResult};
use bray_source::SourceStore;
use bray_symbols::{
    CallableTypeDirectiveKey, CompilerKnownSymbolProvider, ConstantExpressionExpectedType,
    ConstantExpressionOccurrenceKey, ConstantTermId, DeclaredTypeRepresentation, DirectiveSurface,
    ForeignCallableContract, ForeignStaticContract, FunctionSymbolId,
    GenericConstraintObligationKey, ImplementationCandidateSet, ImplementationCoherenceDomainKey,
    ImplementationParticipationQuery, ImplementationRequirementKey, ImplementationSelection,
    ImplementationSymbolId, ImportedSemanticAddress, ImportedSymbolSkeleton, NamedTypeSymbolId,
    PackageIdentity, ProductIdentity, ProductSemantics, ProofOutcome, SemanticValueStore,
    SemanticValueStoreCreateError, StaticSymbolId, SymbolGraph,
    TraitImplementationConformanceQuery, TypeAssociatedSurface, TypeId,
};

use crate::fact::{
    BoundUnitIdentityMap, CancellationToken, ConstantInstanceQueryKey, FactCell, FactCellMap,
    FactQueryError, FactRuntime, ImportedSemanticRecordKey, UnitQueryCache,
};
use crate::request::{CompilationOptions, DependencyInterfaceInput, PackageInterfaceExportRequest};

use crate::compilation::binder::CompilationSymbolSemantics;
use crate::compilation::source_graph::ProductSourceGraph;
use crate::compilation::testing::TestDiscovery;

/// Durable immutable compilation context and demand-driven query entrypoint.
#[derive(Clone)]
pub struct Compilation {
    pub(in crate::compilation) state: Arc<CompilationState>,
}

pub(in crate::compilation) struct CompilationState {
    pub(in crate::compilation) package_identity: PackageIdentity,
    pub(in crate::compilation) package_source_authority: crate::PackageSourceAuthority,
    pub(in crate::compilation) standard_library:
        Option<bray_standard_library::StandardLibraryResolver>,
    pub(in crate::compilation) standard_library_providers:
        Option<bray_standard_library::StandardLibraryResolver>,
    pub(in crate::compilation) options: CompilationOptions,
    pub(in crate::compilation) sources: SourceStore,
    pub(in crate::compilation) source_diagnostics: DiagnosticBag,
    pub(in crate::compilation) package_interface_export: Option<PackageInterfaceExportRequest>,
    pub(in crate::compilation) profile_product: Option<ProductIdentity>,
    pub(in crate::compilation) dependency_interfaces: Box<[DependencyInterfaceInput]>,
    pub(in crate::compilation) platform_services:
        Box<[bray_runtime_interface::PlatformServiceBinding]>,
    pub(in crate::compilation) runtime_roles:
        Box<[bray_runtime_interface::RuntimeRoleSourceBinding]>,
    pub(in crate::compilation) fact_runtime: FactRuntime,
    pub(in crate::compilation) cancellation: CancellationToken,
    pub(in crate::compilation) source_unit_syntax: Vec<FactCell<SourceUnitSyntaxResult>>,
    pub(in crate::compilation) syntax_tree_result: FactCell<SyntaxTreeResult>,
    pub(in crate::compilation) declaration_chunks: Vec<FactCell<DeclarationChunkResult>>,
    pub(in crate::compilation) source_reference_indexes:
        Vec<FactCell<crate::compilation::tooling::SourceReferenceIndex>>,
    pub(in crate::compilation) declaration_table_result: FactCell<DeclarationTableResult>,
    pub(in crate::compilation) product_source_graph:
        FactCell<Result<ProductSourceGraph, FactQueryError>>,
    pub(in crate::compilation) product_semantics: FactCell<DiagnosticResult<ProductSemantics>>,
    pub(in crate::compilation) test_discoveries:
        FactCellMap<ProductIdentity, Arc<DiagnosticResult<TestDiscovery>>>,
    pub(in crate::compilation) compiler_known_symbols: Arc<CompilerKnownSymbolProvider>,
    pub(in crate::compilation) selected_target: crate::SelectedTargetContext,
    pub(in crate::compilation) target_validity:
        FactCellMap<TargetValidityRequest, Arc<DiagnosticResult<TargetValidity>>>,
    pub(in crate::compilation) module_contribution_gates:
        FactCellMap<ModulePartId, Arc<DiagnosticResult<bray_symbols::ModuleContributionGate>>>,
    pub(in crate::compilation) callable_type_directives:
        FactCellMap<CallableTypeDirectiveKey, Arc<DiagnosticResult<DirectiveSurface>>>,
    pub(in crate::compilation) bound_unit_identities:
        OnceLock<Result<BoundUnitIdentityMap, FactQueryError>>,
    pub(in crate::compilation) discovery_symbol_graph:
        FactCell<Result<SymbolGraph, FactQueryError>>,
    pub(in crate::compilation) symbol_graph: FactCell<Result<SymbolGraph, FactQueryError>>,
    pub(in crate::compilation) semantic_values:
        OnceLock<Result<SemanticValueStore, SemanticValueStoreCreateError>>,
    pub(in crate::compilation) loaded_dependency_interfaces:
        Vec<FactCell<crate::compilation::imported::LoadedDependencyInterface>>,
    pub(in crate::compilation) loaded_dependency_implementations: Vec<
        FactCell<
            DiagnosticResult<Option<Arc<bray_package_interface::PackageImplementationArtifact>>>,
        >,
    >,
    pub(in crate::compilation) imported_symbol_skeleton:
        FactCell<DiagnosticResult<Option<Arc<ImportedSymbolSkeleton>>>>,
    pub(in crate::compilation) imported_semantic_graphs:
        Vec<FactCell<DiagnosticResult<Option<Arc<ImportedSemantics>>>>>,
    pub(in crate::compilation) imported_semantics: FactCellMap<
        ImportedSemanticRecordKey,
        Arc<DiagnosticResult<Arc<[ImportedSemanticRecord]>>>,
    >,
    pub(in crate::compilation) imported_constant_callable_bodies: FactCellMap<
        ImportedSemanticAddress,
        Arc<DiagnosticResult<Option<Arc<bray_bound_tree::CheckedTemplate>>>>,
    >,
    pub(in crate::compilation) imported_executable_templates: FactCellMap<
        crate::fact::ImportedExecutableTemplateAddress,
        Arc<DiagnosticResult<Option<Arc<bray_ir::MirUnit>>>>,
    >,
    pub(in crate::compilation) imported_diagnostics: FactCell<DiagnosticBag>,
    pub(in crate::compilation) implementation_participation: FactCellMap<
        ImplementationCoherenceDomainKey,
        Arc<
            DiagnosticResult<
                <ImplementationParticipationQuery as bray_symbols::SemanticQueryContract>::Value,
            >,
        >,
    >,
    pub(in crate::compilation) implementation_coherence: FactCell<DiagnosticBag>,
    pub(in crate::compilation) callable_overload_validation: FactCell<DiagnosticBag>,
    pub(in crate::compilation) foreign_callable_contracts:
        FactCellMap<FunctionSymbolId, Arc<DiagnosticResult<Option<ForeignCallableContract>>>>,
    pub(in crate::compilation) foreign_static_contracts:
        FactCellMap<StaticSymbolId, Arc<DiagnosticResult<Option<ForeignStaticContract>>>>,
    pub(in crate::compilation) foreign_callable_validation: FactCell<DiagnosticBag>,
    pub(in crate::compilation) type_associated_surfaces:
        FactCellMap<NamedTypeSymbolId, Arc<DiagnosticResult<TypeAssociatedSurface>>>,
    pub(in crate::compilation) declared_type_representations:
        FactCellMap<NamedTypeSymbolId, Arc<DiagnosticResult<DeclaredTypeRepresentation>>>,
    pub(in crate::compilation) codegen_lifecycle_needs:
        Mutex<BTreeMap<TypeId, crate::compilation::product::CodegenLifecycleNeeds>>,
    pub(in crate::compilation) type_associated_implementation_index:
        FactCell<crate::compilation::type_surface::InherentImplementationAssociationIndex>,
    pub(in crate::compilation) implementation_index: FactCell<
        DiagnosticResult<Arc<crate::compilation::implementation::ImplementationHeaderIndex>>,
    >,
    pub(in crate::compilation) implementation_candidate_sets: FactCellMap<
        ImplementationRequirementKey,
        Arc<DiagnosticResult<ImplementationCandidateSet>>,
    >,
    pub(in crate::compilation) trait_implementation_conformance: FactCellMap<
        ImplementationSymbolId,
        Arc<
            DiagnosticResult<
                <TraitImplementationConformanceQuery as bray_symbols::SemanticQueryContract>::Value,
            >,
        >,
    >,
    pub(in crate::compilation) generic_constraint_satisfaction:
        FactCellMap<GenericConstraintObligationKey, Arc<DiagnosticResult<ProofOutcome>>>,
    pub(in crate::compilation) implementation_selections:
        FactCellMap<ImplementationRequirementKey, Arc<DiagnosticResult<ImplementationSelection>>>,
    pub(in crate::compilation) iteration_sources: FactCellMap<
        crate::fact::IterationSourceQueryKey,
        Arc<DiagnosticResult<Option<SelectedIterationSource>>>,
    >,
    pub(in crate::compilation) semantic_diagnostics: FactCell<DiagnosticBag>,
    pub(in crate::compilation) symbol_semantics: CompilationSymbolSemantics,
    pub(in crate::compilation) discovery_symbol_semantics: CompilationSymbolSemantics,
    pub(in crate::compilation) bound_units: UnitQueryCache<BoundUnit>,
    pub(in crate::compilation) declared_value_type_templates:
        UnitQueryCache<DeclaredValueTypeTemplates>,
    pub(in crate::compilation) checked_control_flow: UnitQueryCache<CheckedControlFlow>,
    pub(in crate::compilation) provisional_expression_semantics:
        UnitQueryCache<CheckedExpressionSemantics>,
    pub(in crate::compilation) expression_semantics: UnitQueryCache<CheckedExpressionSemantics>,
    pub(in crate::compilation) checked_patterns: UnitQueryCache<CheckedPatterns>,
    pub(in crate::compilation) storage_plans: UnitQueryCache<StoragePlan>,
    pub(in crate::compilation) memory_operations: UnitQueryCache<CheckedMemoryOperations>,
    pub(in crate::compilation) execution_candidates:
        UnitQueryCache<bray_checker::ExecutionCandidates>,
    pub(in crate::compilation) certified_execution:
        UnitQueryCache<bray_checker::ExecutionCertification>,
    pub(in crate::compilation) body_semantics: UnitQueryCache<CheckedBodySemantics>,
    pub(in crate::compilation) checked_body_behaviors: UnitQueryCache<CheckedBodyBehavior>,
    pub(in crate::compilation) lowered_units: UnitQueryCache<Option<bray_lowering::LoweredUnit>>,
    pub(in crate::compilation) codegen: Option<CodegenConfiguration>,
    pub(in crate::compilation) codegen_artifacts:
        FactCellMap<crate::fact::CodegenArtifactQueryKey, Arc<CodegenOutcome>>,
    pub(in crate::compilation) native_products: FactCellMap<
        crate::fact::NativeProductQueryKey,
        Result<
            Arc<crate::compilation::NativeProductPlan>,
            Arc<crate::compilation::NativeProductPlanningError>,
        >,
    >,
    pub(in crate::compilation) declared_units:
        FactCell<Result<crate::compilation::unit::DeclaredUnitIndex, FactQueryError>>,
    pub(in crate::compilation) symbolic_constant_terms: UnitQueryCache<ConstantTermId>,
    pub(in crate::compilation) embedded_constant_expectations:
        Mutex<BTreeMap<ConstantExpressionOccurrenceKey, ConstantExpressionExpectedType>>,
    pub(in crate::compilation) constant_instances: FactCellMap<
        ConstantInstanceQueryKey,
        Arc<DiagnosticResult<bray_checker::EvaluatedConstantCall>>,
    >,
    pub(in crate::compilation) constant_calls: FactCellMap<
        crate::fact::ConstantCallQueryKey,
        Arc<DiagnosticResult<Option<bray_checker::EvaluatedConstantCall>>>,
    >,
    pub(in crate::compilation) check_diagnostics: FactCell<DiagnosticBag>,
    pub(in crate::compilation) package_interface_export_bundle: FactCell<
        Result<
            Arc<PackageInterfaceExportBundle>,
            crate::compilation::export::PackageInterfaceExportError,
        >,
    >,
}
