use bray_diagnostics::{
    DiagnosticProductDataKind, DiagnosticProductQueryContext, DiagnosticProductQueryContextKind,
    DiagnosticProductQueryFailure, DiagnosticProductValueKind,
};

use crate::compilation::{
    ProductDataKind, ProductQueryContext, ProductQueryError, ProductQueryFailure, ProductValueKind,
};

pub(in crate::compilation) fn diagnostic_product_query_failure(
    error: &ProductQueryError,
) -> DiagnosticProductQueryFailure {
    match error.cause() {
        ProductQueryFailure::Missing { context, data } => DiagnosticProductQueryFailure::Missing {
            context: diagnostic_product_query_context(context),
            data: diagnostic_product_data_kind(*data),
        },
        ProductQueryFailure::Conflict { context, data } => {
            DiagnosticProductQueryFailure::Conflict {
                context: diagnostic_product_query_context(context),
                data: diagnostic_product_data_kind(*data),
            }
        }
        ProductQueryFailure::UnexpectedKind {
            context,
            expected,
            actual,
        } => DiagnosticProductQueryFailure::UnexpectedKind {
            context: diagnostic_product_query_context(context),
            expected: diagnostic_product_value_kind(*expected),
            actual: diagnostic_product_value_kind(*actual),
        },
        ProductQueryFailure::CountMismatch {
            context,
            data,
            expected,
            actual,
        } => DiagnosticProductQueryFailure::CountMismatch {
            context: diagnostic_product_query_context(context),
            data: diagnostic_product_data_kind(*data),
            expected: *expected,
            actual: *actual,
        },
        ProductQueryFailure::UnsupportedConstantValue { value, kind } => {
            DiagnosticProductQueryFailure::UnsupportedConstantValue {
                value: format!("{value:?}"),
                kind: format!("{kind:?}"),
            }
        }
        ProductQueryFailure::GenericSubstitution {
            substitution,
            cause,
        } => diagnostic_generic_substitution(*substitution, cause),
        ProductQueryFailure::TraitApplicationMismatch {
            witness,
            expected,
            actual,
        } => diagnostic_trait_application_mismatch(*witness, *expected, *actual),
        ProductQueryFailure::ConflictingImplementationWitness {
            identity,
            existing,
            actual,
        } => diagnostic_conflicting_implementation_witness(identity, *existing, *actual),
        ProductQueryFailure::UnexpectedSemanticType {
            ty,
            expected,
            actual,
        } => diagnostic_unexpected_semantic_type(*ty, *expected, actual),
        ProductQueryFailure::SynchronizationPoisoned { component } => {
            diagnostic_synchronization_poisoned(*component)
        }
        ProductQueryFailure::StaticDependencyOverflow { static_instance } => {
            DiagnosticProductQueryFailure::StaticDependencyOverflow {
                static_instance: format!("{static_instance:?}"),
            }
        }
        ProductQueryFailure::StaticDependencyUnderflow { static_instance } => {
            DiagnosticProductQueryFailure::StaticDependencyUnderflow {
                static_instance: format!("{static_instance:?}"),
            }
        }
        ProductQueryFailure::StaticLifecycleCycle { instances } => {
            diagnostic_static_lifecycle_cycle(instances)
        }
        ProductQueryFailure::ConflictingConcreteInstance { key } => {
            DiagnosticProductQueryFailure::ConflictingConcreteInstance {
                key: format!("{key:?}"),
            }
        }
        ProductQueryFailure::ConflictingStaticRelocation { value } => {
            DiagnosticProductQueryFailure::ConflictingStaticRelocation {
                value: format!("{value:?}"),
            }
        }
        ProductQueryFailure::CallableDefinitionMismatch {
            instance,
            expected,
            actual,
        } => diagnostic_callable_definition_mismatch(instance, *expected, *actual),
        ProductQueryFailure::TraitDefinitionMismatch {
            context,
            expected,
            actual,
        } => diagnostic_trait_definition_mismatch(context, *expected, *actual),
        ProductQueryFailure::InvalidCodegenSourceFile { source, path } => {
            diagnostic_invalid_codegen_source_file(*source, path)
        }
        ProductQueryFailure::SourceIndex { source, cause } => {
            DiagnosticProductQueryFailure::SourceIndex {
                source: source.raw(),
                cause: format!("{cause:?}"),
            }
        }
        ProductQueryFailure::ExternalSymbolIdentity { symbol, cause } => {
            DiagnosticProductQueryFailure::ExternalSymbolIdentity {
                symbol: diagnostic_symbol_identity(*symbol),
                cause: format!("{cause:?}"),
            }
        }
        ProductQueryFailure::InvalidCompilerKnownDeclarationKey { key } => {
            DiagnosticProductQueryFailure::InvalidCompilerKnownDeclarationKey { key: key.clone() }
        }
        ProductQueryFailure::InvalidRecognizedStandardLibraryDeclarationKey { key } => {
            DiagnosticProductQueryFailure::InvalidRecognizedStandardLibraryDeclarationKey {
                key: key.clone(),
            }
        }
        ProductQueryFailure::InvalidPackageIdentity { identity } => {
            DiagnosticProductQueryFailure::InvalidPackageIdentity {
                identity: identity.clone(),
            }
        }
        ProductQueryFailure::UnexpectedEntryResult { actual } => {
            DiagnosticProductQueryFailure::UnexpectedEntryResult {
                actual: format!("{actual:?}"),
            }
        }
        ProductQueryFailure::CompilerKnownRepresentationMismatch {
            ty,
            expected,
            actual,
        } => diagnostic_compiler_known_representation_mismatch(*ty, *expected, *actual),
        ProductQueryFailure::NativeBoundaryKindMismatch { reference, actual } => {
            DiagnosticProductQueryFailure::NativeBoundaryKindMismatch {
                reference: format!("{reference:?}"),
                actual: format!("{actual:?}"),
            }
        }
        ProductQueryFailure::UnexpectedSymbolKind {
            symbol,
            expected,
            actual,
        } => diagnostic_unexpected_symbol_kind(*symbol, *expected, *actual),
        ProductQueryFailure::ImplementationSymbolKeyExpected { key, actual } => {
            DiagnosticProductQueryFailure::ImplementationSymbolKeyExpected {
                key: format!("{key:?}"),
                actual: actual.as_str().to_owned(),
            }
        }
        ProductQueryFailure::UnsupportedRuntimeRole { role } => {
            DiagnosticProductQueryFailure::UnsupportedRuntimeRole {
                role: role.as_str().to_owned(),
            }
        }
        ProductQueryFailure::InvalidHelperOperation {
            context,
            helper,
            operation,
        } => diagnostic_invalid_helper_operation(context, helper, operation),
        ProductQueryFailure::InvalidHelperCallTarget {
            context,
            helper,
            target,
        } => diagnostic_invalid_helper_call_target(context, helper, target),
        ProductQueryFailure::LifecycleRoleMismatch {
            instance,
            expected,
            actual,
        } => diagnostic_lifecycle_role_mismatch(instance, *expected, *actual),
        ProductQueryFailure::BuiltInProofMismatch {
            requirement,
            actual,
        } => diagnostic_built_in_proof_mismatch(requirement, *actual),
        ProductQueryFailure::ImplementationSelectionMismatch {
            requirement,
            actual,
        } => DiagnosticProductQueryFailure::ImplementationSelectionMismatch {
            requirement: format!("{requirement:?}"),
            actual: format!("{actual:?}"),
        },
        ProductQueryFailure::UnsupportedRuntimeDefaultSubject { provider, subject } => {
            diagnostic_unsupported_runtime_default_subject(provider, subject)
        }
        ProductQueryFailure::InvalidCallableDefinitionSymbol { symbol, actual } => {
            DiagnosticProductQueryFailure::InvalidCallableDefinitionSymbol {
                symbol: diagnostic_symbol_identity(*symbol),
                actual: actual.as_str().to_owned(),
            }
        }
        ProductQueryFailure::UnsupportedLifecycleRole { role } => {
            DiagnosticProductQueryFailure::UnsupportedLifecycleRole {
                role: format!("{role:?}"),
            }
        }
        ProductQueryFailure::SourceSnapshotMismatch {
            source,
            expected,
            actual,
        } => DiagnosticProductQueryFailure::SourceSnapshotMismatch {
            source: source.raw(),
            expected: format!("{expected:?}"),
            actual: actual.map(|version| format!("{version:?}")),
        },
        ProductQueryFailure::TestProductMismatch {
            requested,
            compilation_package,
            compilation_kind,
        } => diagnostic_test_product_mismatch(requested, compilation_package, *compilation_kind),
        ProductQueryFailure::TestCatalog {
            product,
            identity,
            cause,
        } => diagnostic_test_catalog(product, identity, *cause),
        ProductQueryFailure::InvalidTestErrorTypeIdentity { digest } => {
            diagnostic_invalid_test_error_type_identity(*digest)
        }
        ProductQueryFailure::InvalidModulePath {
            declaration,
            segments,
        } => DiagnosticProductQueryFailure::InvalidModulePath {
            declaration: declaration.raw(),
            segments: segments.clone(),
        },
        ProductQueryFailure::UnsupportedEntryResultType { ty, actual } => {
            DiagnosticProductQueryFailure::UnsupportedEntryResultType {
                ty: format!("{ty:?}"),
                actual: actual.map(|role| role.as_str().to_owned()),
            }
        }
        ProductQueryFailure::InvalidCodegenRequest { unit, cause } => {
            DiagnosticProductQueryFailure::InvalidCodegenRequest {
                unit: super::common::codegen_unit_identity(unit),
                cause: format!("{cause:?}"),
            }
        }
        ProductQueryFailure::CodegenBackendSelection { unit, cause } => {
            DiagnosticProductQueryFailure::CodegenBackendSelection {
                unit: super::common::codegen_unit_identity(unit),
                cause: format!("{cause:?}"),
            }
        }
    }
}

fn diagnostic_built_in_proof_mismatch(
    requirement: &bray_symbols::ImplementationRequirementKey,
    actual: Option<bray_symbols::ProofOutcome>,
) -> DiagnosticProductQueryFailure {
    DiagnosticProductQueryFailure::BuiltInProofMismatch {
        requirement: format!("{requirement:?}"),
        actual: actual.map(|proof| format!("{proof:?}")),
    }
}

fn diagnostic_test_catalog(
    product: &bray_symbols::ProductIdentity,
    identity: &bray_test_protocol::TestIdentity,
    cause: crate::compilation::ProductTestCatalogFailureKind,
) -> DiagnosticProductQueryFailure {
    DiagnosticProductQueryFailure::TestCatalog {
        product: product.to_string(),
        identity: format!("{identity:?}"),
        cause: format!("{cause:?}"),
    }
}

fn diagnostic_unexpected_semantic_type(
    ty: bray_symbols::TypeId,
    expected: ProductValueKind,
    actual: &bray_symbols::TypeData,
) -> DiagnosticProductQueryFailure {
    DiagnosticProductQueryFailure::UnexpectedSemanticType {
        ty: format!("{ty:?}"),
        expected: diagnostic_product_value_kind(expected),
        actual: format!("{actual:?}"),
    }
}

fn diagnostic_invalid_codegen_source_file(
    source: Option<bray_source::SourceId>,
    path: &str,
) -> DiagnosticProductQueryFailure {
    DiagnosticProductQueryFailure::InvalidCodegenSourceFile {
        source: source.map(bray_source::SourceId::raw),
        path: path.to_owned(),
    }
}

fn diagnostic_unexpected_symbol_kind(
    symbol: bray_symbols::AnySymbolId,
    expected: bray_symbols::SymbolKind,
    actual: bray_symbols::SymbolKind,
) -> DiagnosticProductQueryFailure {
    DiagnosticProductQueryFailure::UnexpectedSymbolKind {
        symbol: diagnostic_symbol_identity(symbol),
        expected: expected.as_str().to_owned(),
        actual: actual.as_str().to_owned(),
    }
}

fn diagnostic_invalid_helper_operation(
    context: &ProductQueryContext,
    helper: &bray_ir::MirHelperReference,
    operation: &bray_ir::MirOperationKind,
) -> DiagnosticProductQueryFailure {
    DiagnosticProductQueryFailure::InvalidHelperOperation {
        context: diagnostic_product_query_context(context),
        helper: format!("{helper:?}"),
        operation: format!("{operation:?}"),
    }
}

fn diagnostic_invalid_helper_call_target(
    context: &ProductQueryContext,
    helper: &bray_ir::MirHelperReference,
    target: &bray_ir::MirCallTarget,
) -> DiagnosticProductQueryFailure {
    DiagnosticProductQueryFailure::InvalidHelperCallTarget {
        context: diagnostic_product_query_context(context),
        helper: format!("{helper:?}"),
        target: format!("{target:?}"),
    }
}

fn diagnostic_lifecycle_role_mismatch(
    instance: &bray_codegen::CodegenInstanceKey,
    expected: Option<bray_ir::MirGeneratedLifecycleRole>,
    actual: Option<bray_ir::MirGeneratedLifecycleRole>,
) -> DiagnosticProductQueryFailure {
    DiagnosticProductQueryFailure::LifecycleRoleMismatch {
        instance: format!("{instance:?}"),
        expected: expected.map(|role| format!("{role:?}")),
        actual: actual.map(|role| format!("{role:?}")),
    }
}

fn diagnostic_generic_substitution(
    substitution: Option<bray_symbols::GenericSubstitutionId>,
    cause: &bray_symbols::GenericSubstitutionShapeError,
) -> DiagnosticProductQueryFailure {
    DiagnosticProductQueryFailure::GenericSubstitution {
        substitution: substitution.map(|identity| format!("{identity:?}")),
        cause: format!("{cause:?}"),
    }
}

fn diagnostic_trait_application_mismatch(
    witness: bray_symbols::ImplementationInstanceId,
    expected: bray_symbols::TraitApplicationId,
    actual: bray_symbols::TraitApplicationId,
) -> DiagnosticProductQueryFailure {
    DiagnosticProductQueryFailure::TraitApplicationMismatch {
        witness: format!("{witness:?}"),
        expected: format!("{expected:?}"),
        actual: format!("{actual:?}"),
    }
}

fn diagnostic_conflicting_implementation_witness(
    identity: &bray_codegen::CodegenImplementationWitness,
    existing: bray_symbols::ImplementationInstanceId,
    actual: bray_symbols::ImplementationInstanceId,
) -> DiagnosticProductQueryFailure {
    DiagnosticProductQueryFailure::ConflictingImplementationWitness {
        identity: format!("{identity:?}"),
        existing: format!("{existing:?}"),
        actual: format!("{actual:?}"),
    }
}

fn diagnostic_callable_definition_mismatch(
    instance: &bray_codegen::CodegenInstanceKey,
    expected: bray_symbols::CallableDefinitionId,
    actual: bray_symbols::CallableDefinitionId,
) -> DiagnosticProductQueryFailure {
    DiagnosticProductQueryFailure::CallableDefinitionMismatch {
        instance: format!("{instance:?}"),
        expected: expected.symbol().symbol_id().raw(),
        actual: actual.symbol().symbol_id().raw(),
    }
}

fn diagnostic_trait_definition_mismatch(
    context: &ProductQueryContext,
    expected: bray_symbols::TraitSymbolId,
    actual: bray_symbols::TraitSymbolId,
) -> DiagnosticProductQueryFailure {
    DiagnosticProductQueryFailure::TraitDefinitionMismatch {
        context: diagnostic_product_query_context(context),
        expected: expected.symbol_id().raw(),
        actual: actual.symbol_id().raw(),
    }
}

fn diagnostic_compiler_known_representation_mismatch(
    ty: bray_symbols::TypeId,
    expected: bray_compiler_known::RepresentationRole,
    actual: Option<bray_compiler_known::RepresentationRole>,
) -> DiagnosticProductQueryFailure {
    DiagnosticProductQueryFailure::CompilerKnownRepresentationMismatch {
        ty: format!("{ty:?}"),
        expected: expected.as_str().to_owned(),
        actual: actual.map(|role| role.as_str().to_owned()),
    }
}

fn diagnostic_synchronization_poisoned(
    component: crate::compilation::ProductSynchronizationComponent,
) -> DiagnosticProductQueryFailure {
    let component = match component {
        crate::compilation::ProductSynchronizationComponent::LifecycleNeeds => "lifecycle_needs",
    };

    DiagnosticProductQueryFailure::SynchronizationPoisoned {
        component: component.to_owned(),
    }
}

fn diagnostic_static_lifecycle_cycle(
    instances: &[bray_codegen::CodegenStaticInstanceKey],
) -> DiagnosticProductQueryFailure {
    DiagnosticProductQueryFailure::StaticLifecycleCycle {
        instances: instances
            .iter()
            .map(|instance| format!("{instance:?}"))
            .collect(),
    }
}

fn diagnostic_unsupported_runtime_default_subject(
    provider: &bray_symbols::AnySymbolId,
    subject: &bray_symbols::AnySymbolId,
) -> DiagnosticProductQueryFailure {
    DiagnosticProductQueryFailure::UnsupportedRuntimeDefaultSubject {
        provider: diagnostic_symbol_identity(*provider),
        subject: diagnostic_symbol_identity(*subject),
    }
}

fn diagnostic_test_product_mismatch(
    requested: &bray_symbols::ProductIdentity,
    compilation_package: &bray_symbols::PackageIdentity,
    compilation_kind: bray_symbols::ProductKind,
) -> DiagnosticProductQueryFailure {
    let compilation_kind = match compilation_kind {
        bray_symbols::ProductKind::Executable => "executable",
        bray_symbols::ProductKind::Library => "library",
        bray_symbols::ProductKind::Test => "test",
    };

    DiagnosticProductQueryFailure::TestProductMismatch {
        requested: requested.to_string(),
        compilation_package: compilation_package.as_str().to_owned(),
        compilation_kind: compilation_kind.to_owned(),
    }
}

fn diagnostic_invalid_test_error_type_identity(digest: [u8; 32]) -> DiagnosticProductQueryFailure {
    DiagnosticProductQueryFailure::InvalidTestErrorTypeIdentity {
        digest: bray_diagnostics::DiagnosticArtifactDigest::new(
            bray_diagnostics::DiagnosticArtifactDigestAlgorithm::Blake3,
            digest,
        ),
    }
}

fn diagnostic_symbol_identity(symbol: bray_symbols::AnySymbolId) -> String {
    format!("{}:{}", symbol.kind().as_str(), symbol.symbol_id().raw())
}

fn diagnostic_product_query_context(
    context: &ProductQueryContext,
) -> DiagnosticProductQueryContext {
    let kind = match context {
        ProductQueryContext::Product(_) => DiagnosticProductQueryContextKind::Product,
        ProductQueryContext::Function(_) => DiagnosticProductQueryContextKind::Function,
        ProductQueryContext::Symbol(_) => DiagnosticProductQueryContextKind::Symbol,
        ProductQueryContext::Declaration(_) => DiagnosticProductQueryContextKind::Declaration,
        ProductQueryContext::Container(_) => DiagnosticProductQueryContextKind::Container,
        ProductQueryContext::ModulePath { .. } => DiagnosticProductQueryContextKind::ModulePath,
        ProductQueryContext::SymbolKey(_) => DiagnosticProductQueryContextKind::SymbolKey,
        ProductQueryContext::Source(_) => DiagnosticProductQueryContextKind::Source,
        ProductQueryContext::Target(_) => DiagnosticProductQueryContextKind::Target,
        ProductQueryContext::Instance(_) => DiagnosticProductQueryContextKind::Instance,
        ProductQueryContext::CallSite(_) => DiagnosticProductQueryContextKind::CallSite,
        ProductQueryContext::CodegenUnit(_) => DiagnosticProductQueryContextKind::CodegenUnit,
        ProductQueryContext::CodegenStatic(_) => DiagnosticProductQueryContextKind::CodegenStatic,
        ProductQueryContext::CallableData(_) => DiagnosticProductQueryContextKind::CallableData,
        ProductQueryContext::Implementation(_) => DiagnosticProductQueryContextKind::Implementation,
        ProductQueryContext::StaticReference(_) => {
            DiagnosticProductQueryContextKind::StaticReference
        }
        ProductQueryContext::Substitution(_) => DiagnosticProductQueryContextKind::Substitution,
        ProductQueryContext::GenericOwner(_) => DiagnosticProductQueryContextKind::GenericOwner,
        ProductQueryContext::Type(_) => DiagnosticProductQueryContextKind::Type,
        ProductQueryContext::ImplementationRequirement(_) => {
            DiagnosticProductQueryContextKind::ImplementationRequirement
        }
        ProductQueryContext::MirUnit(_) => DiagnosticProductQueryContextKind::MirUnit,
        ProductQueryContext::MirHelper(_) => DiagnosticProductQueryContextKind::MirHelper,
        ProductQueryContext::UnaryRepresentation { .. } => {
            DiagnosticProductQueryContextKind::UnaryRepresentation
        }
        ProductQueryContext::Operation { .. } => DiagnosticProductQueryContextKind::Operation,
        ProductQueryContext::MirOperation { .. } => DiagnosticProductQueryContextKind::MirOperation,
        ProductQueryContext::CallableDefinition(_) => {
            DiagnosticProductQueryContextKind::CallableDefinition
        }
        ProductQueryContext::SourceLocation { .. } => {
            DiagnosticProductQueryContextKind::SourceLocation
        }
        ProductQueryContext::CompilerKnownRepresentation(_) => {
            DiagnosticProductQueryContextKind::CompilerKnownRepresentation
        }
        ProductQueryContext::CompilerKnownDeclaration(_) => {
            DiagnosticProductQueryContextKind::CompilerKnownDeclaration
        }
    };

    DiagnosticProductQueryContext::new(kind, format!("{context:?}"))
}

const fn diagnostic_product_data_kind(kind: ProductDataKind) -> DiagnosticProductDataKind {
    match kind {
        ProductDataKind::Symbol => DiagnosticProductDataKind::Symbol,
        ProductDataKind::TestResult => DiagnosticProductDataKind::TestResult,
        ProductDataKind::ContainingModule => DiagnosticProductDataKind::ContainingModule,
        ProductDataKind::ContainingSymbol => DiagnosticProductDataKind::ContainingSymbol,
        ProductDataKind::MemberName => DiagnosticProductDataKind::MemberName,
        ProductDataKind::SourceAnchor => DiagnosticProductDataKind::SourceAnchor,
        ProductDataKind::SourceSnapshot => DiagnosticProductDataKind::SourceSnapshot,
        ProductDataKind::SymbolKey => DiagnosticProductDataKind::SymbolKey,
        ProductDataKind::ConcreteInstance => DiagnosticProductDataKind::ConcreteInstance,
        ProductDataKind::CallableInstance => DiagnosticProductDataKind::CallableInstance,
        ProductDataKind::AnonymousCallableInstance => {
            DiagnosticProductDataKind::AnonymousCallableInstance
        }
        ProductDataKind::BoundHelperInstance => DiagnosticProductDataKind::BoundHelperInstance,
        ProductDataKind::GeneratedLifecycleInstance => {
            DiagnosticProductDataKind::GeneratedLifecycleInstance
        }
        ProductDataKind::ContextualSelfWitness => DiagnosticProductDataKind::ContextualSelfWitness,
        ProductDataKind::TraitDispatch => DiagnosticProductDataKind::TraitDispatch,
        ProductDataKind::GenericSubstitution => DiagnosticProductDataKind::GenericSubstitution,
        ProductDataKind::ImplementationWitness => DiagnosticProductDataKind::ImplementationWitness,
        ProductDataKind::CallableFulfillment => DiagnosticProductDataKind::CallableFulfillment,
        ProductDataKind::GenericConstraint => DiagnosticProductDataKind::GenericConstraint,
        ProductDataKind::GenericOwner => DiagnosticProductDataKind::GenericOwner,
        ProductDataKind::Intrinsic => DiagnosticProductDataKind::Intrinsic,
        ProductDataKind::ConversionPlan => DiagnosticProductDataKind::ConversionPlan,
        ProductDataKind::CompilerKnownRepresentation => {
            DiagnosticProductDataKind::CompilerKnownRepresentation
        }
        ProductDataKind::LifecycleRole => DiagnosticProductDataKind::LifecycleRole,
        ProductDataKind::LifecycleMember => DiagnosticProductDataKind::LifecycleMember,
        ProductDataKind::LifecycleType => DiagnosticProductDataKind::LifecycleType,
        ProductDataKind::RealizedStatic => DiagnosticProductDataKind::RealizedStatic,
        ProductDataKind::StaticDependencyCounter => {
            DiagnosticProductDataKind::StaticDependencyCounter
        }
        ProductDataKind::StaticDeclaredType => DiagnosticProductDataKind::StaticDeclaredType,
        ProductDataKind::StaticInitializer => DiagnosticProductDataKind::StaticInitializer,
        ProductDataKind::ImplementationHeader => DiagnosticProductDataKind::ImplementationHeader,
        ProductDataKind::TestDiscovery => DiagnosticProductDataKind::TestDiscovery,
        ProductDataKind::DeclarationChunk => DiagnosticProductDataKind::DeclarationChunk,
        ProductDataKind::DeclarationContainer => DiagnosticProductDataKind::DeclarationContainer,
        ProductDataKind::ModulePath => DiagnosticProductDataKind::ModulePath,
        ProductDataKind::PackageRoot => DiagnosticProductDataKind::PackageRoot,
        ProductDataKind::Module => DiagnosticProductDataKind::Module,
        ProductDataKind::ReachabilityRealization => {
            DiagnosticProductDataKind::ReachabilityRealization
        }
        ProductDataKind::ReachabilityEvaluation => {
            DiagnosticProductDataKind::ReachabilityEvaluation
        }
        ProductDataKind::PartitionInstance => DiagnosticProductDataKind::PartitionInstance,
        ProductDataKind::CodegenUnitMapping => DiagnosticProductDataKind::CodegenUnitMapping,
        ProductDataKind::ProductHostOwnerUnit => DiagnosticProductDataKind::ProductHostOwnerUnit,
        ProductDataKind::ProductHostStaticMapping => {
            DiagnosticProductDataKind::ProductHostStaticMapping
        }
        ProductDataKind::ResultRepresentation => DiagnosticProductDataKind::ResultRepresentation,
        ProductDataKind::CallableSignature => DiagnosticProductDataKind::CallableSignature,
        ProductDataKind::RuntimeDefaultSubject => DiagnosticProductDataKind::RuntimeDefaultSubject,
        ProductDataKind::RuntimeDefaultUnit => DiagnosticProductDataKind::RuntimeDefaultUnit,
        ProductDataKind::OperationResultType => DiagnosticProductDataKind::OperationResultType,
        ProductDataKind::SourceLineIndex => DiagnosticProductDataKind::SourceLineIndex,
        ProductDataKind::SourceLocation => DiagnosticProductDataKind::SourceLocation,
        ProductDataKind::NativeStaticContract => DiagnosticProductDataKind::NativeStaticContract,
        ProductDataKind::CallableParameters => DiagnosticProductDataKind::CallableParameters,
        ProductDataKind::CallableReceiver => DiagnosticProductDataKind::CallableReceiver,
        ProductDataKind::StaticSubstitution => DiagnosticProductDataKind::StaticSubstitution,
        ProductDataKind::ImportedSemanticAddress => {
            DiagnosticProductDataKind::ImportedSemanticAddress
        }
        ProductDataKind::ResolvedType => DiagnosticProductDataKind::ResolvedType,
    }
}

const fn diagnostic_product_value_kind(kind: ProductValueKind) -> DiagnosticProductValueKind {
    match kind {
        ProductValueKind::TypeArgument => DiagnosticProductValueKind::TypeArgument,
        ProductValueKind::ConstantArgument => DiagnosticProductValueKind::ConstantArgument,
        ProductValueKind::TraitSatisfactionConstraint => {
            DiagnosticProductValueKind::TraitSatisfactionConstraint
        }
        ProductValueKind::PredicateConstraint => DiagnosticProductValueKind::PredicateConstraint,
        ProductValueKind::TypeEqualityConstraint => {
            DiagnosticProductValueKind::TypeEqualityConstraint
        }
        ProductValueKind::GenericTypeArgument => DiagnosticProductValueKind::GenericTypeArgument,
        ProductValueKind::ClosedStaticReference => {
            DiagnosticProductValueKind::ClosedStaticReference
        }
        ProductValueKind::OpenStaticReference => DiagnosticProductValueKind::OpenStaticReference,
        ProductValueKind::NamedType => DiagnosticProductValueKind::NamedType,
        ProductValueKind::CallableType => DiagnosticProductValueKind::CallableType,
        ProductValueKind::LifecycleRepresentableType => {
            DiagnosticProductValueKind::LifecycleRepresentableType
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticProductDataKind, DiagnosticProductQueryContextKind};
    use bray_source::SourceId;

    use super::diagnostic_product_query_failure;
    use crate::compilation::{
        ProductDataKind, ProductQueryContext, ProductQueryError, ProductQueryFailure,
    };

    #[test]
    fn adapter_preserves_exact_missing_product_context() {
        let error = ProductQueryError::from(ProductQueryFailure::Missing {
            context: ProductQueryContext::Source(SourceId::new(17)),
            data: ProductDataKind::SourceSnapshot,
        });

        let diagnostic = diagnostic_product_query_failure(&error);

        let bray_diagnostics::DiagnosticProductQueryFailure::Missing { context, data } = diagnostic
        else {
            panic!("missing product data should retain a structured missing-data diagnostic");
        };

        assert_eq!(context.kind(), DiagnosticProductQueryContextKind::Source);
        assert!(context.identity().contains("17"));
        assert_eq!(data, DiagnosticProductDataKind::SourceSnapshot);
    }
}
