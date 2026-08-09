use super::model::{
    CompilationProfileCategory, CompilationProfileMode, CompilationProfileOutcome,
    CompilationProfileSubjectKind, CompilationProfileUnit,
};
use crate::fact::CompilationFactKey;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum ProfileOperation {
    CompilationLoad,
    QueryEvaluation,
    SchedulerQueue,
    DependencyWait,
    Lowering,
    CodeGeneration,
    Linking,
    Emission,
    InterfaceExport,
}

impl ProfileOperation {
    pub(crate) const COUNT: usize = 9;

    pub(crate) const fn index(self) -> usize {
        self as usize
    }

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::CompilationLoad => "compiler.compilation.load",
            Self::QueryEvaluation => "compiler.query.evaluate",
            Self::SchedulerQueue => "compiler.scheduler.queue",
            Self::DependencyWait => "compiler.query.wait",
            Self::Lowering => "compiler.lower",
            Self::CodeGeneration => "compiler.codegen.generate",
            Self::Linking => "compiler.link",
            Self::Emission => "compiler.emit",
            Self::InterfaceExport => "compiler.interface.export",
        }
    }

    pub(crate) const fn id(self) -> u16 {
        self as u16 + 1
    }

    pub(crate) const fn category(self) -> CompilationProfileCategory {
        match self {
            Self::SchedulerQueue | Self::DependencyWait => CompilationProfileCategory::Wait,
            Self::Linking => CompilationProfileCategory::External,
            Self::CompilationLoad
            | Self::QueryEvaluation
            | Self::Lowering
            | Self::CodeGeneration
            | Self::Emission
            | Self::InterfaceExport => CompilationProfileCategory::Work,
        }
    }

    pub(crate) const fn allowed_subjects(self) -> &'static [CompilationProfileSubjectKind] {
        use CompilationProfileSubjectKind::{
            Artifact, CodegenUnit, Compilation, Product, SemanticUnit,
        };

        match self {
            Self::CompilationLoad => &[Compilation],
            Self::QueryEvaluation | Self::DependencyWait => &[
                Compilation,
                CompilationProfileSubjectKind::SourceUnit,
                SemanticUnit,
                CodegenUnit,
                Product,
            ],
            Self::SchedulerQueue => &[Compilation],
            Self::Lowering => &[SemanticUnit],
            Self::CodeGeneration => &[CodegenUnit],
            Self::Linking | Self::InterfaceExport => &[Product],
            Self::Emission => &[Product, Artifact],
        }
    }

    pub(crate) const fn all() -> [Self; Self::COUNT] {
        [
            Self::CompilationLoad,
            Self::QueryEvaluation,
            Self::SchedulerQueue,
            Self::DependencyWait,
            Self::Lowering,
            Self::CodeGeneration,
            Self::Linking,
            Self::Emission,
            Self::InterfaceExport,
        ]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum ProfileMetricKind {
    SourceUnits,
    SourceBytes,
    SyntaxTokens,
    Declarations,
    BoundUnits,
    CheckedBodies,
    MirUnits,
    MirBlocks,
    MirOperations,
    ConcreteInstances,
    CodegenUnits,
    InterfaceSections,
    InterfaceBytes,
    LinkInputs,
    EmittedArtifacts,
    EmittedBytes,
}

impl ProfileMetricKind {
    pub(crate) const COUNT: usize = 16;

    pub(crate) const fn index(self) -> usize {
        self as usize
    }

    pub(crate) const fn descriptor(self) -> (&'static str, CompilationProfileUnit) {
        match self {
            Self::SourceUnits => ("compiler.source.units", CompilationProfileUnit::Count),
            Self::SourceBytes => ("compiler.source.bytes", CompilationProfileUnit::Bytes),
            Self::SyntaxTokens => ("compiler.syntax.tokens", CompilationProfileUnit::Count),
            Self::Declarations => ("compiler.declarations", CompilationProfileUnit::Count),
            Self::BoundUnits => ("compiler.bound.units", CompilationProfileUnit::Count),
            Self::CheckedBodies => ("compiler.checked.bodies", CompilationProfileUnit::Count),
            Self::MirUnits => ("compiler.mir.units", CompilationProfileUnit::Count),
            Self::MirBlocks => ("compiler.mir.blocks", CompilationProfileUnit::Count),
            Self::MirOperations => ("compiler.mir.operations", CompilationProfileUnit::Count),
            Self::ConcreteInstances => (
                "compiler.codegen.instances",
                CompilationProfileUnit::Count,
            ),
            Self::CodegenUnits => ("compiler.codegen.units", CompilationProfileUnit::Count),
            Self::InterfaceSections => (
                "compiler.interface.sections",
                CompilationProfileUnit::Count,
            ),
            Self::InterfaceBytes => ("compiler.interface.bytes", CompilationProfileUnit::Bytes),
            Self::LinkInputs => ("compiler.link.inputs", CompilationProfileUnit::Count),
            Self::EmittedArtifacts => (
                "compiler.emitted.artifacts",
                CompilationProfileUnit::Count,
            ),
            Self::EmittedBytes => ("compiler.emitted.bytes", CompilationProfileUnit::Bytes),
        }
    }

    pub(crate) const fn id(self) -> u16 {
        self as u16 + 2_000
    }

    pub(crate) const fn allowed_subjects(self) -> &'static [CompilationProfileSubjectKind] {
        use CompilationProfileSubjectKind::{
            Artifact, CodegenUnit, Compilation, Product, SemanticUnit, SourceUnit,
        };

        match self {
            Self::SourceUnits | Self::SourceBytes | Self::SyntaxTokens => &[SourceUnit],
            Self::Declarations => &[Compilation, SourceUnit],
            Self::BoundUnits
            | Self::CheckedBodies
            | Self::MirUnits
            | Self::MirBlocks
            | Self::MirOperations => &[SemanticUnit],
            Self::ConcreteInstances | Self::CodegenUnits => &[CodegenUnit],
            Self::InterfaceSections | Self::InterfaceBytes | Self::LinkInputs => &[Product],
            Self::EmittedArtifacts | Self::EmittedBytes => &[Product, Artifact],
        }
    }

    pub(crate) const fn all() -> [Self; Self::COUNT] {
        [
            Self::SourceUnits,
            Self::SourceBytes,
            Self::SyntaxTokens,
            Self::Declarations,
            Self::BoundUnits,
            Self::CheckedBodies,
            Self::MirUnits,
            Self::MirBlocks,
            Self::MirOperations,
            Self::ConcreteInstances,
            Self::CodegenUnits,
            Self::InterfaceSections,
            Self::InterfaceBytes,
            Self::LinkInputs,
            Self::EmittedArtifacts,
            Self::EmittedBytes,
        ]
    }
}

macro_rules! define_profile_query_kinds {
    ($( $variant:ident => $name:literal, )+) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        #[repr(u8)]
        pub(crate) enum ProfileQueryKind {
            $( $variant, )+
        }

        impl ProfileQueryKind {
            pub(crate) const COUNT: usize = [$( Self::$variant, )+].len();

            pub(crate) const fn index(self) -> usize {
                self as usize
            }

            pub(crate) const fn all() -> [Self; Self::COUNT] {
                [$( Self::$variant, )+]
            }

            pub(crate) const fn name(self) -> &'static str {
                match self {
                    $( Self::$variant => $name, )+
                }
            }

            pub(crate) const fn id(self) -> u16 {
                self as u16 + 1_000
            }
        }
    };
}

define_profile_query_kinds! {
    SelectedTarget => "selected_target",
    TargetValidity => "target_validity",
    ModuleContributionGate => "module_contribution_gate",
    CallableTypeDirectives => "callable_type_directives",
    CompilerKnownSymbols => "compiler_known_symbols",
    BoundUnitIdentities => "bound_unit_identities",
    BoundUnit => "bound_unit",
    CheckDiagnostics => "check_diagnostics",
    ConstantTemplateKeys => "constant_template_keys",
    CallableBodyKeys => "callable_body_keys",
    PredicateDefinitionKeys => "predicate_definition_keys",
    ConstantInstance => "constant_instance",
    ConstantCall => "constant_call",
    ConstantCallCycle => "constant_call_cycle",
    CheckedControlFlow => "checked_control_flow",
    CheckedExpressionTypes => "checked_expression_types",
    CheckedLiteralValues => "checked_literal_values",
    CheckedPatterns => "checked_patterns",
    CheckedSemanticSelections => "checked_semantic_selections",
    StoragePlan => "storage_plan",
    Liveness => "liveness",
    RefinementFacts => "refinement_facts",
    StorageFlowFacts => "storage_flow_facts",
    DependencyContracts => "dependency_contracts",
    MemoryOperations => "memory_operations",
    AsyncFacts => "async_facts",
    BodyBehaviorContributions => "body_behavior_contributions",
    CheckedBodyBehavior => "checked_body_behavior",
    LoweredUnit => "lowered_unit",
    CodegenArtifact => "codegen_artifact",
    NativeProduct => "native_product",
    DeclaredValueTypeTemplates => "declared_value_type_templates",
    ExpressionSemantics => "expression_semantics",
    ProvisionalExpressionSemantics => "provisional_expression_semantics",
    SymbolicConstantTerm => "symbolic_constant_term",
    DeclarationChunk => "declaration_chunk",
    DeclarationTable => "declaration_table",
    ProductSourceGraph => "product_source_graph",
    ProductSemantics => "product_semantics",
    TestDiscovery => "test_discovery",
    DiscoverySymbolGraph => "discovery_symbol_graph",
    DependencyInterface => "dependency_interface",
    DependencyImplementation => "dependency_implementation",
    ImportedDiagnostics => "imported_diagnostics",
    ImplementationHeaderIndex => "implementation_header_index",
    ImplementationCandidateSet => "implementation_candidate_set",
    TraitImplementationConformance => "trait_implementation_conformance",
    GenericConstraintSatisfaction => "generic_constraint_satisfaction",
    ImplementationSelection => "implementation_selection",
    IterationSource => "iteration_source",
    OperationSelection => "operation_selection",
    ImportedSemanticGraph => "imported_semantic_graph",
    ImportedSemanticFact => "imported_semantic_fact",
    ImportedConstantCallableBody => "imported_constant_callable_body",
    ImportedExecutableTemplate => "imported_executable_template",
    ImplementationParticipation => "implementation_participation",
    ImplementationCoherence => "implementation_coherence",
    CallableOverloadValidation => "callable_overload_validation",
    ForeignCallableContract => "foreign_callable_contract",
    ForeignCallableValidation => "foreign_callable_validation",
    TypeAssociatedSurface => "type_associated_surface",
    DeclaredTypeRepresentation => "declared_type_representation",
    TypeAssociatedImplementationIndex => "type_associated_implementation_index",
    ImportedSymbolSkeleton => "imported_symbol_skeleton",
    PackageInterfaceExportBundle => "package_interface_export_bundle",
    SemanticValueStore => "semantic_value_store",
    SemanticDiagnostics => "semantic_diagnostics",
    SourceUnitSyntax => "source_unit_syntax",
    SourceReferenceIndex => "source_reference_index",
    SymbolGraph => "symbol_graph",
    Symbol => "symbol",
    SyntaxTree => "syntax_tree",
}

impl ProfileQueryKind {
    pub(crate) const fn completed_unit_metric(self) -> Option<ProfileMetricKind> {
        match self {
            Self::BoundUnit => Some(ProfileMetricKind::BoundUnits),
            Self::CheckedBodyBehavior => Some(ProfileMetricKind::CheckedBodies),
            _ => None,
        }
    }

    pub(crate) const fn from_key(key: &CompilationFactKey) -> Self {
        match key {
            CompilationFactKey::SelectedTarget => Self::SelectedTarget,
            CompilationFactKey::TargetValidity(_) => Self::TargetValidity,
            CompilationFactKey::ModuleContributionGate(_) => Self::ModuleContributionGate,
            CompilationFactKey::CallableTypeDirectives(_) => Self::CallableTypeDirectives,
            CompilationFactKey::CompilerKnownSymbols => Self::CompilerKnownSymbols,
            CompilationFactKey::BoundUnitIdentities => Self::BoundUnitIdentities,
            CompilationFactKey::BoundUnit(_) => Self::BoundUnit,
            CompilationFactKey::CheckDiagnostics => Self::CheckDiagnostics,
            CompilationFactKey::ConstantTemplateKeys => Self::ConstantTemplateKeys,
            CompilationFactKey::CallableBodyKeys => Self::CallableBodyKeys,
            CompilationFactKey::PredicateDefinitionKeys => Self::PredicateDefinitionKeys,
            CompilationFactKey::ConstantInstance(_) => Self::ConstantInstance,
            CompilationFactKey::ConstantCall(_) => Self::ConstantCall,
            CompilationFactKey::ConstantCallCycle(_) => Self::ConstantCallCycle,
            CompilationFactKey::CheckedControlFlow(_) => Self::CheckedControlFlow,
            CompilationFactKey::CheckedExpressionTypes(_) => Self::CheckedExpressionTypes,
            CompilationFactKey::CheckedLiteralValues(_) => Self::CheckedLiteralValues,
            CompilationFactKey::CheckedPatterns(_) => Self::CheckedPatterns,
            CompilationFactKey::CheckedSemanticSelections(_) => Self::CheckedSemanticSelections,
            CompilationFactKey::StoragePlan(_) => Self::StoragePlan,
            CompilationFactKey::Liveness(_) => Self::Liveness,
            CompilationFactKey::RefinementFacts(_) => Self::RefinementFacts,
            CompilationFactKey::StorageFlowFacts(_) => Self::StorageFlowFacts,
            CompilationFactKey::DependencyContracts(_) => Self::DependencyContracts,
            CompilationFactKey::MemoryOperations(_) => Self::MemoryOperations,
            CompilationFactKey::AsyncFacts(_) => Self::AsyncFacts,
            CompilationFactKey::BodyBehaviorContributions(_) => Self::BodyBehaviorContributions,
            CompilationFactKey::CheckedBodyBehavior(_) => Self::CheckedBodyBehavior,
            CompilationFactKey::LoweredUnit(_) => Self::LoweredUnit,
            CompilationFactKey::CodegenArtifact(_) => Self::CodegenArtifact,
            CompilationFactKey::NativeProduct(_) => Self::NativeProduct,
            CompilationFactKey::DeclaredValueTypeTemplates(_) => Self::DeclaredValueTypeTemplates,
            CompilationFactKey::ExpressionSemantics(_) => Self::ExpressionSemantics,
            CompilationFactKey::ProvisionalExpressionSemantics(_) => Self::ProvisionalExpressionSemantics,
            CompilationFactKey::SymbolicConstantTerm(_) => Self::SymbolicConstantTerm,
            CompilationFactKey::DeclarationChunk(_) => Self::DeclarationChunk,
            CompilationFactKey::DeclarationTable => Self::DeclarationTable,
            CompilationFactKey::ProductSourceGraph => Self::ProductSourceGraph,
            CompilationFactKey::ProductSemantics => Self::ProductSemantics,
            CompilationFactKey::TestDiscovery(_) => Self::TestDiscovery,
            CompilationFactKey::DiscoverySymbolGraph => Self::DiscoverySymbolGraph,
            CompilationFactKey::DependencyInterface(_) => Self::DependencyInterface,
            CompilationFactKey::DependencyImplementation(_) => Self::DependencyImplementation,
            CompilationFactKey::ImportedDiagnostics => Self::ImportedDiagnostics,
            CompilationFactKey::ImplementationHeaderIndex => Self::ImplementationHeaderIndex,
            CompilationFactKey::ImplementationCandidateSet(_) => Self::ImplementationCandidateSet,
            CompilationFactKey::TraitImplementationConformance(_) => Self::TraitImplementationConformance,
            CompilationFactKey::GenericConstraintSatisfaction(_) => Self::GenericConstraintSatisfaction,
            CompilationFactKey::ImplementationSelection(_) => Self::ImplementationSelection,
            CompilationFactKey::IterationSource(_) => Self::IterationSource,
            CompilationFactKey::OperationSelection(_) => Self::OperationSelection,
            CompilationFactKey::ImportedSemanticGraph(_) => Self::ImportedSemanticGraph,
            CompilationFactKey::ImportedSemanticFact(_) => Self::ImportedSemanticFact,
            CompilationFactKey::ImportedConstantCallableBody(_) => Self::ImportedConstantCallableBody,
            CompilationFactKey::ImportedExecutableTemplate(_) => Self::ImportedExecutableTemplate,
            CompilationFactKey::ImplementationParticipation(_) => Self::ImplementationParticipation,
            CompilationFactKey::ImplementationCoherence => Self::ImplementationCoherence,
            CompilationFactKey::CallableOverloadValidation => Self::CallableOverloadValidation,
            CompilationFactKey::ForeignCallableContract(_) => Self::ForeignCallableContract,
            CompilationFactKey::ForeignCallableValidation => Self::ForeignCallableValidation,
            CompilationFactKey::TypeAssociatedSurface(_) => Self::TypeAssociatedSurface,
            CompilationFactKey::DeclaredTypeRepresentation(_) => Self::DeclaredTypeRepresentation,
            CompilationFactKey::TypeAssociatedImplementationIndex => Self::TypeAssociatedImplementationIndex,
            CompilationFactKey::ImportedSymbolSkeleton => Self::ImportedSymbolSkeleton,
            CompilationFactKey::PackageInterfaceExportBundle => Self::PackageInterfaceExportBundle,
            CompilationFactKey::SemanticValueStore => Self::SemanticValueStore,
            CompilationFactKey::SemanticDiagnostics => Self::SemanticDiagnostics,
            CompilationFactKey::SourceUnitSyntax(_) => Self::SourceUnitSyntax,
            CompilationFactKey::SourceReferenceIndex(_) => Self::SourceReferenceIndex,
            CompilationFactKey::SymbolGraph => Self::SymbolGraph,
            CompilationFactKey::Symbol(_) => Self::Symbol,
            CompilationFactKey::SyntaxTree => Self::SyntaxTree,
        }
    }
}

pub(crate) const fn records_trace(mode: CompilationProfileMode) -> bool {
    matches!(mode, CompilationProfileMode::Trace)
}

pub(crate) const fn result_outcome<T, E>(result: &Result<T, E>) -> CompilationProfileOutcome {
    if result.is_ok() {
        CompilationProfileOutcome::Completed
    } else {
        CompilationProfileOutcome::Failed
    }
}
