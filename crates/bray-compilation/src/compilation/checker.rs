use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use bray_binder::{BinderFactContext, BinderFactError, SymbolFactProvider};
use bray_bound_tree::{BoundSourceAnchor, BoundUnit, BoundUnitKey};
use bray_checker::{
    CheckedConstantTerms, CheckerFactError, CheckerFactResult, CheckerInfrastructureError,
    CheckerOutcome, CheckerRequestContext, CheckerSemanticFactProvider, CheckerSource,
    DefaultTargetValidityChecker, ImplementationHookResolution, TargetValidity,
    TargetValidityChecker, TargetValidityContext, TargetValidityRequest,
    resolve_type_expression_template,
};
use bray_compiler_known::{
    COMPILER_KNOWN_CATALOG, RecognizedStandardLibraryDeclarationIdentity,
    RecognizedStandardLibraryDeclarationOwner,
};
use bray_diagnostics::DiagnosticResult;
use bray_source::{SourceSnapshot, SourceSpan};
use bray_standard_library::{
    PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY, is_public_standard_library_package,
};
use bray_symbols::{
    AnySymbolId, AvailableCompilerKnownSymbols, DeclaredTypeRepresentation,
    GenericDeclarationTemplateFact, GenericOwnerId, MemberLookupResult, ModuleOwnerId,
    ModulePathKey, NamedTypeSymbolId, PackageIdentity, SemanticValueStore, SymbolFactContract,
    SymbolFactRequest, SymbolFactResult, SymbolKind, TraitSymbolId, TypeId,
};
use bray_target::TargetProfile;

use super::Compilation;
use super::binder::CompilationBinderFacts;
use crate::fact::{CancellationToken, FactQueryError};

pub(super) struct CompilationCheckerContext<'compilation> {
    facts: CompilationBinderFacts<'compilation>,
    recognized_standard_library_implementations:
        OnceLock<Result<BTreeMap<AnySymbolId, ImplementationHookResolution>, CheckerFactError>>,
}

impl<'compilation> CompilationCheckerContext<'compilation> {
    pub(super) fn new(facts: CompilationBinderFacts<'compilation>) -> Self {
        Self {
            facts,
            recognized_standard_library_implementations: OnceLock::new(),
        }
    }

    pub(super) fn symbols(&self) -> &bray_symbols::SymbolGraph {
        self.facts.symbols()
    }

    fn statically_establishes_copyability(
        &self,
        context: &bray_checker::SemanticUnitContext,
        ty: TypeId,
    ) -> CheckerFactResult<bool> {
        let copyable_key = bray_compiler_known::CompilerKnownDeclarationKey::try_new("Copyable")
            .ok_or(CheckerFactError::Infrastructure(
                CheckerInfrastructureError::SemanticValueUnavailable,
            ))?;

        let copyable = self
            .available_compiler_known_symbols()
            .declaration_symbol::<TraitSymbolId>(&copyable_key)
            .ok_or(CheckerFactError::Infrastructure(
                CheckerInfrastructureError::SemanticValueUnavailable,
            ))?;

        let Some(mut symbol) = self
            .symbols()
            .symbol_for_key(context.key().declared_owner())
        else {
            return Err(CheckerFactError::Infrastructure(
                CheckerInfrastructureError::SemanticValueUnavailable,
            ));
        };

        loop {
            if let Some(owner) = GenericOwnerId::try_new(symbol)
                && self.generic_owner_establishes_copyability(owner, copyable, ty)?
            {
                return Ok(true);
            }

            let Some(containing) = self.symbols().containing_symbol(symbol) else {
                break;
            };

            symbol = containing;
        }

        Ok(false)
    }

    fn generic_owner_establishes_copyability(
        &self,
        owner: GenericOwnerId,
        copyable: TraitSymbolId,
        ty: TypeId,
    ) -> CheckerFactResult<bool> {
        let generic = self
            .facts
            .symbol_fact(SymbolFactRequest::<GenericDeclarationTemplateFact>::new(
                owner,
            ))
            .map_err(|error| match error {
                BinderFactError::Cancelled => CheckerFactError::Cancelled,
                BinderFactError::DependencyUnavailable => CheckerFactError::Infrastructure(
                    CheckerInfrastructureError::SemanticValueUnavailable,
                ),
            })?;

        for constraint in generic.value().constraints() {
            let Some((subject, application)) = constraint.trait_satisfaction_templates() else {
                continue;
            };

            if application.definition() != copyable || !application.arguments().is_empty() {
                continue;
            }

            let constants = self.checked_constraint_constants(subject, application)?;

            let Some(subject) =
                resolve_type_expression_template(self.facts.semantic_values(), subject, &constants)
                    .map_err(CheckerFactError::Infrastructure)?
            else {
                continue;
            };

            if subject == ty {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn checked_constraint_constants(
        &self,
        subject: &bray_symbols::TypeExpressionTemplate,
        application: &bray_symbols::TraitApplicationTemplate,
    ) -> CheckerFactResult<CheckedConstantTerms> {
        let mut terms = Vec::new();

        for occurrence in subject
            .constant_expressions()
            .into_iter()
            .chain(application.constant_expressions())
        {
            let result = self
                .facts
                .compilation()
                .embedded_constant_term_with_cancellation(occurrence, self.facts.cancellation())
                .map_err(checker_fact_error)?;

            if result.diagnostics().has_errors() {
                continue;
            }

            terms.push((occurrence.key(), *result.value()));
        }

        CheckedConstantTerms::try_from_terms(terms).map_err(|_| {
            CheckerFactError::Infrastructure(CheckerInfrastructureError::SemanticValueUnavailable)
        })
    }

    fn recognized_standard_library_implementations(
        &self,
    ) -> CheckerFactResult<&BTreeMap<AnySymbolId, ImplementationHookResolution>> {
        self.recognized_standard_library_implementations
            .get_or_init(|| self.build_recognized_standard_library_implementations())
            .as_ref()
            .map_err(|error| *error)
    }

    fn build_recognized_standard_library_implementations(
        &self,
    ) -> CheckerFactResult<BTreeMap<AnySymbolId, ImplementationHookResolution>> {
        let mut implementations = self.source_standard_library_implementations()?;

        let imported = self
            .facts
            .compilation()
            .imported_symbol_skeleton_result_with_cancellation(self.facts.cancellation())
            .map_err(checker_fact_error)?;

        let Some(imported) = imported.value() else {
            return Ok(implementations);
        };

        let standard_library = standard_library_package_identity()?;

        let all = Arc::clone(imported).recognize_standard_library(&standard_library, |_| true);
        let target = self.facts.compilation().selected_target().target();

        let available = Arc::clone(imported)
            .recognize_standard_library(&standard_library, |rule| target.supports(rule));

        for declaration in all.declarations() {
            let Some(descriptor) = COMPILER_KNOWN_CATALOG
                .recognized_standard_library_declaration(declaration.descriptor())
            else {
                continue;
            };

            let Some(hook) = descriptor.implementation_hook() else {
                continue;
            };

            implementations.insert(
                declaration.symbol(),
                ImplementationHookResolution::new(
                    hook,
                    available.descriptor(declaration.symbol()).is_some(),
                ),
            );
        }

        Ok(implementations)
    }

    fn source_standard_library_implementations(
        &self,
    ) -> CheckerFactResult<BTreeMap<AnySymbolId, ImplementationHookResolution>> {
        if !is_public_standard_library_source(self.facts.compilation()) {
            return Ok(BTreeMap::new());
        }

        let standard_library = standard_library_package_identity()?;

        let symbols = self.symbols();

        let Some(package) = symbols
            .packages()
            .iter()
            .find(|package| package.identity() == &standard_library)
        else {
            return Ok(BTreeMap::new());
        };

        let target = self.facts.compilation().selected_target().target();
        let mut implementations = BTreeMap::new();

        for descriptor in COMPILER_KNOWN_CATALOG.recognized_standard_library_declarations() {
            let Some(hook) = descriptor.implementation_hook() else {
                continue;
            };

            let RecognizedStandardLibraryDeclarationOwner::Scope(scope) = descriptor.owner() else {
                continue;
            };

            let Some(scope) = COMPILER_KNOWN_CATALOG.recognized_standard_library_scope(scope)
            else {
                continue;
            };

            let Some(path) = ModulePathKey::try_new(scope.path().segments()) else {
                continue;
            };

            let Some(module) = symbols.module_by_path(ModuleOwnerId::from(package.id()), &path)
            else {
                continue;
            };

            let RecognizedStandardLibraryDeclarationIdentity::Name(name) = descriptor.identity()
            else {
                continue;
            };

            let MemberLookupResult::Found(symbol) =
                symbols.lookup_member(module.id().into(), name.as_ref())
            else {
                continue;
            };

            if symbol.kind() != SymbolKind::Function {
                continue;
            }

            implementations.insert(
                symbol,
                ImplementationHookResolution::new(
                    hook,
                    target.supports(descriptor.availability_rule()),
                ),
            );
        }

        Ok(implementations)
    }
}

fn is_public_standard_library_source(compilation: &Compilation) -> bool {
    compilation.package_source_authority().is_standard_library()
        && is_public_standard_library_package(compilation.package_identity())
}

fn standard_library_package_identity() -> CheckerFactResult<PackageIdentity> {
    PackageIdentity::try_new(PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY).ok_or(
        CheckerFactError::Infrastructure(CheckerInfrastructureError::SemanticValueUnavailable),
    )
}

impl CheckerRequestContext for CompilationCheckerContext<'_> {
    fn semantic_context_matches(
        &self,
        unit: &BoundUnit,
        context: &bray_checker::SemanticUnitContext,
    ) -> bool {
        bray_binder::semantic_unit_context(self.symbols(), unit)
            .is_ok_and(|expected| expected == *context)
    }

    fn semantic_values(&self) -> &SemanticValueStore {
        self.facts.semantic_values()
    }

    fn symbols(&self) -> &bray_symbols::SymbolGraph {
        CompilationCheckerContext::symbols(self)
    }

    fn available_compiler_known_symbols(&self) -> &AvailableCompilerKnownSymbols {
        self.facts
            .compilation()
            .selected_target()
            .available_compiler_known_symbols()
    }

    fn recognized_standard_library_implementation_hook(
        &self,
        symbol: AnySymbolId,
    ) -> CheckerFactResult<Option<ImplementationHookResolution>> {
        Ok(self
            .recognized_standard_library_implementations()?
            .get(&symbol)
            .copied())
    }

    fn selected_target(&self) -> &TargetProfile {
        self.facts
            .compilation()
            .selected_target()
            .target()
            .profile()
    }

    fn checked_constant_expression(
        &self,
        occurrence: bray_symbols::ConstantExpressionOccurrence,
    ) -> CheckerFactResult<DiagnosticResult<bray_symbols::ConstantTermId>> {
        // The checker request contract returns an owned result across the crate boundary.
        self.facts
            .compilation()
            .embedded_constant_term_with_cancellation(occurrence, self.facts.cancellation())
            .map(|result| (*result).clone())
            .map_err(checker_fact_error)
    }

    fn generic_constraints(
        &self,
        obligation: bray_symbols::GenericConstraintObligationKey,
    ) -> CheckerFactResult<DiagnosticResult<bray_symbols::ProofOutcome>> {
        match self
            .facts
            .compilation()
            .generic_constraint_satisfaction_with_cancellation(
                obligation,
                self.facts.cancellation(),
            ) {
            Ok(result) => Ok((*result).clone()),
            Err(FactQueryError::Cycle(_)) => Ok(DiagnosticResult::without_diagnostics(
                bray_symbols::ProofOutcome::Unknown,
            )),
            Err(error) => Err(checker_fact_error(error)),
        }
    }

    fn declared_type_representation(
        &self,
        subject: NamedTypeSymbolId,
    ) -> CheckerFactResult<DiagnosticResult<DeclaredTypeRepresentation>> {
        // The checker request contract returns an owned result across the crate boundary.
        self.facts
            .compilation()
            .declared_type_representation_with_cancellation(subject, self.facts.cancellation())
            .map(|result| (*result).clone())
            .map_err(checker_fact_error)
    }

    fn statically_establishes_copyability(
        &self,
        context: &bray_checker::SemanticUnitContext,
        ty: TypeId,
    ) -> CheckerFactResult<bool> {
        CompilationCheckerContext::statically_establishes_copyability(self, context, ty)
    }

    fn source(
        &self,
        anchor: BoundSourceAnchor,
    ) -> Result<CheckerSource<'_>, CheckerInfrastructureError> {
        checker_source(self.facts.compilation(), anchor)
    }

    fn cancellation(&self) -> &dyn bray_base::Cancellation {
        self.facts.cancellation()
    }
}

pub(in crate::compilation) fn checker_fact_error(error: FactQueryError) -> CheckerFactError {
    match error {
        FactQueryError::Cancelled => CheckerFactError::Cancelled,
        FactQueryError::CheckerInfrastructure(error) => CheckerFactError::Infrastructure(error),
        FactQueryError::Cycle(_)
        | FactQueryError::InfrastructureFailure
        | FactQueryError::SemanticUnitContext(_) => {
            CheckerFactError::Infrastructure(CheckerInfrastructureError::SemanticValueUnavailable)
        }
    }
}

struct CompilationTargetValidityContext<'compilation> {
    compilation: &'compilation Compilation,
    cancellation: &'compilation CancellationToken,
}

impl TargetValidityContext for CompilationTargetValidityContext<'_> {
    fn selected_target(&self) -> &TargetProfile {
        self.compilation.selected_target().target().profile()
    }

    fn source(
        &self,
        anchor: BoundSourceAnchor,
    ) -> Result<CheckerSource<'_>, CheckerInfrastructureError> {
        checker_source(self.compilation, anchor)
    }

    fn cancellation(&self) -> &dyn bray_base::Cancellation {
        self.cancellation
    }
}

fn checker_source_snapshot(
    compilation: &Compilation,
    anchor: BoundSourceAnchor,
) -> Result<&SourceSnapshot, CheckerInfrastructureError> {
    let source_id = anchor.syntax().source_id();

    let Some(source) = compilation.source(source_id) else {
        return Err(CheckerInfrastructureError::MissingSource { source_id });
    };

    if source.version() != anchor.source_version() {
        return Err(CheckerInfrastructureError::SourceVersionMismatch {
            source_id,
            expected: anchor.source_version(),
            actual: source.version(),
        });
    }

    Ok(source)
}

fn checker_source(
    compilation: &Compilation,
    anchor: BoundSourceAnchor,
) -> Result<CheckerSource<'_>, CheckerInfrastructureError> {
    let source = checker_source_snapshot(compilation, anchor)?;
    let range = anchor.syntax().full_range();
    let span = SourceSpan::new(source.source_id(), range);

    let Some(text) = source.text_slice(range) else {
        return Err(CheckerInfrastructureError::InvalidSourceRange { span });
    };

    Ok(CheckerSource::new(span, text))
}

impl<'compilation, C> CheckerSemanticFactProvider<C> for CompilationCheckerContext<'compilation>
where
    C: SymbolFactContract,
    CompilationBinderFacts<'compilation>: SymbolFactProvider<C>,
{
    fn symbol_fact(
        &self,
        request: SymbolFactRequest<C>,
    ) -> CheckerFactResult<Arc<SymbolFactResult<C>>> {
        self.facts
            .symbol_fact(request)
            .map_err(|error| match error {
                BinderFactError::Cancelled => CheckerFactError::Cancelled,
                BinderFactError::DependencyUnavailable => CheckerFactError::Infrastructure(
                    CheckerInfrastructureError::SemanticFactUnavailable {
                        symbol: request.symbol(),
                        kind: request.kind(),
                    },
                ),
            })
    }
}

impl Compilation {
    /// Returns post-selection validity and diagnostics for one exact target requirement.
    pub fn target_validity(
        &self,
        request: TargetValidityRequest,
    ) -> Result<Arc<DiagnosticResult<TargetValidity>>, FactQueryError> {
        self.target_validity_with_cancellation(request, &self.state.cancellation)
    }

    pub(in crate::compilation) fn target_validity_with_cancellation(
        &self,
        request: TargetValidityRequest,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<TargetValidity>>, FactQueryError> {
        let cell = self.state.target_validity.cell(request.clone())?;

        let published = self.query_fact_with_cancellation(
            crate::fact::CompilationFactKey::TargetValidity(request.clone()),
            &cell,
            cancellation,
            |cancellation| {
                let context = CompilationTargetValidityContext {
                    compilation: self,
                    cancellation,
                };

                checker_result(
                    DefaultTargetValidityChecker.check_target_validity(&context, &request),
                )
                .map(Arc::new)
            },
        )?;

        // The caller owns the immutable publication independently of the map cell guard.
        Ok(Arc::clone(published))
    }

    pub(super) fn checker_context<'compilation>(
        &'compilation self,
        cancellation: &'compilation CancellationToken,
    ) -> Result<CompilationCheckerContext<'compilation>, FactQueryError> {
        let facts = self.binder_facts(cancellation)?;

        Ok(CompilationCheckerContext::new(facts))
    }

    pub(super) fn checker_context_for<'compilation>(
        &'compilation self,
        key: &BoundUnitKey,
        cancellation: &'compilation CancellationToken,
    ) -> Result<CompilationCheckerContext<'compilation>, FactQueryError> {
        let facts = self.binder_facts_for(key, cancellation)?;

        Ok(CompilationCheckerContext::new(facts))
    }
}

pub(super) fn checker_result<T>(
    outcome: CheckerOutcome<T>,
) -> Result<DiagnosticResult<T>, FactQueryError> {
    match outcome {
        CheckerOutcome::Complete(result) => Ok(result),
        CheckerOutcome::Cancelled => Err(FactQueryError::Cancelled),
        CheckerOutcome::InfrastructureFailure(error) => {
            Err(FactQueryError::CheckerInfrastructure(error))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_binder::semantic_unit_context;
    use bray_bound_tree::BoundSourceAnchor;
    use bray_checker::{
        CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView, TargetValidity,
        TargetValidityRequest, TargetValidityRequirement,
    };
    use bray_compiler_known::RepresentationRole;
    use bray_diagnostics::DiagnosticKind;
    use bray_source::SourceVersion;
    use bray_symbols::{
        CallableSignatureFact, CallableSymbolId, PackageIdentity, SymbolFactRequest, SymbolOrigin,
    };

    use super::{Compilation, is_public_standard_library_source};
    use crate::fact::CompilationFactKey;
    use crate::request::CompilationRequest;
    use crate::test_support::{compilation, source_callable_body_key, source_input};

    #[test]
    fn contexts_resolve_only_the_source_range_named_by_a_bound_anchor() {
        let compilation = callable_compilation();
        let key = source_callable_body_key(&compilation);

        let context = match compilation.checker_context_for(&key, &compilation.state.cancellation) {
            Ok(context) => context,
            Err(error) => panic!("checker context must be available: {error:?}"),
        };

        let anchor = key.source();

        let source = match context.source(anchor) {
            Ok(source) => source,
            Err(error) => panic!("bound source must resolve: {error:?}"),
        };

        let span = source.span();

        assert_eq!(span.source_id(), anchor.syntax().source_id());
        assert_eq!(span.range(), anchor.syntax().full_range());

        let Some(snapshot) = compilation.source(span.source_id()) else {
            panic!("test compilation must retain its source");
        };

        assert_eq!(snapshot.text_slice(span.range()), Some(source.text()));
        assert_ne!(source.text(), snapshot.text());
    }

    #[test]
    fn contexts_reject_bound_anchors_from_another_source_revision() {
        let compilation = callable_compilation();
        let key = source_callable_body_key(&compilation);

        let context = match compilation.checker_context_for(&key, &compilation.state.cancellation) {
            Ok(context) => context,
            Err(error) => panic!("checker context must be available: {error:?}"),
        };

        let source = key.source();

        let stale = BoundSourceAnchor::new(
            source.syntax(),
            SourceVersion::new(source.source_version().raw() + 1),
        );

        assert!(matches!(
            context.source(stale),
            Err(CheckerInfrastructureError::SourceVersionMismatch { .. })
        ));
    }

    #[test]
    fn contexts_supply_typed_symbol_facts_without_origin_specific_apis() {
        let compilation = callable_compilation();
        let key = source_callable_body_key(&compilation);

        let context = match compilation.checker_context_for(&key, &compilation.state.cancellation) {
            Ok(context) => context,
            Err(error) => panic!("checker context must be available: {error:?}"),
        };

        let bound = match compilation.bound_unit(key.clone()) {
            Ok(bound) => bound,
            Err(error) => panic!("bound unit must be available: {error:?}"),
        };

        let entry = match semantic_unit_context(context.symbols(), bound.value()) {
            Ok(entry) => entry,
            Err(error) => panic!("semantic unit context must be available: {error:?}"),
        };

        let request = match CheckerUnitView::new(bound.value(), &entry, &context) {
            Ok(request) => request,
            Err(error) => panic!("checker unit view must be valid: {error:?}"),
        };

        let graph = match compilation.symbol_graph() {
            Ok(graph) => graph,
            Err(error) => panic!("symbol graph must be available: {error:?}"),
        };

        let source_functions = graph
            .functions()
            .iter()
            .filter(|function| function.origin() == SymbolOrigin::Source)
            .collect::<Vec<_>>();

        let [function] = source_functions.as_slice() else {
            panic!("test source must contain one function");
        };

        let signature_request =
            SymbolFactRequest::<CallableSignatureFact>::new(CallableSymbolId::from(function.id()));

        let signature = request.symbol_fact(signature_request);

        assert!(signature.is_ok());
    }

    #[test]
    fn target_validity_reuses_one_exact_published_fact() {
        let compilation = callable_compilation();
        let source = source_callable_body_key(&compilation).source();

        let request = TargetValidityRequest::new(
            source,
            TargetValidityRequirement::Representation(RepresentationRole::ScalarR16),
        );

        let first = match compilation.target_validity(request.clone()) {
            Ok(result) => result,
            Err(error) => panic!("target validity must be available: {error:?}"),
        };

        let second = match compilation.target_validity(request) {
            Ok(result) => result,
            Err(error) => panic!("target validity must be available: {error:?}"),
        };

        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn unconditional_target_validity_does_not_demand_target_or_source() {
        let compilation = callable_compilation();
        let source = source_callable_body_key(&compilation).source();

        let stale_source = BoundSourceAnchor::new(
            source.syntax(),
            SourceVersion::new(source.source_version().raw() + 1),
        );

        let request = TargetValidityRequest::new(
            stale_source,
            TargetValidityRequirement::Representation(RepresentationRole::ScalarI32),
        );

        let key = CompilationFactKey::TargetValidity(request.clone());

        let validity = match compilation.target_validity(request) {
            Ok(result) => result,
            Err(error) => panic!("unconditional target validity must be available: {error:?}"),
        };

        let dependencies = match compilation.state.fact_runtime.dependencies(&key) {
            Ok(Some(dependencies)) => dependencies,
            Ok(None) => panic!("target-validity dependencies must be published"),
            Err(error) => panic!("target-validity dependencies must be readable: {error:?}"),
        };

        assert_eq!(*validity.value(), TargetValidity::Valid);
        assert!(validity.diagnostics().is_empty());
        assert!(dependencies.is_empty());
    }

    #[test]
    fn target_dependent_invalidity_demands_target_and_reports_source() {
        let compilation = callable_compilation();
        let source = source_callable_body_key(&compilation).source();

        let request = TargetValidityRequest::new(
            source,
            TargetValidityRequirement::Representation(RepresentationRole::ScalarR16),
        );

        let key = CompilationFactKey::TargetValidity(request.clone());

        let validity = match compilation.target_validity(request) {
            Ok(result) => result,
            Err(error) => panic!("target validity must be available: {error:?}"),
        };

        let dependencies = match compilation.state.fact_runtime.dependencies(&key) {
            Ok(Some(dependencies)) => dependencies,
            Ok(None) => panic!("target-validity dependencies must be published"),
            Err(error) => panic!("target-validity dependencies must be readable: {error:?}"),
        };

        let [diagnostic] = validity.diagnostics().diagnostics() else {
            panic!("invalid target requirement must report one diagnostic");
        };

        assert_eq!(*validity.value(), TargetValidity::Invalid);

        assert_eq!(
            diagnostic.kind(),
            DiagnosticKind::CheckingTargetRepresentationUnavailable
        );

        assert_eq!(
            diagnostic.primary_span().map(|span| span.range()),
            Some(source.syntax().full_range())
        );

        assert_eq!(dependencies.as_ref(), [CompilationFactKey::SelectedTarget]);
    }

    #[test]
    fn source_standard_library_recognition_requires_exact_public_package_identity() {
        let public = standard_library_compilation("std");
        let support = standard_library_compilation("std.runtime");

        assert!(is_public_standard_library_source(&public));
        assert!(!is_public_standard_library_source(&support));
    }

    fn callable_compilation() -> Compilation {
        compilation(
            r#"module app;
func main()
{
}
"#,
        )
    }

    fn standard_library_compilation(package: &str) -> Compilation {
        let package = PackageIdentity::try_new(package)
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let request = CompilationRequest::new(package, vec![source_input("module std;", 0)])
            .with_standard_library_source_authority();

        Compilation::load(request)
            .unwrap_or_else(|error| panic!("standard library compilation must load: {error:?}"))
    }
}
