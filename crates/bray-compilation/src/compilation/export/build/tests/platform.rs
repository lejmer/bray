use bray_diagnostics::DiagnosticKind;

use super::fixtures::{execution_consumer, standard_library_compilation_with_options};

const THREAD_IDENTITIES: [(&str, &str, &str, &str); 2] = [
    ("std.os.linux", "pthread_self", "pthread_self", "u64"),
    ("std.os.windows", "get_current_thread_id", "GetCurrentThreadId", "u32"),
];

fn observer_source(module: &str, name: &str, symbol: &str, result: &str) -> String {
    format!(r#"
        trusted module {module};

        @link(name = "observer", kind = system)
        @symbol(name = "{symbol}")
        @abi(c)
        extern trusted func {name}() -> {result}
            uses(foreign_call);

        struct State
        {{
            mut registration: usize?;
        }}

        trusted func observe(pos state: &mut State)
            requires(state.registration matches ?_)
            ensures(state.registration matches ?_)
            uses(foreign_call)
        {{
            _ = trusted {name}();
        }}
    "#)
}

fn observer_compilation(sources: impl IntoIterator<Item = impl AsRef<str>>) -> crate::Compilation {
    standard_library_compilation_with_options(sources, crate::CompilationOptions::default().with_native_link_inputs([
        bray_symbols::NativeLinkRequirement::new(
            bray_base::NonEmptySharedStr::try_new("observer").expect("fixture native library name is nonempty"),
            bray_symbols::NativeLinkKind::System,
        ),
    ]))
}

#[test]
fn platform_thread_observers_preserve_storage_in_source_and_imported_contracts() {
    for (module, name, symbol, result) in THREAD_IDENTITIES {
        let source = observer_source(module, name, symbol, result);
        let provider = observer_compilation([&source]);

        assert!(!provider.check_diagnostics().has_errors(), "{:#?}", provider.check_diagnostics());

        let summary = crate::compilation::result_dependencies::source_function_dependencies(&provider, "observe");

        assert!(summary.effects().is_empty(), "native identity observation leaves storage unchanged: {summary:#?}");

        let caller = format!(r#"
            trusted module app;

            using {module};

            trusted func caller(pos state: &mut {module}.State)
                requires(state.registration matches ?_)
                ensures(state.registration matches ?_)
            {{
                trusted {module}.observe(state);
            }}
        "#);

        for imported in [false, true] {
            let application = if imported {
                execution_consumer(&provider, &caller)
            } else {
                observer_compilation([source.as_str(), caller.as_str()])
            };

            assert!(!application.check_diagnostics().has_errors(), "{module}, imported={imported}: {:#?}", application.check_diagnostics());
        }
    }
}

#[test]
fn platform_thread_observer_contracts_grant_neither_purity_nor_name_based_authority() {
    for (module, name, symbol, result) in THREAD_IDENTITIES {
        let ordinary = observer_compilation([observer_source("std.os.ordinary", name, symbol, result)]);

        bray_testing::assert_goal_state_diagnostic_outcome(ordinary.check_diagnostics(), Some(DiagnosticKind::CheckingPostconditionNotProven), name);

        let source = observer_source(module, name, symbol, result);
        let provider = observer_compilation([&source]);
        let caller = format!(r#"
            trusted module app;

            using {module};

            trusted func caller(pos state: &mut {module}.State)
                requires(state.registration matches ?_)
                executes(pure)
            {{
                trusted {module}.observe(state);
            }}
        "#);

        for imported in [false, true] {
            let application = if imported {
                execution_consumer(&provider, &caller)
            } else {
                observer_compilation([source.as_str(), caller.as_str()])
            };

            bray_testing::assert_goal_state_diagnostic_outcome(application.check_diagnostics(), Some(DiagnosticKind::CheckingExecutionGuaranteeNotProven), name);
        }
    }
}
