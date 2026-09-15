use bray_binder::SymbolQueryProvider;
use bray_ir::MirUnitKey;
use bray_symbols::{CallableSignatureQuery, SymbolQueryRequest};

use crate::compilation::binder::binding_query_error;
use crate::compilation::{CodegenPreparationError, Compilation};
use crate::fact::CancellationToken;

impl Compilation {
    pub(in crate::compilation::product) fn codegen_callable_template(
        &self,
        definition: bray_symbols::CallableDefinitionId,
        cancellation: &CancellationToken,
    ) -> Result<MirUnitKey, CodegenPreparationError> {
        if let Some(body) = self.callable_body_key(definition)? {
            return Ok(MirUnitKey::Bound(body));
        }

        if self.compiler_provided_heap_method(definition)?.is_some() {
            return Ok(MirUnitKey::CompilerProvidedCallable(definition));
        }

        let skeleton = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        let Some(address) = skeleton.value().as_ref().and_then(|skeleton| {
            skeleton.imported_semantic_address(definition.callable_symbol().into_any())
        }) else {
            return self.codegen_external_callable_template(definition, cancellation);
        };

        let binding_context = self.binding_context(cancellation)?;

        let signature = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                definition.callable_symbol(),
            ))
            .map_err(binding_query_error)?;

        if !signature.value().has_body() {
            return self.codegen_external_callable_template(definition, cancellation);
        }

        let template = self.imported_executable_template_with_cancellation(
            crate::fact::ImportedExecutableTemplateAddress::root(address),
            cancellation,
        )?;

        if let Some(template) = template.value() {
            let platform_service = match template.key() {
                MirUnitKey::ImportedExecutable(key) => key.platform_service(),
                MirUnitKey::Bound(_)
                | MirUnitKey::ExternalCallable(_)
                | MirUnitKey::ExternalRuntimeDefault(_)
                | MirUnitKey::ExecutableHost(_)
                | MirUnitKey::GeneratedLifecycle(_) => None,
                MirUnitKey::CompilerProvidedCallable(_) => None,
            };

            let expected_key = MirUnitKey::ImportedExecutable(
                bray_ir::MirImportedExecutableKey::new(
                    definition.callable_symbol().into_any(),
                    bray_ir::MirExecutableTemplateId::ROOT,
                )
                .with_platform_service(platform_service),
            );

            assert_eq!(
                template.key(),
                &expected_key,
                "imported callable template at {address:?}"
            );

            let MirUnitKey::ImportedExecutable(key) = template.key() else {
                unreachable!("validated imported executable key must retain its variant");
            };

            return Ok(MirUnitKey::ImportedExecutable(*key));
        }

        let diagnostics = signature.diagnostics().merged(template.diagnostics());

        if diagnostics.has_errors() {
            return Err(CodegenPreparationError::Diagnostics(diagnostics));
        }

        self.codegen_external_callable_template(definition, cancellation)
    }

    fn codegen_external_callable_template(
        &self,
        definition: bray_symbols::CallableDefinitionId,
        cancellation: &CancellationToken,
    ) -> Result<MirUnitKey, CodegenPreparationError> {
        if let bray_symbols::CallableSymbolId::Function(function) = definition.callable_symbol() {
            let source =
                self.foreign_callable_contract_with_cancellation(function, cancellation)?;

            if source.diagnostics().has_errors() {
                return Err(CodegenPreparationError::Diagnostics(
                    source.diagnostics().clone(),
                ));
            }

            let imported =
                self.imported_native_boundary_with_cancellation(function.into(), cancellation)?;

            if source.value().as_ref().is_some_and(|contract| {
                contract.direction() == bray_symbols::ForeignCallableDirection::Import
            }) || imported.is_some_and(|contract| {
                contract.direction() == bray_symbols::ForeignCallableDirection::Import
            }) {
                return Ok(MirUnitKey::ExternalCallable(definition));
            }
        }

        let symbols = self.symbol_graph()?;
        let imported = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        let callable = crate::compilation::diagnostics::symbol_diagnostic_identity(
            &symbols,
            imported.value().as_deref(),
            definition.symbol(),
        )?;

        let source = symbols
            .declaration_syntax_anchor(definition.symbol())
            .map(|anchor| bray_source::SourceSpan::new(anchor.source_id(), anchor.full_range()));

        Err(CodegenPreparationError::MissingCallableImplementation {
            definition,
            callable: Box::new(callable),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::fact::CancellationToken;
    use crate::test_support::{compilation, compilation_with_options, source_function};
    use crate::{CodegenPreparationError, CompilationOptions, SelectedTarget, WorkerBudget};
    use bray_base::NonEmptySharedStr;
    use bray_ir::MirUnitKey;
    use bray_symbols::{CallableDefinitionId, NativeLinkKind, NativeLinkRequirement, ProductKind};

    #[test]
    fn missing_source_implementations_retain_declaration_identity_and_source() {
        let compilation = compilation("module app; func missing() -> i32;");
        let function = source_function(&compilation, "missing");
        let definition = CallableDefinitionId::try_new(function.into()).unwrap();

        let error = compilation
            .codegen_callable_template(definition, &CancellationToken::new())
            .unwrap_err();

        let CodegenPreparationError::MissingCallableImplementation {
            definition: actual,
            callable,
            source,
        } = error
        else {
            panic!("missing implementation must produce its specific compiler failure");
        };

        assert_eq!(actual, definition);
        assert!(source.is_some());

        assert!(
            matches!(*callable, bray_diagnostics::DiagnosticInterfaceSymbolIdentity::Declaration {
            identity: bray_diagnostics::DiagnosticInterfaceDeclarationIdentity::Name(ref name), ..
        } if name == "missing")
        );
    }

    #[test]
    fn native_imports_require_a_valid_foreign_contract() {
        let options = CompilationOptions::new(
            WorkerBudget::serial(),
            ProductKind::Library,
            SelectedTarget::baseline(),
        )
        .with_native_link_inputs([NativeLinkRequirement::new(
            NonEmptySharedStr::try_new("native").unwrap(),
            NativeLinkKind::Dynamic,
        )]);

        let compilation = compilation_with_options(
            r#"trusted module app;
@link(name = "native")
@symbol(name = "native_read")
@abi(c)
extern trusted func native_read(pos value: i32) -> i32 uses(foreign_call);
"#,
            options,
        );

        let definition =
            CallableDefinitionId::try_new(source_function(&compilation, "native_read").into())
                .unwrap();

        assert_eq!(
            compilation
                .codegen_callable_template(definition, &CancellationToken::new())
                .unwrap(),
            MirUnitKey::ExternalCallable(definition)
        );

        let invalid = compilation_with_options(
            r#"module app;
@symbol(name = "invalid_read")
@abi(c)
extern func invalid_read(pos value: i32) -> i32;
"#,
            CompilationOptions::new(
                WorkerBudget::serial(),
                ProductKind::Library,
                SelectedTarget::baseline(),
            ),
        );

        let definition =
            CallableDefinitionId::try_new(source_function(&invalid, "invalid_read").into())
                .unwrap();

        assert!(
            matches!(invalid.codegen_callable_template(definition, &CancellationToken::new()), Err(CodegenPreparationError::Diagnostics(diagnostics)) if diagnostics.has_errors())
        );
    }

    #[test]
    fn source_names_cannot_select_compiler_provided_heap_bodies() {
        for name in [
            "HeapStorageCreate",
            "create",
            "borrow",
            "borrow_mut",
            "destroy",
            "release",
        ] {
            let compilation = compilation(&format!("module app; func {name}() {{}}"));

            let definition =
                CallableDefinitionId::try_new(source_function(&compilation, name).into()).unwrap();

            assert_eq!(
                compilation
                    .compiler_provided_heap_method(definition)
                    .unwrap(),
                None
            );

            assert!(matches!(
                compilation
                    .codegen_callable_template(definition, &CancellationToken::new())
                    .unwrap(),
                MirUnitKey::Bound(_)
            ));
        }
    }
}
