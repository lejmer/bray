use bray_diagnostics::DiagnosticKind;
use bray_symbols::ConstantValueKind;

use super::fixtures::{execution_consumer, source_constant_value, standard_library_compilation};

const NUMERIC_SOURCES: [&str; 3] = [
    include_str!("../../../../../../../standard-library/std/src/std.bray"),
    include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
    include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
];

#[test]
fn numeric_truncation_values_agree_in_source_and_imported_constants() {
    let provider = standard_library_compilation(NUMERIC_SOURCES);

    for (source_type, target_type, value, expected) in [
        ("u16", "u8", "257", "1"),
        ("u16", "i8", "255", "-1"),
        ("i16", "u8", "-1", "255"),
        ("r64", "i8", "-128.75", "std.numeric.I8_MIN"),
        ("r64", "u8", "-0.75", "0"),
        ("r32", "i32", "1.75", "1"),
        ("r32", "u32", "65504.0", "65504"),
        ("r32", "r64", "1.5", "1.5"),
        ("i32", "r64", "2147483647", "2147483647.0"),
        ("u32", "r64", "42", "42.0"),
    ] {
        let source = format!(
            r#"
            module app;

            using std.numeric;

            const INPUT: {source_type} = {value};
            const VALUE: {target_type} = std.numeric.truncate_to<{target_type}>(INPUT);
            const CORRECT: bool = VALUE == {expected};
        "#
        );

        let local =
            standard_library_compilation(NUMERIC_SOURCES.into_iter().chain([source.as_str()]));

        let imported = execution_consumer(&provider, &source);

        for compilation in [&local, &imported] {
            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{source}: {:#?}",
                compilation.check_diagnostics()
            );

            assert_eq!(
                source_constant_value(compilation, "CORRECT").kind(),
                &ConstantValueKind::Boolean(true)
            );
        }
    }
}

#[test]
fn numeric_truncation_rejects_invalid_constant_domains_with_requirement_context() {
    let provider = standard_library_compilation(NUMERIC_SOURCES);

    for (target_type, value) in [
        ("i8", "-129.0"),
        ("i8", "128.0"),
        ("u8", "-1.0"),
        ("u8", "256.0"),
    ] {
        let source = format!(
            r#"
            module app;

            using std.numeric;

            const INPUT: r64 = {value};
            const VALUE: {target_type} = std.numeric.truncate_to<{target_type}>(INPUT);
        "#
        );

        let local =
            standard_library_compilation(NUMERIC_SOURCES.into_iter().chain([source.as_str()]));

        let imported = execution_consumer(&provider, &source);

        for compilation in [&local, &imported] {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingConstantCallRequirementNotProven,
            );

            let diagnostic = compilation
                .check_diagnostics()
                .by_kind(DiagnosticKind::CheckingConstantCallRequirementNotProven)
                .next()
                .expect("invalid truncation reports its precondition");

            assert!(diagnostic.primary_span().is_some(), "{diagnostic:#?}");
        }
    }
}

#[test]
fn numeric_truncation_rejects_unsupported_types_before_lowering() {
    let provider = standard_library_compilation(NUMERIC_SOURCES);

    for (source_type, target_type) in [
        ("bool", "u8"),
        ("char", "u8"),
        ("u8", "bool"),
        ("r64", "r32"),
        ("i64", "r32"),
        ("u128", "r64"),
        ("c64", "r64"),
        ("[u8; 1]", "u8"),
    ] {
        let source = format!(
            r#"
            module app;

            using std.numeric;

            func invalid(pos value: {source_type}) -> {target_type}
            {{
                return std.numeric.truncate_to<{target_type}>(value);
            }}
        "#
        );

        let local =
            standard_library_compilation(NUMERIC_SOURCES.into_iter().chain([source.as_str()]));

        let imported = execution_consumer(&provider, &source);

        for compilation in [&local, &imported] {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingNoApplicableCandidate,
            );
        }
    }
}

#[test]
fn numeric_truncation_preserves_callable_value_and_predicate_contracts() {
    let source = r#"
        module app;

        using std.numeric;

        callable Narrow = const func(pos source: r64) -> i8
            requires(source > -129.0 && source < 128.0);

        callable QueriedNarrow = const func(pos source: r64) -> i8
            requires(std.numeric.in_truncation_domain<i8>(source));

        callable IntegerNarrow = const func(pos source: u16) -> u8
            requires(std.numeric.in_truncation_domain<u8>(source));

        const func queried_convert(pos value: r64) -> i8
            requires(value > -129.0, value < 128.0)
        {
            let selected: QueriedNarrow = std.numeric.truncate_to<i8, r64>;

            return selected(value);
        }

        public func runtime_integer(pos value: u16) -> u8
        {
            let selected: IntegerNarrow = std.numeric.truncate_to<u8, u16>;

            return selected(value);
        }

        const func convert(pos value: r64) -> i8
            requires(value > -129.0, value < 128.0)
        {
            let selected: Narrow = std.numeric.truncate_to<i8, r64>;

            return selected(value);
        }

        predicate guarded(value: r64) = if value > -129.0 && value < 128.0
        {
            yield std.numeric.truncate_to<i8>(value) >= std.numeric.I8_MIN;
        }
        else
        {
            yield true;
        };

        const VALUE: i8 = convert(-128.75);
        const CORRECT: bool = VALUE == std.numeric.I8_MIN && queried_convert(-128.75) == VALUE;

        public func runtime(pos value: r64) -> i8
        {
            let selected: Narrow = std.numeric.truncate_to<i8, r64>;

            return selected(value);
        }
    "#;

    let provider = standard_library_compilation(NUMERIC_SOURCES);
    let local = standard_library_compilation(NUMERIC_SOURCES.into_iter().chain([source]));
    let imported = execution_consumer(&provider, source);

    for compilation in [&local, &imported] {
        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        assert_eq!(
            source_constant_value(compilation, "CORRECT").kind(),
            &ConstantValueKind::Boolean(true)
        );

        assert!(
            compilation
                .codegen_instance_count_for_test(|key| matches!(
                    key,
                    bray_ir::MirUnitKey::CompilerProvidedCallable(_)
                ))
                .expect("fixture `codegen_instance_count_for_test` call must succeed")
                > 0
        );
    }
}

#[test]
fn numeric_truncation_keeps_unproved_real_domains_out_of_predicates() {
    let provider = standard_library_compilation(NUMERIC_SOURCES);

    let source = r#"
        module app;

        using std.numeric;

        predicate invalid(value: r64) = std.numeric.truncate_to<i8>(value) >= 0;
    "#;

    let local = standard_library_compilation(NUMERIC_SOURCES.into_iter().chain([source]));
    let imported = execution_consumer(&provider, source);

    for compilation in [&local, &imported] {
        assert!(
            compilation.check_diagnostics().has_errors(),
            "unproved real conversion must be rejected"
        );
    }
}

#[test]
fn integer_truncation_has_no_runtime_requirement_helper_before_llvm() {
    let source = r#"
        module app;

        using std.numeric;

        public func narrow(pos value: u128) -> u8
        {
            return std.numeric.truncate_to<u8>(value);
        }
    "#;

    let provider = standard_library_compilation(NUMERIC_SOURCES);
    let local = standard_library_compilation(NUMERIC_SOURCES.into_iter().chain([source]));
    let imported = execution_consumer(&provider, source);

    for compilation in [&local, &imported] {
        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        assert_eq!(
            compilation
                .codegen_instance_count_for_test(|key| matches!(
                    key,
                    bray_ir::MirUnitKey::Predicate(_)
                ))
                .expect("fixture `codegen_instance_count_for_test` call must succeed"),
            0
        );
    }
}

#[test]
fn numeric_truncation_composes_with_open_generic_contracts() {
    let source = r#"
        module app;

        using std.numeric;

        func convert<Target, Source>(pos source: Source) -> Target
            with(Source: Copyable, std.numeric.supports_truncation<Target, Source>())
            requires(std.numeric.in_truncation_domain<Target, Source>(source))
        {
            return std.numeric.truncate_to<Target>(source);
        }

        func checked_convert<Target, Source>(pos source: Source) -> Target
            with(Target: Copyable, Source: Copyable, std.numeric.supports_truncation<Target, Source>())
        {
            return std.numeric.truncate_to<Target>(source);
        }

        func indirect<Target, Source>(pos source: Source) -> Target
            with(Source: Copyable, std.numeric.supports_truncation<Target, Source>())
        {
            let selected: const func(pos source: Source) -> Target
                requires(std.numeric.in_truncation_domain<Target, Source>(source)) =
                std.numeric.truncate_to<Target, Source>;

            return selected(source);
        }

        public func indirect_real(pos value: r64) -> i8
        {
            return indirect<i8, r64>(value);
        }

        public func checked_real(pos value: r64) -> i8
        {
            return checked_convert<i8, r64>(value);
        }

        public func runtime_integer(pos value: u16) -> u8
        {
            return convert<u8, u16>(value);
        }

        public func runtime_real(pos value: r64) -> i8
        {
            return convert<i8, r64>(value);
        }
    "#;

    let provider = standard_library_compilation(NUMERIC_SOURCES);
    let local = standard_library_compilation(NUMERIC_SOURCES.into_iter().chain([source]));
    let imported = execution_consumer(&provider, source);

    for compilation in [&local, &imported] {
        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        assert!(
            compilation
                .codegen_instance_count_for_test(|key| matches!(key, bray_ir::MirUnitKey::Bound(_)))
                .expect("fixture `codegen_instance_count_for_test` call must succeed")
                > 0
        );
    }
}

#[test]
fn numeric_truncation_does_not_invent_a_different_generic_type_domain() {
    let source = r#"
        module app;

        using std.numeric;

        func invalid<Target, Source, Other>(pos source: Source) -> Other
            with(Target: Copyable, Source: Copyable, std.numeric.supports_truncation<Target, Source>())
        {
            return std.numeric.truncate_to<Other>(source);
        }
    "#;

    let provider = standard_library_compilation(NUMERIC_SOURCES);
    let local = standard_library_compilation(NUMERIC_SOURCES.into_iter().chain([source]));
    let imported = execution_consumer(&provider, source);

    for compilation in [&local, &imported] {
        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingNoApplicableCandidate,
        );
    }
}

#[test]
fn numeric_truncation_static_queries_use_type_premises_without_assuming_their_own_predicate() {
    for (premise, valid) in [("T: Copyable,", true), ("", false)] {
        let source = format!(
            r#"
            module app;

            const func supported<T>() -> bool
                with(T: Copyable)
            {{
                return true;
            }}

            func constrained<T>()
                with({premise} supported<T>())
            {{
            }}
        "#
        );

        let compilation = crate::test_support::compilation(&source);

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{source}: {:#?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn numeric_truncation_respects_optional_target_formats() {
    let provider = standard_library_compilation(NUMERIC_SOURCES);

    for source_type in ["r16", "r128"] {
        let source = format!(
            r#"
            module app;

            using std.numeric;

            func unavailable(pos value: {source_type}) -> i32
            {{
                return std.numeric.truncate_to<i32>(value);
            }}
        "#
        );

        let local =
            standard_library_compilation(NUMERIC_SOURCES.into_iter().chain([source.as_str()]));

        let imported = execution_consumer(&provider, &source);

        for compilation in [&local, &imported] {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingTargetRepresentationUnavailable,
            );
        }
    }
}

#[test]
fn numeric_truncation_callable_values_cannot_erase_the_input_domain() {
    let source = r#"
        module app;

        using std.numeric;

        callable Erased = const func(pos source: r64) -> i8;

        func invalid()
        {
            let selected: Erased = std.numeric.truncate_to<i8, r64>;
        }
    "#;

    let provider = standard_library_compilation(NUMERIC_SOURCES);
    let local = standard_library_compilation(NUMERIC_SOURCES.into_iter().chain([source]));
    let imported = execution_consumer(&provider, source);

    for compilation in [&local, &imported] {
        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingIncompatibleExpressionType,
        );
    }
}

#[test]
fn numeric_truncation_query_values_keep_their_static_domains() {
    let provider = standard_library_compilation(NUMERIC_SOURCES);

    for (source_type, valid) in [("r64", true), ("bool", false)] {
        let source = format!(
            r#"
            module app;

            using std.numeric;

            public func query(pos value: {source_type}) -> bool
            {{
                let selected: const func(pos source: {source_type}) -> bool =
                    std.numeric.in_truncation_domain<i8, {source_type}>;

                return selected(value);
            }}

            public func supported() -> bool
            {{
                let selected: const func() -> bool = std.numeric.supports_truncation<i8, {source_type}>;

                return selected();
            }}
        "#
        );

        let local =
            standard_library_compilation(NUMERIC_SOURCES.into_iter().chain([source.as_str()]));

        let imported = execution_consumer(&provider, &source);

        for compilation in [&local, &imported] {
            if valid {
                assert!(
                    !compilation.check_diagnostics().has_errors(),
                    "{:#?}",
                    compilation.check_diagnostics()
                );

                assert!(
                    compilation
                        .codegen_instance_count_for_test(|key| matches!(
                            key,
                            bray_ir::MirUnitKey::CompilerProvidedCallable(_)
                        ))
                        .expect("fixture `codegen_instance_count_for_test` call must succeed")
                        >= 2
                );
            } else {
                bray_testing::assert_goal_state_diagnostic_kind(
                    compilation.check_diagnostics(),
                    DiagnosticKind::CheckingStaticConstraintUnsatisfied,
                );
            }
        }
    }
}

#[test]
fn numeric_truncation_unsupported_support_queries_are_false_without_runtime_helpers() {
    let source = r#"
        module app;

        using std.numeric;

        const SUPPORTED: bool = std.numeric.supports_truncation<i8, bool>();

        public func supported() -> bool
        {
            return std.numeric.supports_truncation<i8, bool>();
        }
    "#;

    let provider = standard_library_compilation(NUMERIC_SOURCES);
    let local = standard_library_compilation(NUMERIC_SOURCES.into_iter().chain([source]));
    let imported = execution_consumer(&provider, source);

    for compilation in [&local, &imported] {
        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        assert_eq!(
            source_constant_value(compilation, "SUPPORTED").kind(),
            &ConstantValueKind::Boolean(false)
        );

        assert_eq!(
            compilation
                .codegen_instance_count_for_test(|key| matches!(
                    key,
                    bray_ir::MirUnitKey::CompilerProvidedCallable(_)
                        | bray_ir::MirUnitKey::Predicate(_)
                ))
                .expect("fixture `codegen_instance_count_for_test` call must succeed"),
            0
        );
    }
}

#[test]
fn numeric_truncation_inherits_predicates_through_type_equalities() {
    let source = r#"
        module app;

        using std.numeric;

        struct Holder<T, U>
        {
            value: U;
        }

        impl Holder<T, U>
            with (T == T, U: Copyable, std.numeric.supports_truncation<usize, U>())
        {
            consume func convert() -> usize
                with (T == U)
            {
                return std.numeric.truncate_to<usize>(self.value);
            }
        }

        public func converted(pos input: u8) -> usize
        {
            let value: Holder<u8, u8> = { value = input };
            return value.convert();
        }
    "#;

    let provider = standard_library_compilation(NUMERIC_SOURCES);
    let local = standard_library_compilation(NUMERIC_SOURCES.into_iter().chain([source]));
    let imported = execution_consumer(&provider, source);

    for compilation in [&local, &imported] {
        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        assert!(
            compilation
                .codegen_instance_count_for_test(|key| matches!(key, bray_ir::MirUnitKey::Bound(_)))
                .expect("fixture `codegen_instance_count_for_test` call must succeed")
                > 0
        );
    }
}
