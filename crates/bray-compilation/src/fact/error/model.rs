use bray_checker::CheckerInfrastructureError;

use crate::compilation::{SemanticQueryError, SemanticQueryFailure};
use crate::fact::{CompilationFactKey, FactRuntimeError, FactRuntimeFailure};

/// One detected cycle in the compilation fact dependency graph.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct FactCycle {
    facts: Box<[CompilationFactKey]>,
}

/// A resource limit encountered while loading imported interfaces.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ImportedQueryFailure {
    InterfaceCapacityExceeded(usize),
}

impl FactCycle {
    pub(crate) fn new(facts: impl Into<Box<[CompilationFactKey]>>) -> Self {
        Self {
            facts: canonical_cycle(facts.into()),
        }
    }

    pub(crate) fn facts(&self) -> &[CompilationFactKey] {
        &self.facts
    }
}

fn canonical_cycle(facts: Box<[CompilationFactKey]>) -> Box<[CompilationFactKey]> {
    let mut facts = facts.into_vec();

    let Some(closing) = facts.last().cloned() else {
        return facts.into_boxed_slice();
    };

    let Some(start) = facts[..facts.len().saturating_sub(1)]
        .iter()
        .position(|fact| fact == &closing)
    else {
        return facts.into_boxed_slice();
    };

    facts.drain(..start);

    let cycle_len = facts.len().saturating_sub(1);

    let Some((canonical_start, _)) = facts[..cycle_len]
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| left.cmp(right))
    else {
        return facts.into_boxed_slice();
    };

    let mut canonical = facts[..cycle_len]
        .iter()
        .cycle()
        .skip(canonical_start)
        .take(cycle_len)
        .cloned()
        .collect::<Vec<_>>();

    if let Some(first) = canonical.first().cloned() {
        canonical.push(first);
    }

    canonical.into_boxed_slice()
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticFailureValue, DiagnosticSemanticValueFailure};
    use bray_source::{SourceId, SourceVersion};
    use bray_symbols::{
        GenericSubstitutionShapeError, SemanticValueKind, SemanticValueStoreCreateError,
        SemanticValueStoreError,
    };

    use super::{CompilationFactKey, FactCycle, FactQueryError, ImportedQueryFailure};
    use crate::compilation::{
        ProductDataKind, ProductQueryContext, ProductQueryError, ProductQueryErrorKind,
        ProductQueryFailure, ProductSynchronizationComponent,
    };
    use crate::fact::diagnostic_semantic_value_failure;

    #[test]
    fn cycle_paths_remove_prefixes_and_use_a_canonical_start() {
        let syntax = CompilationFactKey::SyntaxTree;
        let declaration = CompilationFactKey::DeclarationTable;
        let prefix = CompilationFactKey::SourceUnitSyntax(SourceId::new(0));

        let cycle = FactCycle::new([prefix, syntax.clone(), declaration.clone(), syntax.clone()]);

        assert_eq!(cycle.facts(), &[declaration.clone(), syntax, declaration]);

        let diagnostic = crate::fact::diagnostic_cycle_failure(&cycle);

        assert_eq!(diagnostic.reason(), "cycle");

        assert_eq!(
            diagnostic.context()[0].value(),
            &DiagnosticFailureValue::TextList(
                ["declaration_table", "syntax_tree", "declaration_table"]
                    .map(str::to_owned)
                    .into(),
            )
        );

        assert!(matches!(
            diagnostic.context()[1].value(),
            DiagnosticFailureValue::IdentityList(values) if values.len() == 3
        ));
    }

    #[test]
    fn public_fact_errors_render_exact_lower_layer_causes() {
        let codegen =
            FactQueryError::CodegenTarget(bray_codegen::CodegenTargetBuildError::EmptyTriple)
                .to_string();

        let interface = FactQueryError::PackageInterface(Box::new(
            bray_package_interface::InterfaceValidationError::InvalidMagic {
                actual: *b"not-bray",
            },
        ))
        .to_string();

        assert!(codegen.contains("EmptyTriple"));
        assert!(interface.contains("InvalidMagic"));
        assert!(interface.contains("110, 111, 116, 45, 98, 114, 97, 121"));
    }

    #[test]
    fn imported_query_failures_keep_rare_payloads_out_of_query_stack_frames() {
        assert!(std::mem::size_of::<ImportedQueryFailure>() <= 4 * std::mem::size_of::<usize>());
    }

    #[test]
    fn fact_query_failures_keep_rare_payloads_out_of_query_stack_frames() {
        assert!(std::mem::size_of::<FactQueryError>() <= 8 * std::mem::size_of::<usize>());
    }

    #[test]
    fn semantic_value_failures_retain_every_leaf_payload() {
        let capacity = SemanticValueStoreError::CapacityExhausted {
            kind: SemanticValueKind::ConstantTerm,
        };

        assert_eq!(
            diagnostic_semantic_value_failure(capacity),
            DiagnosticSemanticValueFailure::CapacityExhausted {
                kind: "constant_term",
            }
        );

        assert_eq!(
            FactQueryError::from(SemanticValueStoreCreateError::IdentitySpaceExhausted),
            FactQueryError::SemanticValueStoreCreate(
                SemanticValueStoreCreateError::IdentitySpaceExhausted,
            )
        );
    }

    #[test]
    fn product_query_errors_box_and_retain_exact_private_causes() {
        let cause = ProductQueryFailure::missing(
            ProductQueryContext::Source(SourceId::new(7)),
            ProductDataKind::DeclarationChunk,
        );

        let error = ProductQueryError::from(cause.clone());

        assert_eq!(
            std::mem::size_of::<ProductQueryError>(),
            std::mem::size_of::<usize>()
        );

        assert_eq!(error.kind(), ProductQueryErrorKind::MissingData);
        assert_eq!(error.cause(), &cause);

        let query = FactQueryError::from(cause.clone());

        let FactQueryError::Product(error) = query else {
            panic!("product failure must retain the product-query boundary")
        };

        assert_eq!(error.cause(), &cause);
    }

    #[test]
    fn product_query_error_kinds_preserve_domain_boundaries() {
        let cases = [
            (
                ProductQueryFailure::count_mismatch(
                    ProductQueryContext::Source(SourceId::new(1)),
                    ProductDataKind::CallableParameters,
                    1,
                    2,
                ),
                ProductQueryErrorKind::ContractViolation,
            ),
            (
                ProductQueryFailure::GenericSubstitution {
                    substitution: None,
                    cause: GenericSubstitutionShapeError::ArgumentCountMismatch {
                        parameter_count: 1,
                        argument_count: 2,
                    },
                },
                ProductQueryErrorKind::Specialization,
            ),
            (
                ProductQueryFailure::SynchronizationPoisoned {
                    component: ProductSynchronizationComponent::LifecycleNeeds,
                },
                ProductQueryErrorKind::Coordination,
            ),
            (
                ProductQueryFailure::InvalidTestErrorTypeIdentity { digest: [7; 32] },
                ProductQueryErrorKind::ContractViolation,
            ),
            (
                ProductQueryFailure::SourceSnapshotMismatch {
                    source: SourceId::new(2),
                    expected: SourceVersion::new(3),
                    actual: Some(SourceVersion::new(4)),
                },
                ProductQueryErrorKind::Identity,
            ),
        ];

        for (cause, expected) in cases {
            let error = ProductQueryError::from(cause.clone());

            assert_eq!(error.kind(), expected);
            assert_eq!(error.cause(), &cause);
        }
    }
}

/// An outer compiler-query outcome that must not be represented as a source diagnostic.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum FactQueryError {
    /// The requesting operation was cancelled before completion.
    Cancelled,
    /// Evaluation encountered a same-worker or cross-worker dependency cycle.
    Cycle(FactCycle),
    /// Immutable symbol-graph construction violated an exact structural contract.
    SymbolGraph(bray_symbols::SymbolGraphBuildError),
    /// Selected native target construction violated an exact target contract.
    CodegenTarget(bray_codegen::CodegenTargetBuildError),
    /// Imported package implementation access violated its encoded interface contract.
    PackageInterface(Box<bray_package_interface::InterfaceValidationError>),
    /// Imported-interface query state omitted an exact required relationship.
    ImportedQuery(ImportedQueryFailure),
    /// The compiler query runtime could not preserve its coordination contract.
    Runtime(FactRuntimeError),
    /// The compilation could not allocate its canonical semantic-value store identity.
    SemanticValueStoreCreate(bray_symbols::SemanticValueStoreCreateError),
    /// The canonical semantic-value store rejected a construction or lookup operation.
    SemanticValueStore(bray_symbols::SemanticValueStoreError),
    /// Name binding could not obtain a required semantic dependency.
    BindingDependencyUnavailable,
    /// Binding one semantic unit violated a typed binding contract.
    Binding(bray_binder::BoundUnitBindingError),
    /// A selected constant callable has no body available for durable evaluation.
    ConstantCallableBodyUnavailable,
    /// A selected constant callable body has no evaluable result expression.
    ConstantCallableRootUnavailable,
    /// The atomic initializer argument has no available compile-time value.
    AtomicInitializerArgumentUnavailable,
    /// The atomic initializer result cannot be retained as a compile-time value.
    AtomicInitializerResultUnavailable,
    /// The uninitialized-storage initializer result cannot be retained as a compile-time value.
    UninitInitializerResultUnavailable,
    /// Semantic checking could not complete because a typed dependency was unavailable.
    CheckerInfrastructure(CheckerInfrastructureError),
    /// Binding or semantic compilation violated an exact query contract.
    SemanticQuery(SemanticQueryError),
    /// Product specialization or realization violated an exact query contract.
    Product(crate::ProductQueryError),
    /// Foreign-boundary compilation violated an exact query contract.
    Foreign(crate::ForeignQueryError),
    /// MIR lowering exhausted a compact identity space.
    MirCapacity(bray_ir::MirCapacityError),
}

impl FactQueryError {
    /// Converts this query failure into its exact locale-neutral diagnostic payload.
    ///
    pub fn diagnostic_evaluation_failure(
        &self,
    ) -> bray_diagnostics::DiagnosticEmissionEvaluationFailure {
        crate::compilation::diagnostic_evaluation_failure(self)
    }
}

impl From<std::convert::Infallible> for FactQueryError {
    fn from(error: std::convert::Infallible) -> Self {
        match error {}
    }
}

impl From<CheckerInfrastructureError> for FactQueryError {
    fn from(error: CheckerInfrastructureError) -> Self {
        Self::CheckerInfrastructure(error)
    }
}

impl From<FactRuntimeFailure> for FactQueryError {
    fn from(error: FactRuntimeFailure) -> Self {
        Self::Runtime(error.into())
    }
}

impl From<SemanticQueryFailure> for FactQueryError {
    fn from(error: SemanticQueryFailure) -> Self {
        Self::SemanticQuery(error.into())
    }
}

impl From<crate::compilation::ProductQueryFailure> for FactQueryError {
    fn from(error: crate::compilation::ProductQueryFailure) -> Self {
        Self::Product(error.into())
    }
}

impl From<crate::compilation::ForeignQueryFailure> for FactQueryError {
    fn from(error: crate::compilation::ForeignQueryFailure) -> Self {
        Self::Foreign(error.into())
    }
}

impl From<ImportedQueryFailure> for FactQueryError {
    fn from(error: ImportedQueryFailure) -> Self {
        Self::ImportedQuery(error)
    }
}

impl From<bray_package_interface::InterfaceValidationError> for FactQueryError {
    fn from(error: bray_package_interface::InterfaceValidationError) -> Self {
        Self::PackageInterface(Box::new(error))
    }
}

impl From<bray_symbols::SemanticValueStoreCreateError> for FactQueryError {
    fn from(error: bray_symbols::SemanticValueStoreCreateError) -> Self {
        Self::SemanticValueStoreCreate(error)
    }
}

impl From<bray_symbols::SemanticValueStoreError> for FactQueryError {
    fn from(error: bray_symbols::SemanticValueStoreError) -> Self {
        Self::SemanticValueStore(error)
    }
}

impl From<bray_symbols::CallableSignatureTemplateError> for FactQueryError {
    fn from(error: bray_symbols::CallableSignatureTemplateError) -> Self {
        SemanticQueryFailure::CallableSignature {
            callable: None,
            cause: error,
        }
        .into()
    }
}

impl<Upstream> From<bray_checker::CheckerQueryError<Upstream>> for FactQueryError
where
    Upstream: Into<Self>,
{
    fn from(error: bray_checker::CheckerQueryError<Upstream>) -> Self {
        match error {
            bray_checker::CheckerQueryError::Cancelled => Self::Cancelled,
            bray_checker::CheckerQueryError::Infrastructure(error) => {
                Self::CheckerInfrastructure(error)
            }
            bray_checker::CheckerQueryError::Upstream(error) => error.into(),
        }
    }
}

impl std::fmt::Display for FactQueryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("fact evaluation was cancelled"),
            Self::Cycle(cycle) => write!(
                formatter,
                "fact evaluation encountered a dependency cycle: {:?}",
                cycle.facts()
            ),
            Self::SymbolGraph(error) => {
                write!(formatter, "symbol graph construction failed: {error}")
            }
            Self::CodegenTarget(error) => {
                write!(formatter, "native target construction failed: {error:?}")
            }
            Self::PackageInterface(error) => {
                write!(formatter, "package interface validation failed: {error:?}")
            }
            Self::ImportedQuery(error) => {
                write!(formatter, "imported interface query failed: {error:?}")
            }
            Self::Runtime(error) => write!(formatter, "{error}"),
            Self::SemanticValueStoreCreate(error) => {
                write!(formatter, "semantic value store creation failed: {error:?}")
            }
            Self::SemanticValueStore(error) => {
                write!(
                    formatter,
                    "semantic value store operation failed: {error:?}"
                )
            }
            Self::BindingDependencyUnavailable => {
                formatter.write_str("name binding could not obtain a required dependency")
            }
            Self::Binding(error) => write!(formatter, "semantic unit binding failed: {error:?}"),
            Self::ConstantCallableBodyUnavailable => {
                formatter.write_str("the constant callable has no available body")
            }
            Self::ConstantCallableRootUnavailable => {
                formatter.write_str("the constant callable body has no result expression")
            }
            Self::AtomicInitializerArgumentUnavailable => {
                formatter.write_str("the atomic initializer argument is unavailable")
            }
            Self::AtomicInitializerResultUnavailable => {
                formatter.write_str("the atomic initializer result cannot be retained")
            }
            Self::UninitInitializerResultUnavailable => formatter
                .write_str("the uninitialized-storage initializer result cannot be retained"),
            Self::CheckerInfrastructure(error) => {
                write!(
                    formatter,
                    "semantic checking infrastructure failed: {error:?}"
                )
            }
            Self::SemanticQuery(error) => write!(formatter, "{error}"),
            Self::Product(error) => write!(formatter, "product query failed: {error:?}"),
            Self::Foreign(error) => write!(formatter, "foreign query failed: {error:?}"),
            Self::MirCapacity(error) => write!(formatter, "MIR capacity exceeded: {error:?}"),
        }
    }
}

impl std::error::Error for FactQueryError {}
