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
    use bray_source::SourceId;
    use bray_symbols::{
        GenericSubstitutionShapeError, SemanticValueKind, SemanticValueStoreCreateError,
        SemanticValueStoreError,
    };

    use super::{CompilationFactKey, FactCycle, FactQueryError, ImportedQueryFailure};
    use crate::compilation::{
        ProductDataKind, ProductQueryContext, ProductQueryError, ProductQueryFailure,
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
        let codegen = FactQueryError::UnsupportedCodegenProfile.to_string();

        let interface = FactQueryError::PackageInterface(Box::new(
            bray_package_interface::InterfaceValidationError::InvalidMagic {
                actual: *b"not-bray",
            },
        ))
        .to_string();

        assert!(codegen.contains("no native code generator"));
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
        let cause = ProductQueryFailure::GenericSubstitution {
            substitution: None,
            cause: GenericSubstitutionShapeError::OrdinalOverflow,
        };

        let error = ProductQueryError::from(cause.clone());

        assert_eq!(
            std::mem::size_of::<ProductQueryError>(),
            std::mem::size_of::<usize>()
        );

        assert_eq!(error.cause(), &cause);

        let query = FactQueryError::from(cause.clone());

        let FactQueryError::Product(error) = query else {
            panic!("product failure must retain its boundary")
        };

        assert_eq!(error.cause(), &cause);
    }

    #[test]
    fn product_contract_failures_panic_before_becoming_query_errors() {
        let missing = std::panic::catch_unwind(|| {
            ProductQueryFailure::missing(
                ProductQueryContext::Source(SourceId::new(7)),
                ProductDataKind::DeclarationChunk,
            )
        })
        .err()
        .expect("compiler invariant must panic");

        let message = bray_testing::panic_payload_text(missing.as_ref());

        assert!(message.contains("DeclarationChunk"));
        assert!(message.contains("7"));

        let mismatch = std::panic::catch_unwind(|| {
            ProductQueryFailure::count_mismatch(
                ProductQueryContext::Source(SourceId::new(1)),
                ProductDataKind::CallableParameters,
                1,
                2,
            )
        })
        .err()
        .expect("compiler invariant must panic");

        let message = bray_testing::panic_payload_text(mismatch.as_ref());

        assert!(message.contains("CallableParameters"));
        assert!(message.contains("expected=1"));
        assert!(message.contains("actual=2"));

        let invalid_substitution = std::panic::catch_unwind(|| {
            ProductQueryError::from(ProductQueryFailure::GenericSubstitution {
                substitution: None,
                cause: GenericSubstitutionShapeError::ArgumentCountMismatch {
                    parameter_count: 1,
                    argument_count: 2,
                },
            })
        })
        .err()
        .expect("compiler invariant must panic");

        assert!(
            bray_testing::panic_payload_text(invalid_substitution.as_ref())
                .contains("ArgumentCountMismatch")
        );
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
    SymbolCapacity { index: usize },
    /// Selected native target construction violated an exact target contract.
    UnsupportedCodegenProfile,
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
    /// Binding one semantic unit violated a typed binding contract.
    BindingCapacity(bray_binder::BoundUnitConstructionError),
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
        panic!("checked callable signature construction failed: {error:?}")
    }
}

impl<Upstream> From<bray_checker::CheckerQueryError<Upstream>> for FactQueryError
where
    Upstream: Into<Self>,
{
    fn from(error: bray_checker::CheckerQueryError<Upstream>) -> Self {
        match error {
            bray_checker::CheckerQueryError::Cancelled => Self::Cancelled,
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
            Self::SymbolCapacity { index } => write!(
                formatter,
                "compilation symbol capacity exceeded at index {index}"
            ),
            Self::UnsupportedCodegenProfile => {
                formatter.write_str("the selected target profile has no native code generator")
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
            Self::BindingCapacity(error) => write!(
                formatter,
                "semantic unit binding capacity exceeded: {error:?}"
            ),
            Self::SemanticQuery(error) => write!(formatter, "{error}"),
            Self::Product(error) => write!(formatter, "product query failed: {error:?}"),
            Self::Foreign(error) => write!(formatter, "foreign query failed: {error:?}"),
            Self::MirCapacity(error) => write!(formatter, "MIR capacity exceeded: {error:?}"),
        }
    }
}

impl std::error::Error for FactQueryError {}

impl From<bray_symbols::SymbolGraphBuildError> for FactQueryError {
    #[track_caller]
    fn from(cause: bray_symbols::SymbolGraphBuildError) -> Self {
        match cause {
            bray_symbols::SymbolGraphBuildError::SymbolCapacityExceeded { index }
            | bray_symbols::SymbolGraphBuildError::CompilerKnown(
                bray_symbols::CompilerKnownSymbolBuildError::SymbolCapacityExceeded { index },
            ) => Self::SymbolCapacity { index },
            _ => panic!("declaration-backed symbol graph construction failed: {cause:?}"),
        }
    }
}

impl From<bray_codegen::CodegenTargetBuildError> for FactQueryError {
    #[track_caller]
    fn from(cause: bray_codegen::CodegenTargetBuildError) -> Self {
        match cause {
            bray_codegen::CodegenTargetBuildError::UnsupportedProfile => {
                Self::UnsupportedCodegenProfile
            }
            _ => panic!("selected target construction violated its native contract: {cause:?}"),
        }
    }
}
