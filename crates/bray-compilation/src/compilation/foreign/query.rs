use std::collections::BTreeMap;
use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_binder::SymbolFactProvider;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    CallableAbi, CallableContractsFact, CallableSignatureFact, CallableSymbolId, DirectiveKind,
    ForeignCallableContract, ForeignCallableDirection, FunctionSymbolId, SymbolFactRequest,
    SymbolOrigin,
};
use bray_syntax::{FunctionDeclarationSyntax, SyntaxKind};

use super::super::Compilation;
use super::diagnostic::{duplicate_native_symbol, missing_directive};
use super::directive::{
    foreign_link_requirements, foreign_symbol_name, validate_foreign_import_requirements,
};
use super::platform::platform_service_role;
use super::validation::validate_platform_service_surface;
use super::validation::{callable_surface, validate_callable_surface};
use crate::compilation::binder::binder_fact_error;
use crate::compilation::directive::first_directive;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError};

impl Compilation {
    /// Returns the validated foreign-boundary contract of one source function.
    pub fn foreign_callable_contract(
        &self,
        function: FunctionSymbolId,
    ) -> Result<Arc<DiagnosticResult<Option<ForeignCallableContract>>>, FactQueryError> {
        self.foreign_callable_contract_with_cancellation(function, &self.state.cancellation)
    }

    pub(in crate::compilation) fn foreign_callable_contract_with_cancellation(
        &self,
        function: FunctionSymbolId,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<Option<ForeignCallableContract>>>, FactQueryError> {
        let cell = self.state.foreign_callable_contracts.cell(function)?;

        let result = self.query_fact_with_cancellation(
            CompilationFactKey::ForeignCallableContract(function),
            &cell,
            cancellation,
            |cancellation| self.compute_foreign_callable_contract(function, cancellation),
        )?;

        // The caller owns the immutable publication independently of the map cell guard.
        Ok(Arc::clone(result))
    }

    pub(in crate::compilation) fn foreign_callable_diagnostics(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<&DiagnosticBag, FactQueryError> {
        self.query_fact_with_cancellation(
            CompilationFactKey::ForeignCallableValidation,
            &self.state.foreign_callable_validation,
            cancellation,
            |cancellation| self.compute_foreign_callable_diagnostics(cancellation),
        )
    }

    fn compute_foreign_callable_contract(
        &self,
        function: FunctionSymbolId,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<Option<ForeignCallableContract>>>, FactQueryError> {
        cancellation.check()?;

        let facts = self.binder_facts(cancellation)?;

        let record = facts
            .function(function)
            .map_err(binder_fact_error)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        if record.origin() != SymbolOrigin::Source {
            return Ok(Arc::new(DiagnosticResult::without_diagnostics(None)));
        }

        let anchor = record
            .syntax_anchor()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let syntax = anchor
            .find_descendant::<FunctionDeclarationSyntax>(self.syntax_tree())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let signature = facts
            .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(
                CallableSymbolId::from(function),
            ))
            .map_err(binder_fact_error)?;

        let mut diagnostics = signature.diagnostics().clone();

        let callable = callable_surface(self.semantic_value_store()?, signature.value())?;
        let abi = callable.abi;

        let declaration_directives = self.declaration_directives(function.into())?;

        diagnostics.add_range(declaration_directives.diagnostics().iter().cloned());

        let symbol_directive =
            first_directive(declaration_directives.value(), DirectiveKind::Symbol);

        let is_extern = syntax.function_modifiers().extern_token().is_some();
        let platform_role = platform_service_role(self, function)?;

        let direction = match (platform_role, is_extern, symbol_directive.is_some()) {
            (Some(_), true, _) => Some(ForeignCallableDirection::Import),
            (Some(_), false, _) => None,
            (None, true, _) => Some(ForeignCallableDirection::Import),
            (None, false, true) => Some(ForeignCallableDirection::Export),
            (None, false, false) => None,
        };

        if abi == CallableAbi::Bray && direction != Some(ForeignCallableDirection::Import) {
            return Ok(Arc::new(DiagnosticResult::new(None, diagnostics)));
        }

        validate_callable_surface(
            self,
            function,
            anchor,
            &syntax,
            &callable,
            cancellation,
            &mut diagnostics,
        )?;

        if let Some(role) = platform_role {
            validate_platform_service_surface(
                self,
                role,
                anchor,
                &callable,
                cancellation,
                &mut diagnostics,
            )?;
        }

        let Some(direction) = direction else {
            return Ok(Arc::new(DiagnosticResult::new(None, diagnostics)));
        };

        if abi == CallableAbi::Bray && symbol_directive.is_none() {
            return Ok(Arc::new(DiagnosticResult::new(None, diagnostics)));
        }

        let symbol = match (platform_role, symbol_directive) {
            (Some(role), _) => NonEmptySharedStr::try_new(
                bray_runtime_interface::native_platform_service_role_symbol(role),
            ),
            (None, Some(directive)) => {
                foreign_symbol_name(self, directive, cancellation, &mut diagnostics)?
            }
            (None, None) => {
                diagnostics.add(missing_directive(anchor, SyntaxKind::SymbolDirective));

                None
            }
        };

        let links = if direction == ForeignCallableDirection::Import && platform_role.is_none() {
            foreign_link_requirements(
                self,
                function,
                declaration_directives.value(),
                cancellation,
                &mut diagnostics,
            )?
        } else {
            Vec::new()
        };

        if abi != CallableAbi::Bray && direction == ForeignCallableDirection::Import {
            let contracts = facts
                .symbol_fact(SymbolFactRequest::<CallableContractsFact>::new(
                    function.into(),
                ))
                .map_err(binder_fact_error)?;

            diagnostics.add_range(contracts.diagnostics().iter().cloned());

            validate_foreign_import_requirements(self, anchor, contracts.value(), &mut diagnostics);
        }

        let contract = if diagnostics.has_errors() {
            None
        } else {
            symbol
                .map(|symbol| ForeignCallableContract::new(function, direction, abi, symbol, links))
        };

        Ok(Arc::new(DiagnosticResult::new(contract, diagnostics)))
    }

    fn compute_foreign_callable_diagnostics(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticBag, FactQueryError> {
        let symbols = self.symbol_graph()?;
        let mut diagnostics = DiagnosticBag::new();
        let mut native_symbols = BTreeMap::new();

        for function in symbols
            .functions()
            .iter()
            .filter(|function| function.origin() == SymbolOrigin::Source)
        {
            cancellation.check()?;

            let result =
                self.foreign_callable_contract_with_cancellation(function.id(), cancellation)?;

            diagnostics.add_range(result.diagnostics().iter().cloned());

            let Some(contract) = result.value() else {
                continue;
            };

            let anchor = function
                .syntax_anchor()
                .ok_or(FactQueryError::InfrastructureFailure)?;

            match native_symbols.entry(contract.symbol().to_owned()) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(anchor);
                }
                std::collections::btree_map::Entry::Occupied(entry) => {
                    diagnostics.add(duplicate_native_symbol(
                        anchor,
                        contract.symbol(),
                        *entry.get(),
                    ));
                }
            }
        }

        Ok(diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;
    use std::sync::Arc;

    use bray_base::NonEmptySharedStr;
    use bray_diagnostics::DiagnosticKind;
    use bray_runtime_interface::{PlatformServiceBinding, PlatformServiceRole};
    use bray_symbols::{ForeignCallableDirection, NativeLinkKind, NativeLinkRequirement};
    use bray_target::{TargetAbiFacts, TargetForeignAbiFacts, TargetProfile};

    use crate::test_support::{
        compilation, compilation_with_options, package_identity, source_function, source_input,
    };
    use crate::{CompilationOptions, CompilationRequest, WorkerBudget};

    #[test]
    fn valid_foreign_imports_publish_typed_boundary_contracts() {
        let compilation = compilation_with_link(
            r#"trusted module app;

@link(name = "c")
@symbol(name = "native_read")
@abi(c)
extern trusted func native_read(pos value: i32) -> i32
    uses(foreign_call);
"#,
            "c",
        );

        let function = source_function(&compilation, "native_read");

        let result = compilation
            .foreign_callable_contract(function)
            .unwrap_or_else(|error| panic!("foreign contract must be available: {error:?}"));

        assert!(result.diagnostics().is_empty());

        let Some(contract) = result.value() else {
            panic!("valid foreign import must publish a contract");
        };

        assert_eq!(contract.direction(), ForeignCallableDirection::Import);
        assert_eq!(contract.symbol(), "native_read");
        assert_eq!(contract.links().len(), 1);
        assert_eq!(contract.links()[0].name(), "c");
    }

    #[test]
    fn platform_services_require_their_closed_abi_shape() {
        let compilation = compilation_with_platform_service(
            r#"trusted module app;

@layout(c)
internal struct PlatformStatus
{
    category: u32;
    reserved: u32;
    native_code: i64;
}

@abi(c)
extern trusted internal func flush(pos handle: u32) -> PlatformStatus
    uses(foreign_call);
"#,
            PlatformServiceRole::StreamFlush,
            "app.flush",
        );

        let function = source_function(&compilation, "flush");

        let result = compilation
            .foreign_callable_contract(function)
            .unwrap_or_else(|error| panic!("platform contract query must complete: {error:?}"));

        assert!(result.diagnostics().iter().any(|diagnostic| {
            diagnostic.kind() == DiagnosticKind::CheckingPlatformServiceSignatureMismatch
        }));

        assert!(result.value().is_none());
    }

    #[test]
    fn valid_platform_services_use_the_runtime_role_symbol() {
        let compilation = compilation_with_platform_service(
            r#"trusted module app;

@layout(c)
internal struct PlatformStatus
{
    category: u32;
    reserved: u32;
    native_code: i64;
}

@abi(c)
extern trusted internal func flush(pos handle: u64) -> PlatformStatus
    uses(foreign_call);
"#,
            PlatformServiceRole::StreamFlush,
            "app.flush",
        );

        let function = source_function(&compilation, "flush");

        let result = compilation
            .foreign_callable_contract(function)
            .unwrap_or_else(|error| panic!("platform contract query must complete: {error:?}"));

        assert!(
            result.diagnostics().is_empty(),
            "{:?}",
            result.diagnostics()
        );

        let Some(contract) = result.value() else {
            panic!("valid platform service must publish a contract");
        };

        assert_eq!(
            contract.symbol(),
            bray_runtime_interface::native_platform_service_role_symbol(
                PlatformServiceRole::StreamFlush,
            ),
        );
    }

    #[test]
    fn ordinary_bray_externs_do_not_require_foreign_contracts() {
        let compilation = compilation(
            r#"module app;

extern func supplied_elsewhere();
"#,
        );

        let function = source_function(&compilation, "supplied_elsewhere");

        let result = compilation
            .foreign_callable_contract(function)
            .unwrap_or_else(|error| panic!("extern contract query must complete: {error:?}"));

        assert!(result.diagnostics().is_empty());
        assert!(result.value().is_none());
    }

    #[test]
    fn module_links_and_checked_symbol_names_are_preserved() {
        let compilation = compilation_with_link(
            r#"@link(name = "native")
trusted module app;

@symbol(name = "native_read")
@abi(c)
extern trusted func native_read() -> i32
    uses(foreign_call);
"#,
            "native",
        );

        let function = source_function(&compilation, "native_read");

        let first = compilation
            .foreign_callable_contract(function)
            .unwrap_or_else(|error| panic!("foreign contract must be available: {error:?}"));

        let second = compilation
            .foreign_callable_contract(function)
            .unwrap_or_else(|error| panic!("cached foreign contract must be available: {error:?}"));

        assert!(Arc::ptr_eq(&first, &second));
        assert!(first.diagnostics().is_empty(), "{:?}", first.diagnostics());

        let Some(contract) = first.value() else {
            panic!("valid foreign import must publish a contract");
        };

        assert_eq!(contract.symbol(), "native_read");
        assert_eq!(contract.links()[0].name(), "native");
    }

    #[test]
    fn omitted_link_kinds_use_the_supplied_input_kind() {
        let compilation = compilation_with_link_kind(
            r#"trusted module app;

@link(name = "native")
@symbol(name = "native_read")
@abi(c)
extern trusted func native_read() -> i32
    uses(foreign_call);
"#,
            "native",
            NativeLinkKind::Static,
        );

        let function = source_function(&compilation, "native_read");

        let result = compilation
            .foreign_callable_contract(function)
            .unwrap_or_else(|error| panic!("foreign contract must be available: {error:?}"));

        assert!(
            result.diagnostics().is_empty(),
            "{:?}",
            result.diagnostics()
        );

        let Some(contract) = result.value() else {
            panic!("valid foreign import must publish a contract");
        };

        assert_eq!(contract.links()[0].kind(), NativeLinkKind::Static);
    }

    #[test]
    fn directive_names_can_use_constant_references_and_calls() {
        let compilation = compilation_with_link(
            r#"trusted module app;

const link_name: string = "native";

const func symbol_name() -> string
{
    return "native_read";
}

@link(name = link_name)
@symbol(name = symbol_name())
@abi(c)
extern trusted func native_read() -> i32
    uses(foreign_call);
"#,
            "native",
        );

        let function = source_function(&compilation, "native_read");

        let result = compilation
            .foreign_callable_contract(function)
            .unwrap_or_else(|error| panic!("foreign contract must be available: {error:?}"));

        assert!(
            result.diagnostics().is_empty(),
            "{:?}",
            result.diagnostics()
        );

        let Some(contract) = result.value() else {
            panic!("valid foreign import must publish a contract");
        };

        assert_eq!(contract.symbol(), "native_read");
    }

    #[test]
    fn duplicate_and_unknown_directive_arguments_are_rejected() {
        let compilation = compilation_with_link(
            r#"trusted module app;

@link(name = "native", unsupported = true)
@symbol(name = "native_read", name = "other")
@abi(c)
extern trusted func native_read() -> i32
    uses(foreign_call);
"#,
            "native",
        );

        let function = source_function(&compilation, "native_read");

        let result = compilation
            .foreign_callable_contract(function)
            .unwrap_or_else(|error| panic!("foreign contract query must complete: {error:?}"));

        let kinds = result
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.kind())
            .collect::<Vec<_>>();

        assert!(kinds.contains(&DiagnosticKind::CheckingInvalidNativeLinkDirective));
        assert!(kinds.contains(&DiagnosticKind::CheckingInvalidNativeSymbolDirective));
        assert!(result.value().is_none());
    }

    #[test]
    fn foreign_imports_report_missing_boundary_requirements() {
        let compilation = compilation(
            r#"module app;

@abi(c)
extern func native_read(pos value: &i32) -> i32;
"#,
        );

        let function = source_function(&compilation, "native_read");

        let result = compilation
            .foreign_callable_contract(function)
            .unwrap_or_else(|error| panic!("foreign contract query must complete: {error:?}"));

        let kinds = result
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.kind())
            .collect::<Vec<_>>();

        assert!(kinds.contains(&DiagnosticKind::CheckingForeignCallableRequiresTrusted));
        assert!(kinds.contains(&DiagnosticKind::CheckingForeignCallableRequiresCapability));
        assert!(kinds.contains(&DiagnosticKind::CheckingForeignAbiTypeUnsupported));
        assert!(kinds.contains(&DiagnosticKind::CheckingMissingForeignCallableDirective));
        assert!(result.value().is_none());
    }

    #[test]
    fn source_foreign_abi_rejects_target_overalignment() {
        let compilation = compilation_with_foreign_alignment(
            r#"trusted module app;

@layout(c)
struct Wide
{
    value: i64;
}

@link(name = "native")
@symbol(name = "native_read")
@abi(c)
extern trusted func native_read(pos value: Wide)
    uses(foreign_call);
"#,
            4,
        );

        let function = source_function(&compilation, "native_read");

        let result = compilation
            .foreign_callable_contract(function)
            .unwrap_or_else(|error| panic!("foreign contract query must complete: {error:?}"));

        assert!(result.diagnostics().iter().any(|diagnostic| {
            diagnostic.kind() == DiagnosticKind::CheckingTargetAlignmentUnsupported
        }));

        assert!(result.value().is_none());
    }

    #[test]
    fn foreign_imports_require_the_exact_compiler_known_capability() {
        let compilation = compilation_with_link(
            r#"trusted module app;

trusted predicate foreign_call();

@link(name = "native")
@symbol(name = "native_read")
@abi(c)
extern trusted func native_read() -> i32
    uses(app.foreign_call);
"#,
            "native",
        );

        let function = source_function(&compilation, "native_read");

        let result = compilation
            .foreign_callable_contract(function)
            .unwrap_or_else(|error| panic!("foreign contract query must complete: {error:?}"));

        assert!(result.diagnostics().iter().any(|diagnostic| {
            diagnostic.kind() == DiagnosticKind::CheckingForeignCallableRequiresCapability
        }));

        assert!(result.value().is_none());
    }

    #[test]
    fn asynchronous_foreign_callables_are_rejected() {
        let compilation = compilation(
            r#"module app;

@symbol(name = "async_entry")
@abi(c)
async func async_entry()
{
}
"#,
        );

        let function = source_function(&compilation, "async_entry");

        let result = compilation
            .foreign_callable_contract(function)
            .unwrap_or_else(|error| panic!("foreign contract query must complete: {error:?}"));

        assert!(result.diagnostics().iter().any(|diagnostic| {
            diagnostic.kind() == DiagnosticKind::CheckingForeignCallableExecutionUnsupported
        }));

        assert!(result.value().is_none());
    }

    #[test]
    fn unavailable_native_link_inputs_are_rejected() {
        let compilation = compilation(
            r#"trusted module app;

@link(name = "missing")
@symbol(name = "native_read")
@abi(c)
extern trusted func native_read() -> i32
    uses(foreign_call);
"#,
        );

        let function = source_function(&compilation, "native_read");

        let result = compilation
            .foreign_callable_contract(function)
            .unwrap_or_else(|error| panic!("foreign contract query must complete: {error:?}"));

        assert!(result.diagnostics().iter().any(|diagnostic| {
            diagnostic.kind() == DiagnosticKind::CheckingUnavailableNativeLinkInput
        }));

        assert!(result.value().is_none());
    }

    #[test]
    fn duplicate_native_symbols_are_rejected_deterministically() {
        let compilation = compilation(
            r#"module app;

@symbol(name = "same")
@abi(c)
func first()
{
}

@symbol(name = "same")
@abi(c)
func second()
{
}

@symbol(name = "same")
@abi(c)
func third()
{
}
"#,
        );

        let diagnostics = compilation.semantic_diagnostics();

        let duplicates = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.kind() == DiagnosticKind::CheckingDuplicateNativeSymbol)
            .collect::<Vec<_>>();

        let first_spans = duplicates
            .iter()
            .map(|diagnostic| diagnostic.labels()[0].span())
            .collect::<Vec<_>>();

        assert_eq!(duplicates.len(), 2);
        assert_eq!(first_spans[0], first_spans[1]);
    }

    fn compilation_with_link(source: &str, name: &str) -> crate::Compilation {
        compilation_with_link_kind(source, name, NativeLinkKind::Dynamic)
    }

    fn compilation_with_platform_service(
        source: &str,
        role: PlatformServiceRole,
        path: &str,
    ) -> crate::Compilation {
        let Some(binding) = PlatformServiceBinding::try_new(role, path) else {
            panic!("test platform binding must be valid");
        };

        let options = CompilationOptions::new(
            WorkerBudget::serial(),
            bray_symbols::ProductKind::Library,
            crate::SelectedTarget::baseline(),
        );

        let request = CompilationRequest::with_options(
            package_identity(),
            vec![source_input(source, 0)],
            options,
        )
        .with_platform_services([binding]);

        crate::Compilation::load(request)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"))
    }

    fn compilation_with_link_kind(
        source: &str,
        name: &str,
        kind: NativeLinkKind,
    ) -> crate::Compilation {
        let Some(name) = NonEmptySharedStr::try_new(name) else {
            panic!("test link input name must be valid");
        };

        let options = CompilationOptions::new(
            WorkerBudget::serial(),
            bray_symbols::ProductKind::Library,
            crate::SelectedTarget::baseline(),
        )
        .with_native_link_inputs([NativeLinkRequirement::new(name, kind)]);

        compilation_with_options(source, options)
    }

    fn compilation_with_foreign_alignment(source: &str, maximum: u64) -> crate::Compilation {
        let baseline = crate::SelectedTarget::baseline();
        let baseline_facts = baseline.profile().facts();

        let maximum = NonZeroU64::new(maximum)
            .unwrap_or_else(|| panic!("test foreign ABI alignment must be nonzero"));

        let foreign = TargetForeignAbiFacts::new(
            baseline_facts
                .abis()
                .c_contract()
                .unwrap_or_else(|| panic!("baseline target must provide a C ABI"))
                .scalars(),
            true,
            true,
            true,
            true,
            maximum,
        );

        let facts = bray_target::TargetFacts::new(
            baseline_facts.identity().clone(),
            baseline_facts.scalars(),
            baseline_facts.atomics(),
            TargetAbiFacts::new(Some(foreign), Some(foreign)),
            baseline_facts.address_spaces(),
            baseline_facts.alignments(),
            baseline_facts.operations(),
        );

        let profile = TargetProfile::try_new(
            baseline.profile().identity().clone(),
            baseline.profile().machine().clone(),
            facts,
        )
        .unwrap_or_else(|error| panic!("test target profile must be valid: {error:?}"));

        let target = crate::SelectedTarget::new(profile, baseline.runtime_abi());

        let options = CompilationOptions::new(
            WorkerBudget::serial(),
            bray_symbols::ProductKind::Library,
            target,
        )
        .with_native_link_inputs([NativeLinkRequirement::new(
            NonEmptySharedStr::try_new("native")
                .unwrap_or_else(|| panic!("test link input name must be valid")),
            NativeLinkKind::Dynamic,
        )]);

        compilation_with_options(source, options)
    }
}
