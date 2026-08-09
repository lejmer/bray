use bray_profile::{
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
        match self {
            Self::CompilationLoad => 1,
            Self::QueryEvaluation => 2,
            Self::SchedulerQueue => 3,
            Self::DependencyWait => 4,
            Self::Lowering => 5,
            Self::CodeGeneration => 6,
            Self::Linking => 7,
            Self::Emission => 8,
            Self::InterfaceExport => 9,
        }
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
        match self {
            Self::SourceUnits => 2_000,
            Self::SourceBytes => 2_001,
            Self::SyntaxTokens => 2_002,
            Self::Declarations => 2_003,
            Self::BoundUnits => 2_004,
            Self::CheckedBodies => 2_005,
            Self::MirUnits => 2_006,
            Self::MirBlocks => 2_007,
            Self::MirOperations => 2_008,
            Self::ConcreteInstances => 2_009,
            Self::CodegenUnits => 2_010,
            Self::InterfaceSections => 2_011,
            Self::InterfaceBytes => 2_012,
            Self::LinkInputs => 2_013,
            Self::EmittedArtifacts => 2_014,
            Self::EmittedBytes => 2_015,
        }
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
    ($( $variant:ident = $id:literal => $name:literal, )+) => {
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
                match self {
                    $( Self::$variant => $id, )+
                }
            }
        }
    };
}

define_profile_query_kinds! {
    SelectedTarget = 1000 => "selected_target",
    TargetValidity = 1001 => "target_validity",
    ModuleContributionGate = 1002 => "module_contribution_gate",
    CallableTypeDirectives = 1003 => "callable_type_directives",
    CompilerKnownSymbols = 1004 => "compiler_known_symbols",
    BoundUnitIdentities = 1005 => "bound_unit_identities",
    BoundUnit = 1006 => "bound_unit",
    CheckDiagnostics = 1007 => "check_diagnostics",
    ConstantTemplateKeys = 1008 => "constant_template_keys",
    CallableBodyKeys = 1009 => "callable_body_keys",
    PredicateDefinitionKeys = 1010 => "predicate_definition_keys",
    ConstantInstance = 1011 => "constant_instance",
    ConstantCall = 1012 => "constant_call",
    ConstantCallCycle = 1013 => "constant_call_cycle",
    CheckedControlFlow = 1014 => "checked_control_flow",
    CheckedExpressionTypes = 1015 => "checked_expression_types",
    CheckedLiteralValues = 1016 => "checked_literal_values",
    CheckedPatterns = 1017 => "checked_patterns",
    CheckedSemanticSelections = 1018 => "checked_semantic_selections",
    StoragePlan = 1019 => "storage_plan",
    Liveness = 1020 => "liveness",
    RefinementFacts = 1021 => "refinement_facts",
    StorageFlowFacts = 1022 => "storage_flow_facts",
    DependencyContracts = 1023 => "dependency_contracts",
    MemoryOperations = 1024 => "memory_operations",
    AsyncFacts = 1025 => "async_facts",
    BodyBehaviorContributions = 1026 => "body_behavior_contributions",
    CheckedBodyBehavior = 1027 => "checked_body_behavior",
    LoweredUnit = 1028 => "lowered_unit",
    CodegenArtifact = 1029 => "codegen_artifact",
    NativeProduct = 1030 => "native_product",
    DeclaredValueTypeTemplates = 1031 => "declared_value_type_templates",
    ExpressionSemantics = 1032 => "expression_semantics",
    ProvisionalExpressionSemantics = 1033 => "provisional_expression_semantics",
    SymbolicConstantTerm = 1034 => "symbolic_constant_term",
    DeclarationChunk = 1035 => "declaration_chunk",
    DeclarationTable = 1036 => "declaration_table",
    ProductSourceGraph = 1037 => "product_source_graph",
    ProductSemantics = 1038 => "product_semantics",
    TestDiscovery = 1039 => "test_discovery",
    DiscoverySymbolGraph = 1040 => "discovery_symbol_graph",
    DependencyInterface = 1041 => "dependency_interface",
    DependencyImplementation = 1042 => "dependency_implementation",
    ImportedDiagnostics = 1043 => "imported_diagnostics",
    ImplementationHeaderIndex = 1044 => "implementation_header_index",
    ImplementationCandidateSet = 1045 => "implementation_candidate_set",
    TraitImplementationConformance = 1046 => "trait_implementation_conformance",
    GenericConstraintSatisfaction = 1047 => "generic_constraint_satisfaction",
    ImplementationSelection = 1048 => "implementation_selection",
    IterationSource = 1049 => "iteration_source",
    OperationSelection = 1050 => "operation_selection",
    ImportedSemanticGraph = 1051 => "imported_semantic_graph",
    ImportedSemanticFact = 1052 => "imported_semantic_fact",
    ImportedConstantCallableBody = 1053 => "imported_constant_callable_body",
    ImportedExecutableTemplate = 1054 => "imported_executable_template",
    ImplementationParticipation = 1055 => "implementation_participation",
    ImplementationCoherence = 1056 => "implementation_coherence",
    CallableOverloadValidation = 1057 => "callable_overload_validation",
    ForeignCallableContract = 1058 => "foreign_callable_contract",
    ForeignCallableValidation = 1059 => "foreign_callable_validation",
    TypeAssociatedSurface = 1060 => "type_associated_surface",
    DeclaredTypeRepresentation = 1061 => "declared_type_representation",
    TypeAssociatedImplementationIndex = 1062 => "type_associated_implementation_index",
    ImportedSymbolSkeleton = 1063 => "imported_symbol_skeleton",
    PackageInterfaceExportBundle = 1064 => "package_interface_export_bundle",
    SemanticValueStore = 1065 => "semantic_value_store",
    SemanticDiagnostics = 1066 => "semantic_diagnostics",
    SourceUnitSyntax = 1067 => "source_unit_syntax",
    SourceReferenceIndex = 1068 => "source_reference_index",
    SymbolGraph = 1069 => "symbol_graph",
    Symbol = 1070 => "symbol",
    SyntaxTree = 1071 => "syntax_tree",
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

#[cfg(test)]
mod tests {
    use super::{ProfileMetricKind, ProfileOperation, ProfileQueryKind};

    #[test]
    fn descriptor_ids_are_schema_locked() {
        assert_eq!(
            ProfileOperation::all().map(ProfileOperation::id),
            [1, 2, 3, 4, 5, 6, 7, 8, 9]
        );

        assert_eq!(
            ProfileMetricKind::all().map(ProfileMetricKind::id),
            [
                2_000, 2_001, 2_002, 2_003, 2_004, 2_005, 2_006, 2_007, 2_008, 2_009,
                2_010, 2_011, 2_012, 2_013, 2_014, 2_015,
            ]
        );

        assert_eq!(
            ProfileQueryKind::all().map(ProfileQueryKind::id),
            std::array::from_fn(|index| {
                1_000 + u16::try_from(index).unwrap_or(u16::MAX)
            })
        );
    }
}
