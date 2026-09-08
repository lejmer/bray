use std::collections::BTreeMap;
use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_binder::SymbolQueryProvider;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    CallableAbi, CallableContractsQuery, CallableSignatureQuery, CallableSymbolId, DirectiveKind,
    ForeignCallableContract, ForeignCallableDirection, FunctionSymbolId, NativeSymbolBinding,
    NativeSymbolContract, NativeSymbolPresence, SymbolOrigin, SymbolQueryRequest,
};
use bray_syntax::{FunctionDeclarationSyntax, SyntaxKind};

use super::super::Compilation;
use super::diagnostic::{duplicate_native_symbol, missing_directive};
use super::directive::{
    foreign_link_requirements, foreign_symbol_contract, invalid_symbol_policy,
    validate_foreign_import_requirements,
};
use super::platform::platform_service_role;
use super::runtime::runtime_import_role;
use super::validation::validate_platform_service_surface;
use super::validation::{callable_surface, validate_callable_surface};
use crate::compilation::binder::binding_query_error;
use crate::compilation::directive::first_directive;
use crate::compilation::{ForeignDataKind, ForeignQueryContext, ForeignQueryFailure};
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

        let result = self.query_with_cancellation(
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
        self.query_with_cancellation(
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

        let binding_context = self.binding_context(cancellation)?;

        let record = binding_context
            .function(function)
            .map_err(binding_query_error)?
            .ok_or_else(|| {
                missing_foreign_data(
                    ForeignQueryContext::Function(function),
                    ForeignDataKind::FunctionBindingRecord,
                )
            })?;

        if record.origin() != SymbolOrigin::Source {
            return Ok(Arc::new(DiagnosticResult::without_diagnostics(None)));
        }

        let anchor = record.syntax_anchor().ok_or_else(|| {
            missing_foreign_data(
                ForeignQueryContext::Function(function),
                ForeignDataKind::SourceAnchor,
            )
        })?;

        let syntax = anchor
            .find_descendant::<FunctionDeclarationSyntax>(self.syntax_tree())
            .ok_or_else(|| {
                missing_foreign_data(
                    ForeignQueryContext::Function(function),
                    ForeignDataKind::FunctionDeclarationSyntax,
                )
            })?;

        let signature = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                CallableSymbolId::from(function),
            ))
            .map_err(binding_query_error)?;

        let mut diagnostics = signature.diagnostics().clone();

        let callable = callable_surface(function, self.semantic_value_store()?, signature.value())?;
        let abi = callable.abi;

        let declaration_directives = self.declaration_directives(function.into())?;

        diagnostics.add_range(declaration_directives.diagnostics().iter().cloned());

        let symbol_directive =
            first_directive(declaration_directives.value(), DirectiveKind::Symbol);

        let is_extern = syntax.function_modifiers().extern_token().is_some();
        let platform_role = platform_service_role(self, function)?;
        let runtime_role = runtime_import_role(self, function, cancellation)?;

        let direction = match (
            platform_role,
            runtime_role,
            is_extern,
            symbol_directive.is_some(),
        ) {
            (Some(_), _, true, _) | (_, Some(_), true, _) => Some(ForeignCallableDirection::Import),
            (Some(_), _, false, _) | (_, Some(_), false, _) => None,
            (None, None, true, _) => Some(ForeignCallableDirection::Import),
            (None, None, false, true) => Some(ForeignCallableDirection::Export),
            (None, None, false, false) => None,
        };

        if abi == CallableAbi::Bray
            && direction != Some(ForeignCallableDirection::Import)
            && platform_role.is_none()
            && runtime_role.is_none()
        {
            return Ok(Arc::new(DiagnosticResult::new(None, diagnostics)));
        }

        validate_callable_surface(
            self,
            function,
            anchor,
            &syntax,
            &callable,
            runtime_role,
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

        let symbol = match (platform_role, runtime_role, symbol_directive) {
            (Some(role), _, _) => NonEmptySharedStr::try_new(role.native_symbol())
                .map(NativeSymbolContract::required_name),
            (None, Some(role), _) => role
                .native_symbol()
                .and_then(NonEmptySharedStr::try_new)
                .map(NativeSymbolContract::required_name),
            (None, None, Some(directive)) => {
                foreign_symbol_contract(self, directive, cancellation, &mut diagnostics)?
            }
            (None, None, None) => {
                diagnostics.add(missing_directive(anchor, SyntaxKind::SymbolDirective));

                None
            }
        };

        if symbol
            .as_ref()
            .is_some_and(|symbol| symbol.presence() != NativeSymbolPresence::Required)
        {
            let anchor = symbol_directive.map_or(anchor, bray_symbols::DirectiveTemplate::syntax);

            diagnostics.add(invalid_symbol_policy(anchor, "presence"));
        }

        if direction == ForeignCallableDirection::Import
            && symbol
                .as_ref()
                .is_some_and(|symbol| symbol.binding() == NativeSymbolBinding::Weak)
        {
            let anchor = symbol_directive.map_or(anchor, bray_symbols::DirectiveTemplate::syntax);

            diagnostics.add(invalid_symbol_policy(anchor, "binding"));
        }

        let links = if direction == ForeignCallableDirection::Import
            && platform_role.is_none()
            && runtime_role.is_none()
        {
            foreign_link_requirements(
                self,
                function.into(),
                declaration_directives.value(),
                cancellation,
                &mut diagnostics,
            )?
        } else {
            Vec::new()
        };

        if abi != CallableAbi::Bray
            && direction == ForeignCallableDirection::Import
            && platform_role.is_none()
            && runtime_role.is_none()
        {
            let contracts = binding_context
                .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(
                    function.into(),
                ))
                .map_err(binding_query_error)?;

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

            let anchor = function.syntax_anchor().ok_or_else(|| {
                missing_foreign_data(
                    ForeignQueryContext::Function(function.id()),
                    ForeignDataKind::SourceAnchor,
                )
            })?;

            match native_symbols.entry(contract.symbol().identity().clone()) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(vec![anchor]);
                }
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    let display = native_symbol_display(contract.symbol().identity());

                    diagnostics.add(duplicate_native_symbol(anchor, &display, entry.get()));

                    entry.get_mut().push(anchor);
                }
            }
        }

        for static_symbol in symbols
            .statics()
            .iter()
            .filter(|static_symbol| static_symbol.origin() == SymbolOrigin::Source)
        {
            cancellation.check()?;

            let result =
                self.foreign_static_contract_with_cancellation(static_symbol.id(), cancellation)?;

            diagnostics.add_range(result.diagnostics().iter().cloned());

            let Some(contract) = result.value() else {
                continue;
            };

            let anchor = static_symbol.syntax_anchor().ok_or_else(|| {
                missing_foreign_data(
                    ForeignQueryContext::Static(static_symbol.id()),
                    ForeignDataKind::SourceAnchor,
                )
            })?;

            match native_symbols.entry(contract.symbol().identity().clone()) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(vec![anchor]);
                }
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    let display = native_symbol_display(contract.symbol().identity());

                    diagnostics.add(duplicate_native_symbol(anchor, &display, entry.get()));
                    entry.get_mut().push(anchor);
                }
            }
        }

        Ok(diagnostics)
    }
}

fn missing_foreign_data(context: ForeignQueryContext, data: ForeignDataKind) -> FactQueryError {
    ForeignQueryFailure::missing(context, data).into()
}

fn native_symbol_display(identity: &bray_symbols::NativeSymbolIdentity) -> String {
    match identity {
        bray_symbols::NativeSymbolIdentity::Name(name) => name.as_str().to_owned(),
        bray_symbols::NativeSymbolIdentity::Ordinal(ordinal) => format!("ordinal {ordinal}"),
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;
    use std::sync::Arc;

    use bray_base::NonEmptySharedStr;
    use bray_diagnostics::{
        DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticBag, DiagnosticCallableAbi,
        DiagnosticCallableExecution, DiagnosticDirectiveArgumentProblem, DiagnosticKind,
        DiagnosticNativeLinkDirectiveProblem, DiagnosticNativeSymbolDirectiveProblem,
        DiagnosticPlatformAbiType, DiagnosticPlatformServiceSignatureProblem,
    };
    use bray_runtime_interface::{PlatformServiceBinding, PlatformServiceRole};
    use bray_symbols::{
        ForeignCallableDirection, NativeLinkKind, NativeLinkRequirement, NativeSymbolBinding,
    };
    use bray_target::{TargetAbiSupport, TargetForeignAbiContract, TargetProfile};
    use bray_testing::{assert_goal_state_diagnostic_kind, assert_goal_state_diagnostics};

    use crate::test_support::{
        compilation, compilation_with_native_link, compilation_with_options, package_identity,
        source_function, source_input,
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
        assert_eq!(contract.symbol().identity().name(), Some("native_read"));
        assert_eq!(contract.links().len(), 1);
        assert_eq!(contract.links()[0].name(), "c");
    }

    #[test]
    fn imported_callables_reject_weak_binding_without_weak_required_resolution() {
        let compilation = compilation_with_link(
            r#"trusted module app;

@link(name = "c")
@symbol(name = "native_read", binding = weak)
@abi(c)
extern trusted func native_read(pos value: i32) -> i32
    uses(foreign_call);
"#,
            "c",
        );

        let result = compilation
            .foreign_callable_contract(source_function(&compilation, "native_read"))
            .unwrap_or_else(|error| panic!("foreign contract must be available: {error:?}"));

        assert!(result.value().is_none());

        assert_goal_state_diagnostic_kind(
            result.diagnostics(),
            DiagnosticKind::CheckingInvalidNativeSymbolDirective,
        );
    }

    #[test]
    fn exported_callables_preserve_weak_binding() {
        let compilation = compilation(
            r#"module app;

@symbol(name = "weak_export", binding = weak)
@abi(c)
func weak_export() -> i32
{
    return 1;
}
"#,
        );

        let result = compilation
            .foreign_callable_contract(source_function(&compilation, "weak_export"))
            .unwrap_or_else(|error| panic!("foreign contract must be available: {error:?}"));

        assert!(
            result.diagnostics().is_empty(),
            "{:#?}",
            result.diagnostics()
        );

        let contract = result
            .value()
            .as_ref()
            .unwrap_or_else(|| panic!("weak native export must publish its contract"));

        assert_eq!(contract.symbol().binding(), NativeSymbolBinding::Weak);
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

@abi(system)
extern trusted internal async func flush(pos handle: i32, pos extra: bool) -> i32
    uses(foreign_call);
"#,
            PlatformServiceRole::StandardOutputFlush,
            "app.flush",
        );

        let function = source_function(&compilation, "flush");

        let result = compilation
            .foreign_callable_contract(function)
            .unwrap_or_else(|error| panic!("platform contract query must complete: {error:?}"));

        assert_goal_state_diagnostic_kind(
            result.diagnostics(),
            DiagnosticKind::CheckingPlatformServiceSignatureMismatch,
        );

        let problems = result
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingPlatformServiceSignatureMismatch)
            .map(|diagnostic| {
                let Some(DiagnosticArgValue::PlatformServiceSignatureProblem(problem)) = diagnostic
                    .args()
                    .iter()
                    .find(|argument| {
                        argument.name() == DiagnosticArgName::PlatformServiceSignatureProblem
                    })
                    .map(DiagnosticArg::value)
                else {
                    panic!("platform signature diagnostic must retain its exact mismatch");
                };

                problem
            })
            .collect::<Vec<_>>();

        assert_eq!(problems.len(), 4, "{problems:?}");

        assert!(problems.iter().any(|problem| matches!(
            problem,
            DiagnosticPlatformServiceSignatureProblem::CallableAbi {
                actual: DiagnosticCallableAbi::System,
                ..
            }
        )));

        assert!(problems.iter().any(|problem| matches!(
            problem,
            DiagnosticPlatformServiceSignatureProblem::Execution {
                actual: DiagnosticCallableExecution::Asynchronous,
                ..
            }
        )));

        assert!(problems.iter().any(|problem| matches!(
            problem,
            DiagnosticPlatformServiceSignatureProblem::ParameterCount {
                expected: 0,
                actual: 2,
                ..
            }
        )));

        assert!(problems.iter().any(|problem| matches!(
            problem,
            DiagnosticPlatformServiceSignatureProblem::ResultType {
                expected: DiagnosticPlatformAbiType::Status,
                ..
            }
        )));

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
extern trusted internal func flush() -> PlatformStatus
    uses(foreign_call);
"#,
            PlatformServiceRole::StandardOutputFlush,
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
            contract.symbol().identity().name(),
            Some(PlatformServiceRole::StandardOutputFlush.native_symbol()),
        );
    }

    #[test]
    fn platform_service_implementations_require_the_closed_abi_shape() {
        let compilation = compilation_with_platform_service(
            r#"trusted module app;

@layout(c)
internal struct PlatformStatus
{
    category: u32;
    reserved: u32;
    native_code: i64;
}

trusted internal func flush() -> PlatformStatus
{
    return { category = 0, reserved = 0, native_code = 0 };
}
"#,
            PlatformServiceRole::StandardOutputFlush,
            "app.flush",
        );

        let result = compilation
            .foreign_callable_contract(source_function(&compilation, "flush"))
            .unwrap_or_else(|error| panic!("platform contract query must complete: {error:?}"));

        assert_goal_state_diagnostic_kind(
            result.diagnostics(),
            DiagnosticKind::CheckingPlatformServiceSignatureMismatch,
        );

        assert!(result.value().is_none());
    }

    #[test]
    fn valid_platform_service_implementations_have_no_foreign_contract() {
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
trusted internal func flush() -> PlatformStatus
{
    return { category = 0, reserved = 0, native_code = 0 };
}
"#,
            PlatformServiceRole::StandardOutputFlush,
            "app.flush",
        );

        let result = compilation
            .foreign_callable_contract(source_function(&compilation, "flush"))
            .unwrap_or_else(|error| panic!("platform contract query must complete: {error:?}"));

        assert!(
            result.diagnostics().is_empty(),
            "{:?}",
            result.diagnostics()
        );

        assert!(result.value().is_none());
    }

    #[test]
    fn platform_services_resolve_after_target_gates_remove_earlier_contributions() {
        let compilation = compilation_with_platform_service_sources(
            &[
                "@target(false) module app.disabled;",
                r#"trusted module app;

@layout(c)
internal struct PlatformStatus
{
    category: u32;
    reserved: u32;
    native_code: i64;
}

@abi(c)
extern trusted internal func flush() -> PlatformStatus
    uses(foreign_call);
"#,
            ],
            PlatformServiceRole::StandardOutputFlush,
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

        assert!(result.value().is_some());
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

        assert_eq!(contract.symbol().identity().name(), Some("native_read"));
        assert_eq!(contract.links()[0].name(), "native");
    }

    #[test]
    fn omitted_link_kinds_use_the_supplied_input_kind() {
        let compilation = compilation_with_native_link(
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

        assert_eq!(contract.symbol().identity().name(), Some("native_read"));
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

        assert_goal_state_diagnostics(result.diagnostics());

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
    fn unknown_native_link_argument_retains_its_exact_cause() {
        let compilation = compilation_with_link(
            r#"trusted module app;

@link(name = "native", unsupported = true)
@symbol(name = "native_read")
@abi(c)
extern trusted func native_read() -> i32
    uses(foreign_call);
"#,
            "native",
        );

        let function = source_function(&compilation, "native_read");

        let result = compilation
            .foreign_callable_contract(function)
            .unwrap_or_else(|error| panic!("foreign contract check must complete: {error:?}"));

        assert_goal_state_diagnostic_kind(
            result.diagnostics(),
            DiagnosticKind::CheckingInvalidNativeLinkDirective,
        );

        let diagnostic = result
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingInvalidNativeLinkDirective)
            .next()
            .unwrap_or_else(|| panic!("invalid link directive must be diagnosed"));

        let Some(DiagnosticArgValue::NativeLinkDirectiveProblem(problem)) = diagnostic
            .args()
            .iter()
            .find(|argument| argument.name() == DiagnosticArgName::NativeLinkDirectiveProblem)
            .map(DiagnosticArg::value)
        else {
            panic!("invalid link directive must retain its cause: {diagnostic:?}");
        };

        assert_eq!(
            problem,
            &DiagnosticNativeLinkDirectiveProblem::Argument(
                DiagnosticDirectiveArgumentProblem::Unknown {
                    name: String::from("unsupported"),
                },
            )
        );
    }

    #[test]
    fn duplicate_native_symbol_argument_retains_its_first_origin() {
        let compilation = compilation_with_link(
            r#"trusted module app;

@link(name = "native")
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
            .unwrap_or_else(|error| panic!("foreign contract check must complete: {error:?}"));

        assert_goal_state_diagnostic_kind(
            result.diagnostics(),
            DiagnosticKind::CheckingInvalidNativeSymbolDirective,
        );

        let diagnostic = result
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingInvalidNativeSymbolDirective)
            .next()
            .unwrap_or_else(|| panic!("invalid symbol directive must be diagnosed"));

        let Some(DiagnosticArgValue::NativeSymbolDirectiveProblem(problem)) = diagnostic
            .args()
            .iter()
            .find(|argument| argument.name() == DiagnosticArgName::NativeSymbolDirectiveProblem)
            .map(DiagnosticArg::value)
        else {
            panic!("invalid symbol directive must retain its cause: {diagnostic:?}");
        };

        assert_eq!(
            problem,
            &DiagnosticNativeSymbolDirectiveProblem::Argument(
                DiagnosticDirectiveArgumentProblem::Duplicate {
                    name: String::from("name"),
                },
            )
        );

        assert_eq!(diagnostic.related_locations().len(), 1);
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

        assert_goal_state_diagnostics(result.diagnostics());

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

        let trusted = result
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingForeignCallableRequiresTrusted)
            .next()
            .cloned()
            .unwrap_or_else(|| panic!("foreign trust failure must be present"));

        assert_goal_state_diagnostic_kind(
            &DiagnosticBag::single(trusted),
            DiagnosticKind::CheckingForeignCallableRequiresTrusted,
        );

        let capability = result
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingForeignCallableRequiresCapability)
            .next()
            .cloned()
            .unwrap_or_else(|| panic!("foreign capability failure must be present"));

        assert_goal_state_diagnostic_kind(
            &DiagnosticBag::single(capability),
            DiagnosticKind::CheckingForeignCallableRequiresCapability,
        );

        let abi_type = result
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingForeignAbiTypeUnsupported)
            .next()
            .cloned()
            .unwrap_or_else(|| panic!("foreign ABI type failure must be present"));

        assert_goal_state_diagnostic_kind(
            &DiagnosticBag::single(abi_type),
            DiagnosticKind::CheckingForeignAbiTypeUnsupported,
        );

        let link = result
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingMissingForeignCallableDirective)
            .next()
            .cloned()
            .unwrap_or_else(|| panic!("missing foreign link directive must be present"));

        assert_goal_state_diagnostic_kind(
            &DiagnosticBag::single(link),
            DiagnosticKind::CheckingMissingForeignCallableDirective,
        );
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
    fn unavailable_atomic_members_are_normal_foreign_abi_rejections() {
        let compilation = compilation_with_link(
            r#"trusted module app;

@layout(c)
struct SharedState
{
    value: core.atomic.Atomic<u128>;
}

@link(name = "native")
@symbol(name = "native_read")
@abi(c)
extern trusted func native_read(pos value: SharedState)
    uses(foreign_call);
"#,
            "native",
        );

        let function = source_function(&compilation, "native_read");

        let result = compilation
            .foreign_callable_contract(function)
            .unwrap_or_else(|error| panic!("foreign contract query must complete: {error:?}"));

        assert_goal_state_diagnostic_kind(
            result.diagnostics(),
            DiagnosticKind::CheckingForeignAbiTypeUnsupported,
        );

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

        assert_goal_state_diagnostic_kind(
            result.diagnostics(),
            DiagnosticKind::CheckingForeignCallableRequiresCapability,
        );

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

        assert_goal_state_diagnostic_kind(
            result.diagnostics(),
            DiagnosticKind::CheckingForeignCallableExecutionUnsupported,
        );

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

        assert_goal_state_diagnostic_kind(
            result.diagnostics(),
            DiagnosticKind::CheckingUnavailableNativeLinkInput,
        );

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

        assert_eq!(duplicates.len(), 2);
        assert_eq!(duplicates[0].related_locations().len(), 1);
        assert_eq!(duplicates[1].related_locations().len(), 2);

        for diagnostic in &duplicates {
            bray_testing::assert_goal_state_diagnostic(diagnostic);
        }

        assert_goal_state_diagnostic_kind(
            &diagnostics,
            DiagnosticKind::CheckingDuplicateNativeSymbol,
        );
    }

    fn compilation_with_link(source: &str, name: &str) -> crate::Compilation {
        compilation_with_native_link(source, name, NativeLinkKind::Dynamic)
    }

    fn compilation_with_platform_service(
        source: &str,
        role: PlatformServiceRole,
        path: &str,
    ) -> crate::Compilation {
        compilation_with_platform_service_sources(&[source], role, path)
    }

    fn compilation_with_platform_service_sources(
        sources: &[&str],
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
            sources
                .iter()
                .enumerate()
                .map(|(index, source)| {
                    source_input(
                        source,
                        u32::try_from(index)
                            .unwrap_or_else(|_| panic!("test source index must fit u32")),
                    )
                })
                .collect(),
            options,
        )
        .with_platform_services([binding]);

        crate::Compilation::load(request)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"))
    }

    fn compilation_with_foreign_alignment(source: &str, maximum: u64) -> crate::Compilation {
        let baseline = crate::SelectedTarget::baseline();
        let baseline_properties = baseline.profile().properties();

        let maximum = NonZeroU64::new(maximum)
            .unwrap_or_else(|| panic!("test foreign ABI alignment must be nonzero"));

        let foreign = TargetForeignAbiContract::new(
            baseline_properties
                .abis()
                .c_contract()
                .unwrap_or_else(|| panic!("baseline target must provide a C ABI"))
                .scalars(),
            true,
            true,
            true,
            true,
            true,
            maximum,
        );

        let properties = bray_target::TargetProperties::new(
            baseline_properties.identity().clone(),
            baseline_properties.scalars(),
            baseline_properties.atomics(),
            TargetAbiSupport::new(Some(foreign), Some(foreign)),
            baseline_properties.c_abi(),
            baseline_properties.address_spaces(),
            baseline_properties.alignments(),
            baseline_properties.operations(),
        );

        let profile = TargetProfile::try_new(
            baseline.profile().identity().clone(),
            baseline.profile().machine().clone(),
            properties,
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
