use bray_codegen::{CodegenCallableSignature, CodegenParameterMapping, CodegenResultMapping};
use bray_compiler_known::RepresentationRole;
use bray_runtime_interface::{NATIVE_PANIC_REPORT_FIELDS, RuntimeAbiRole, RuntimeAbiType};
use bray_symbols::CallableAbi;

use super::super::super::Compilation;
use super::super::super::foreign::{AbiField, abi_type_matches};
use crate::fact::{CancellationToken, FactQueryError};

const BYTE_POINTER: AbiField = AbiField::Pointer(RepresentationRole::ScalarU8);
const U32: AbiField = AbiField::Scalar(RepresentationRole::ScalarU32);
const U64: AbiField = AbiField::Scalar(RepresentationRole::ScalarU64);
const USIZE: AbiField = AbiField::Scalar(RepresentationRole::ScalarUsize);
const PANIC_REPORT_FIELDS: [AbiField; 19] = {
    let mut fields = [AbiField::RawPointer; NATIVE_PANIC_REPORT_FIELDS.len()];
    let mut index = 0;

    while index < fields.len() {
        fields[index] = match NATIVE_PANIC_REPORT_FIELDS[index] {
            RuntimeAbiType::U32 => U32,
            RuntimeAbiType::U64 => U64,
            RuntimeAbiType::Usize => USIZE,
            RuntimeAbiType::Pointer => BYTE_POINTER,
            _ => panic!("panic report field has an unsupported native ABI type"),
        };

        index += 1;
    }

    fields
};
const PANIC_REPORT: AbiField = AbiField::Struct(&PANIC_REPORT_FIELDS);
macro_rules! define_runtime_source_fields {
    ($( $role:ident {
        $documentation:literal, $name:literal,
        native: ($($symbol:ident = $native:literal, [$($native_parameter:ident),*] -> $native_result:ident)?),
        $(service: $service:ident,)?
        call_hook: ($($hook:ident)?),
        compiler: $abi:ident [$($parameter:ident),*] -> $result:ident,
        owner: $owner:ident, availability: $availability:ident,
        bootstrap: ($($bootstrap:literal)?), host_control: $host_control:literal,
        capabilities: [$($capability:ident),*],
        effects: [$($effect:ident),*]
    })+) => {
        fn runtime_source_fields(role: RuntimeAbiRole) -> Option<(&'static [AbiField], Option<AbiField>)> {
            match role {
                $(RuntimeAbiRole::$role => define_runtime_source_fields!(
                    @bootstrap ($($bootstrap)?) ($([$($native_parameter),*] -> $native_result)?)
                ),)+
            }
        }
    };
    (@bootstrap () $native:tt) => { None };
    (@bootstrap ($name:literal) ([$($parameter:ident),*] -> $result:ident)) => {
        Some((&[$(define_runtime_source_fields!(@parameter $parameter),)*], define_runtime_source_fields!(@result $result)))
    };
    (@field U8) => { AbiField::Scalar(RepresentationRole::ScalarU8) };
    (@field U32) => { U32 };
    (@field U64) => { U64 };
    (@field Usize) => { USIZE };
    (@field Pointer) => { AbiField::RawPointer };
    (@parameter PanicReport) => { AbiField::RawPointer };
    (@parameter $kind:ident) => { define_runtime_source_fields!(@field $kind) };
    (@field PanicReport) => { PANIC_REPORT };
    (@result Void) => { None };
    (@result $kind:ident) => { Some(define_runtime_source_fields!(@field $kind)) };
}

bray_runtime_abi::runtime_role_catalog!(define_runtime_source_fields);

pub(super) fn checked_runtime_source_signature(
    compilation: &Compilation,
    role: RuntimeAbiRole,
    instance: &bray_codegen::CodegenInstanceKey,
    signature: CodegenCallableSignature,
    cancellation: &CancellationToken,
) -> Result<CodegenCallableSignature, crate::compilation::CodegenPreparationError> {
    use crate::compilation::CodegenPreparationError;

    if !runtime_source_contract_matches(compilation, role, instance.template(), cancellation)? {
        return Err(CodegenPreparationError::InvalidRuntimeRoleSourceContract {
            role,
            // Retain the rejected identity only on the diagnostic path.
            instance: instance.clone(),
        });
    }

    let expected = compilation.codegen_runtime_signature(role)?;

    let matches = runtime_source_signature_matches(compilation, role, &signature, cancellation)?
        .unwrap_or(signature == expected);

    if !matches {
        return Err(CodegenPreparationError::InvalidRuntimeRoleSourceBinding {
            role,
            expected,
            actual: signature,
        });
    }

    Ok(signature)
}

pub(super) fn runtime_source_contract_matches(
    compilation: &Compilation,
    role: RuntimeAbiRole,
    key: &bray_ir::MirUnitKey,
    cancellation: &CancellationToken,
) -> Result<bool, FactQueryError> {
    if !matches!(
        role,
        RuntimeAbiRole::EntryContextRegistration
            | RuntimeAbiRole::SynchronousRootExecution
            | RuntimeAbiRole::ForeignCallbackExecution
            | RuntimeAbiRole::EntryContinuation
            | RuntimeAbiRole::ThreadStorageExit
            | RuntimeAbiRole::ThreadAttachmentLeave
            | RuntimeAbiRole::ThreadAttachmentEnter
            | RuntimeAbiRole::ThreadAttachmentMarkMain
    ) {
        return Ok(true);
    }

    let bray_ir::MirUnitKey::Bound(body) = key else {
        return Ok(false);
    };

    let declared = compilation.execution_declaration(body.source().syntax())?;

    let conditions = compilation.predicate_condition_inputs(
        body,
        declared.value().requirements(),
        cancellation,
    )?;

    if declared.diagnostics().has_errors() || conditions.diagnostics().has_errors() {
        return Ok(false);
    }

    let [(condition, true, _)] = conditions.value().as_slice() else {
        return Ok(false);
    };

    let condition = match condition {
        bray_checker::ExecutionCondition::Trusted(condition) => condition.as_ref(),
        condition => condition,
    };

    let bray_checker::ExecutionCondition::Predicate(
        bray_symbols::PredicateDefinitionSymbolId::Predicate(predicate),
        substitution,
        arguments,
    ) = condition
    else {
        return Ok(false);
    };

    if !arguments.is_empty()
        || !compilation
            .semantic_value_store()?
            .generic_substitution_data(*substitution)
            .bindings()
            .is_empty()
    {
        return Ok(false);
    }

    let definition = compilation.predicate_definition(
        bray_symbols::PredicateDefinitionSymbolId::Predicate(*predicate),
    )?;

    // The explicit runtime binding establishes this entry's opaque authority.
    // Source spelling and visibility cannot select a role or prove the condition.
    Ok(!definition.diagnostics().has_errors()
        && matches!(
            definition.value(),
            bray_symbols::PredicateDefinitionState::OpaqueTrusted
        ))
}

pub(super) fn runtime_source_signature_matches(
    compilation: &Compilation,
    role: RuntimeAbiRole,
    signature: &CodegenCallableSignature,
    cancellation: &CancellationToken,
) -> Result<Option<bool>, FactQueryError> {
    let Some((parameters, result)) = runtime_source_fields(role) else {
        return Ok(None);
    };

    if signature.abi() != runtime_role_abi(compilation, role, CallableAbi::C)
        || signature.is_variadic()
        || signature.has_panic_report_context()
            != bray_runtime_interface::NativeEntryKind::for_runtime_role(role).is_some()
        || signature.parameters().len() != parameters.len()
    {
        return Ok(Some(false));
    }

    for (parameter, expected) in signature.parameters().iter().zip(parameters) {
        let CodegenParameterMapping::Direct {
            ty,
            coercion: None,
            extension: None,
            attributes,
        } = parameter
        else {
            return Ok(Some(false));
        };

        if !attributes.is_empty() || !abi_type_matches(compilation, *ty, expected, cancellation)? {
            return Ok(Some(false));
        }
    }

    let Some(expected_result) = &result else {
        return Ok(Some(matches!(
            signature.result(),
            CodegenResultMapping::Void
        )));
    };

    let CodegenResultMapping::Direct {
        ty,
        coercion: None,
        extension: None,
        attributes,
    } = signature.result()
    else {
        return Ok(Some(false));
    };

    Ok(Some(
        attributes.is_empty() && abi_type_matches(compilation, *ty, expected_result, cancellation)?,
    ))
}

pub(super) fn runtime_role_abi(
    compilation: &Compilation,
    role: RuntimeAbiRole,
    declared: CallableAbi,
) -> CallableAbi {
    if role == RuntimeAbiRole::ThreadStorageExit
        && compilation
            .selected_target()
            .target()
            .profile()
            .properties()
            .identity()
            .system()
            == "windows"
    {
        return CallableAbi::System;
    }

    declared
}

#[cfg(test)]
mod tests {
    use bray_codegen::{CodegenCallableSignature, CodegenParameterMapping, CodegenResultMapping};
    use bray_compiler_known::RepresentationRole;
    use bray_runtime_interface::RuntimeAbiRole;
    use bray_symbols::CallableAbi;

    use super::runtime_source_signature_matches;
    use crate::CancellationToken;
    use crate::test_support::compilation;

    #[test]
    fn attachment_retirement_roles_require_one_opaque_entry_condition_independent_of_spelling() {
        for (role, function) in [
            (RuntimeAbiRole::EntryContextRegistration, "install"),
            (RuntimeAbiRole::SynchronousRootExecution, "root"),
            (RuntimeAbiRole::ForeignCallbackExecution, "foreign"),
            (RuntimeAbiRole::EntryContinuation, "invoke"),
            (
                RuntimeAbiRole::ThreadAttachmentMarkMain,
                "thread_attachment_mark_main",
            ),
            (RuntimeAbiRole::ThreadStorageExit, "thread_storage_exit"),
            (
                RuntimeAbiRole::ThreadAttachmentLeave,
                "thread_attachment_leave",
            ),
            (
                RuntimeAbiRole::ThreadAttachmentEnter,
                "thread_attachment_enter",
            ),
        ] {
            for (module, requirement, expected) in [
                (
                    "bray.runtime.bootstrap.thread.destruction",
                    "requires(trusted native_attachment_exit())",
                    true,
                ),
                ("bray.runtime.bootstrap.thread.destruction", "", false),
                (
                    "bray.runtime.bootstrap.thread.destruction",
                    "requires(trusted unrelated())",
                    true,
                ),
                (
                    "bray.runtime.bootstrap.thread.destruction",
                    "requires(trusted native_attachment_exit(), value != core.memory.null<u8>())",
                    false,
                ),
                ("app", "requires(trusted native_attachment_exit())", true),
            ] {
                let compilation = compilation(&format!(
                    r#"
                trusted internal module {module};
                trusted predicate native_attachment_exit();
                trusted predicate unrelated();
                @abi(c)
                trusted internal func entry(pos value: RawPointer<u8>) {requirement} {{}}
            "#
                ));

                let key = crate::test_support::source_function_body_key(&compilation, "entry");

                assert_eq!(
                    super::runtime_source_contract_matches(
                        &compilation,
                        role,
                        &bray_ir::MirUnitKey::Bound(key),
                        &CancellationToken::new()
                    )
                    .unwrap(),
                    expected,
                    "{module}: {requirement}"
                );
            }

            for (declaration, requirement) in [
                ("predicate available() = true;", "available()"),
                (
                    "trusted predicate available<T>();",
                    "trusted available<u32>()",
                ),
                (
                    "trusted predicate available(value: u32);",
                    "trusted available(value = 0)",
                ),
            ] {
                let compilation = compilation(&format!(
                    "trusted module app; {declaration} @abi(c) trusted func entry(pos value: RawPointer<u8>) requires({requirement}) {{}}"
                ));

                let key = crate::test_support::source_function_body_key(&compilation, "entry");

                assert!(
                    !super::runtime_source_contract_matches(
                        &compilation,
                        role,
                        &bray_ir::MirUnitKey::Bound(key),
                        &CancellationToken::new()
                    )
                    .unwrap(),
                    "{declaration}"
                );
            }

            for target in bray_target::NativeTarget::ALL {
                let compilation =
                    crate::compilation::trusted_contracts::production_host_compilation_for_target(
                        "module app;",
                        crate::SelectedTarget::for_native(target),
                    );

                let key = crate::test_support::source_function_body_key(&compilation, function);

                assert!(
                    super::runtime_source_contract_matches(
                        &compilation,
                        role,
                        &bray_ir::MirUnitKey::Bound(key),
                        &CancellationToken::new()
                    )
                    .unwrap(),
                    "{target:?}"
                );
            }
        }
    }

    #[test]
    fn source_initialization_requires_native_abi_and_pointer_width_parameters() {
        let compilation = compilation("module app;\n");

        let usize = compilation
            .codegen_representation_type(RepresentationRole::ScalarUsize)
            .unwrap();

        let u32 = compilation
            .codegen_representation_type(RepresentationRole::ScalarU32)
            .unwrap();

        for (abi, parameter, context, expected) in [
            (CallableAbi::C, usize, true, true),
            (CallableAbi::C, usize, false, false),
            (CallableAbi::C, u32, true, false),
            (CallableAbi::Bray, usize, true, false),
        ] {
            let signature = CodegenCallableSignature::new(
                [
                    CodegenParameterMapping::direct(parameter, None, []),
                    CodegenParameterMapping::direct(parameter, None, []),
                ],
                CodegenResultMapping::direct(u32, None, []),
                abi,
                false,
            );

            let signature = if context {
                signature.with_panic_report_context()
            } else {
                signature
            };

            assert_eq!(
                runtime_source_signature_matches(
                    &compilation,
                    RuntimeAbiRole::RuntimeInitialization,
                    &signature,
                    &CancellationToken::new(),
                )
                .unwrap(),
                Some(expected),
            );
        }
    }

    #[test]
    fn record_roles_accept_typed_raw_pointers_and_require_void_results() {
        let compilation = compilation("module app;\n");

        let usize = compilation
            .codegen_representation_type(RepresentationRole::ScalarUsize)
            .unwrap();

        let pointer = compilation
            .available_compiler_known_symbols()
            .unary_representation_type(
                compilation.semantic_value_store().unwrap(),
                RepresentationRole::RawPointer,
                usize,
            )
            .unwrap()
            .unwrap();

        for (parameter, result, expected) in [
            (pointer, CodegenResultMapping::Void, true),
            (usize, CodegenResultMapping::Void, false),
            (
                pointer,
                CodegenResultMapping::direct(usize, None, []),
                false,
            ),
        ] {
            let signature = CodegenCallableSignature::new(
                [
                    CodegenParameterMapping::direct(parameter, None, []),
                    CodegenParameterMapping::direct(pointer, None, []),
                ],
                result,
                CallableAbi::C,
                false,
            );

            assert_eq!(
                runtime_source_signature_matches(
                    &compilation,
                    RuntimeAbiRole::ReportRecordPop,
                    &signature,
                    &CancellationToken::new(),
                )
                .unwrap(),
                Some(expected)
            );
        }
    }

    #[test]
    fn production_bootstrap_operations_and_layouts_cover_every_target() {
        for target in bray_target::NativeTarget::ALL {
            let compilation =
                crate::compilation::trusted_contracts::production_host_compilation_for_target(
                    "module app;",
                    crate::SelectedTarget::for_native(target),
                );

            for name in [
                "initialize_host",
                "shutdown_host",
                "thread_attachment_enter",
                "thread_attachment_leave",
                "thread_storage_exit",
                "attachment_destructor_address",
                "finish_native_attachment",
            ] {
                let checked = compilation
                    .lowered_unit(crate::test_support::source_function_body_key(
                        &compilation,
                        name,
                    ))
                    .unwrap();

                assert!(
                    !checked.diagnostics().has_errors(),
                    "{target:?}, {name}: {checked:#?}"
                );
            }

            let values = compilation.semantic_value_store().unwrap();
            let symbols = compilation.symbol_graph().unwrap();

            let types = symbols
                .structures()
                .iter()
                .filter(|symbol| {
                    symbols.member_name(symbol.id().into()).is_some_and(|name| {
                        [
                            "HostOwner",
                            "HostStorage",
                            "Attachment",
                            "ExitScope",
                            "Synchronization",
                            "HostLease",
                            "HostAnchor",
                            "AttachmentExit",
                        ]
                        .contains(&name.as_str())
                    })
                })
                .map(|symbol| {
                    crate::compilation::substitution::named_type(
                        values,
                        bray_symbols::NamedTypeSymbolId::Struct(symbol.id()),
                    )
                    .unwrap()
                })
                .collect::<std::collections::BTreeSet<_>>();

            assert_eq!(types.len(), 8);

            let target = compilation
                .selected_target()
                .target()
                .codegen_target()
                .unwrap();

            compilation
                .codegen_types(types, None, &target, &CancellationToken::new())
                .unwrap();
        }
    }

    #[test]
    fn thread_destructors_require_the_selected_native_callback_abi() {
        for target in bray_target::NativeTarget::ALL {
            let compilation = crate::test_support::compilation_with_options(
                "module app;\n",
                crate::CompilationOptions::new(
                    crate::WorkerBudget::serial(),
                    bray_symbols::ProductKind::Library,
                    crate::SelectedTarget::for_native(target),
                ),
            );

            let byte = compilation
                .codegen_representation_type(RepresentationRole::ScalarU8)
                .unwrap();

            let pointer = compilation
                .available_compiler_known_symbols()
                .unary_representation_type(
                    compilation.semantic_value_store().unwrap(),
                    RepresentationRole::RawPointer,
                    byte,
                )
                .unwrap()
                .unwrap();

            let expected = if target.as_str().contains("windows") {
                CallableAbi::System
            } else {
                CallableAbi::C
            };

            for abi in [CallableAbi::C, CallableAbi::System, CallableAbi::Bray] {
                let signature = CodegenCallableSignature::new(
                    [CodegenParameterMapping::direct(pointer, None, [])],
                    CodegenResultMapping::Void,
                    abi,
                    false,
                )
                .with_panic_report_context();

                assert_eq!(
                    runtime_source_signature_matches(
                        &compilation,
                        RuntimeAbiRole::ThreadStorageExit,
                        &signature,
                        &CancellationToken::new(),
                    )
                    .unwrap(),
                    Some(abi == expected),
                    "{target:?}, {abi:?}",
                );
            }
        }
    }
}
