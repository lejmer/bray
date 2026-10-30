use crate::SelectedTarget;
use crate::compilation::trusted_contracts::production_host_compilation_for_target;
use crate::test_support::assert_boolean_constants;

#[test]
#[ignore = "tier:exhaustive"]
fn constant_layout_queries_use_physical_layout_on_every_native_target() {
    for target in bray_target::NativeTarget::ALL {
        let compilation = production_host_compilation_for_target(
            include_str!("../../../../../xtask/fixtures/native-execution/sync-layout.bray"),
            SelectedTarget::for_native(target),
        );

        assert_boolean_constants(
            &compilation,
            &[
                ("SCALARS", true),
                ("AGGREGATES", true),
                ("FIXED", true),
                ("FLEXIBLE", true),
            ],
        );
    }
}

#[test]
fn constant_layout_queries_reject_invalid_fixed_and_trailing_types() {
    for (source, expected) in [
        (
            "const BAD: usize = std.memory.size_of<[u8]>();",
            bray_diagnostics::DiagnosticKind::CheckingFixedLayoutQueryTypeUnsupported,
        ),
        (
            "const BAD: Result<std.memory.MemoryLayout, std.memory.MemoryLayoutError> = std.memory.layout_of<[u8]>(count = 1);",
            bray_diagnostics::DiagnosticKind::CheckingFixedLayoutQueryTypeUnsupported,
        ),
        (
            "const BAD: Result<std.memory.MemoryLayout, std.memory.MemoryLayoutError> = std.memory.trailing_layout_of<u64>(count = 1);",
            bray_diagnostics::DiagnosticKind::CheckingTrailingLayoutQueryTypeUnsupported,
        ),
        (
            "const BAD: usize = std.memory.size_of<[u64; 18446744073709551615]>();",
            bray_diagnostics::DiagnosticKind::CheckingTypeLayoutOverflow,
        ),
    ] {
        let compilation = production_host_compilation_for_target(
            &format!("module app; using std.memory; {source}"),
            SelectedTarget::baseline(),
        );

        let evaluated = compilation
            .constant_instance(named_constant_key(&compilation, "BAD"))
            .expect("invalid layout constant produces diagnostics");

        let diagnostics = evaluated.diagnostics();

        assert!(
            diagnostics.by_kind(expected).next().is_some(),
            "{source}: {diagnostics:?}"
        );

        if expected == bray_diagnostics::DiagnosticKind::CheckingTypeLayoutOverflow {
            let overflow = diagnostics
                .by_kind(expected)
                .next()
                .expect("layout overflow");

            assert!(overflow.primary_span().is_some());

            for name in [
                bray_diagnostics::DiagnosticArgName::ActualType,
                bray_diagnostics::DiagnosticArgName::TargetTriple,
            ] {
                assert!(overflow.args().iter().any(|arg| arg.name() == name));
            }
        }
    }
}

#[test]
fn constant_layout_results_charge_their_materialized_aggregate_elements() {
    let compilation = production_host_compilation_for_target(
        r#"
module app;
using std.memory;

const SUCCESS: Result<std.memory.MemoryLayout, std.memory.MemoryLayoutError> =
    std.memory.layout_of<u64>(count = 1);
const FAILURE: Result<std.memory.MemoryLayout, std.memory.MemoryLayoutError> =
    std.memory.layout_of<u64>(count = 18446744073709551615);
const SIZE: usize = std.memory.size_of<u64>();
"#,
        SelectedTarget::baseline(),
    );

    for (name, rejected) in [("SUCCESS", true), ("FAILURE", false), ("SIZE", false)] {
        let key = named_constant_key(&compilation, name);

        let evaluated = compilation
            .constant_instance_with_limits(
                key,
                bray_checker::ConstantEvaluationLimits::new(10_000, 2, 10_000),
                &crate::CancellationToken::new(),
            )
            .expect("limited layout evaluation completes");

        assert_eq!(
            evaluated
                .diagnostics()
                .by_kind(bray_diagnostics::DiagnosticKind::CheckingConstantAggregateLimitExceeded)
                .next()
                .is_some(),
            rejected,
            "{name}: {:?}",
            evaluated.diagnostics()
        );
    }
}

fn named_constant_key(
    compilation: &crate::Compilation,
    name: &str,
) -> bray_symbols::ConstantInstanceKey {
    let symbols = compilation.symbol_graph().expect("layout symbols");
    let values = compilation.semantic_value_store().expect("layout values");

    let symbol = symbols
        .constants()
        .iter()
        .find(|symbol| {
            symbols
                .member_name(symbol.id().into())
                .is_some_and(|actual| actual.as_str() == name)
        })
        .expect("named layout constant");

    let definition = bray_symbols::AnyConstantDefinitionId::Constant(symbol.id());

    let substitution =
        crate::compilation::substitution::empty_substitution(values, definition.into_any())
            .expect("closed layout constant");

    bray_symbols::ConstantInstanceKey::new(definition, substitution, None)
}
