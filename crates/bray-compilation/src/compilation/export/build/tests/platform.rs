use bray_diagnostics::DiagnosticKind;

use super::fixtures::{execution_consumer, standard_library_compilation_with_options};

const PLATFORM_STATE_CALLS: [(&str, &str, &str, &str, &str, &str); 6] = [
    ("std.os.linux", "pthread_self", "pthread_self", "", "u64", ""),
    ("std.os.linux", "pthread_getspecific", "pthread_getspecific", "pos key: u32", "DataPointer<u8>", "0"),
    ("std.os.windows", "get_current_thread_id", "GetCurrentThreadId", "", "u32", ""),
    ("std.os.windows", "fls_get_value", "FlsGetValue", "pos key: u32", "DataPointer<u8>", "0"),
    ("std.os.windows", "get_last_error", "GetLastError", "", "u32", ""),
    ("std.os.windows", "set_last_error", "SetLastError", "pos code: u32", "unit", "0"),
];

fn platform_state_source(module: &str, name: &str, symbol: &str, parameters: &str, result: &str, arguments: &str) -> String {
    format!(r#"
        trusted module {module};

        using core.memory;

        @link(name = "observer", kind = system)
        @symbol(name = "{symbol}")
        @abi(c)
        extern trusted func {name}({parameters}) -> {result}
            uses(foreign_call);

        struct State
        {{
            mut registration: usize?;
        }}

        trusted func call_platform(pos state: &mut State)
            requires(state.registration matches ?_)
            ensures(state.registration matches ?_)
            uses(foreign_call)
        {{
            _ = trusted {name}({arguments});
        }}
    "#)
}

fn platform_state_compilation(sources: impl IntoIterator<Item = impl AsRef<str>>) -> crate::Compilation {
    standard_library_compilation_with_options(sources, crate::CompilationOptions::default().with_native_link_inputs([
        bray_symbols::NativeLinkRequirement::new(
            bray_base::NonEmptySharedStr::try_new("observer").expect("fixture native library name is nonempty"),
            bray_symbols::NativeLinkKind::System,
        ),
    ]))
}

#[test]
fn platform_local_state_preserves_storage_in_source_and_imported_contracts() {
    for (module, name, symbol, parameters, result, arguments) in PLATFORM_STATE_CALLS {
        let source = platform_state_source(module, name, symbol, parameters, result, arguments);
        let provider = platform_state_compilation([&source]);

        assert!(!provider.check_diagnostics().has_errors(), "{:#?}", provider.check_diagnostics());

        let summary = crate::compilation::result_dependencies::source_function_dependencies(&provider, "call_platform");

        assert!(summary.effects().is_empty(), "native local state access leaves Bray storage unchanged: {summary:#?}");

        let caller = format!(r#"
            trusted module app;

            using {module};

            trusted func caller(pos state: &mut {module}.State)
                requires(state.registration matches ?_)
                ensures(state.registration matches ?_)
            {{
                trusted {module}.call_platform(state);
            }}
        "#);

        for imported in [false, true] {
            let application = if imported {
                execution_consumer(&provider, &caller)
            } else {
                platform_state_compilation([source.as_str(), caller.as_str()])
            };

            assert!(!application.check_diagnostics().has_errors(), "{module}, imported={imported}: {:#?}", application.check_diagnostics());
        }
    }
}

#[test]
fn platform_local_state_contracts_grant_neither_purity_nor_name_based_authority() {
    for (module, name, symbol, parameters, result, arguments) in PLATFORM_STATE_CALLS {
        let ordinary = platform_state_compilation([platform_state_source("std.os.ordinary", name, symbol, parameters, result, arguments)]);

        bray_testing::assert_goal_state_diagnostic_outcome(ordinary.check_diagnostics(), Some(DiagnosticKind::CheckingPostconditionNotProven), name);

        let source = platform_state_source(module, name, symbol, parameters, result, arguments);
        let provider = platform_state_compilation([&source]);

        let caller = format!(r#"
            trusted module app;

            using {module};

            trusted func caller(pos state: &mut {module}.State)
                requires(state.registration matches ?_)
                executes(pure)
            {{
                trusted {module}.call_platform(state);
            }}
        "#);

        for imported in [false, true] {
            let application = if imported {
                execution_consumer(&provider, &caller)
            } else {
                platform_state_compilation([source.as_str(), caller.as_str()])
            };

            bray_testing::assert_goal_state_diagnostic_outcome(application.check_diagnostics(), Some(DiagnosticKind::CheckingExecutionGuaranteeNotProven), name);
        }
    }
}
