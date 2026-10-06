use std::sync::Arc;

use bray_package_interface::{
    InterfaceCheckedTemplateOperation, InterfaceValidationLimits, PackageImplementationArtifact,
    encode_package_interface,
};
use bray_symbols::{ConstantValueKind, IntegerConstant};

use super::fixtures::{compilation, execution_consumer, execution_dependency, export};

#[test]
fn indexed_constant_templates_evaluate_after_import() {
    for (expression, start, end, valid) in [
        ("values[1]", 1, 2, true),
        ("values[..][1]", 1, 2, true),
        ("values[1..2][0]", 1, 2, true),
        ("values[start]", 1, 2, true),
        ("values[start..end][0]", 1, 2, true),
        ("values[..end][start]", 1, 2, true),
        ("values[start]", 2, 2, false),
        ("values[start..end][0]", 2, 1, false),
        ("values[start..end][0]", 0, 3, false),
    ] {
        let declaration = format!(
            r#"
            const func selected(pos values: [i32; 2], pos start: usize, pos end: usize) -> i32 {{
                return {expression};
            }}
        "#
        );

        let provider = compilation(&format!("module api;\n{declaration}"));

        assert!(
            !provider.check_diagnostics().has_errors(),
            "{expression}: {:?}",
            provider.check_diagnostics()
        );

        let body = format!("const result: i32 = selected([19, 37], {start}, {end});");
        let source = compilation(&format!("module api;\n{declaration}\n{body}"));
        let bundle = export(&provider);
        let interface = encode_package_interface(bundle).unwrap();

        let implementation = PackageImplementationArtifact::try_from_export_bundle(
            &interface,
            bundle,
            InterfaceValidationLimits::default(),
        )
        .unwrap();

        let dependency = execution_dependency(&provider)
            .with_implementation_artifact("provider.brayimpl", Arc::new(implementation));

        let imported = crate::test_support::compilation_with_dependencies(
            &format!(
                "module app;\nusing example.package.api;\n{}",
                body.replace("selected(", "example.package.api.selected(")
            ),
            [dependency],
        );

        for consumer in [&source, &imported] {
            let diagnostics = consumer.check_diagnostics();

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    &diagnostics,
                    bray_diagnostics::DiagnosticKind::CheckingInvalidConstantExpression,
                );

                continue;
            }

            assert!(!diagnostics.has_errors(), "{expression}: {diagnostics:?}");

            let graph = consumer.symbol_graph().unwrap();

            let definition = graph
                .constants()
                .iter()
                .find(|constant| constant.origin() == bray_symbols::SymbolOrigin::Source)
                .unwrap();

            let definition = bray_symbols::AnyConstantDefinitionId::Constant(definition.id());
            let values = consumer.semantic_value_store().unwrap();

            let substitution =
                crate::compilation::substitution::empty_substitution(values, definition.into_any())
                    .unwrap();

            let result = consumer
                .constant_instance(bray_symbols::ConstantInstanceKey::new(
                    definition,
                    substitution,
                    None,
                ))
                .unwrap();

            let result = values.constant_value_data(result.value().value());

            assert_eq!(
                result.kind(),
                &ConstantValueKind::Integer(IntegerConstant::from_u64(37))
            );
        }
    }
}

#[test]
fn custom_index_templates_preserve_selected_calls_and_capabilities() {
    for (protocol, method, selector, cap, parameters) in [
        ("ElementIndex", "index", "start", "", "pos selector: &i32"),
        (
            "MutableElementIndex",
            "index",
            "start",
            "mut ",
            "pos selector: &i32",
        ),
        (
            "SliceIndex",
            "slice",
            "start..",
            "",
            "pos start: i32?, pos end: i32?",
        ),
        (
            "SliceIndex",
            "slice",
            "..end",
            "",
            "pos start: i32?, pos end: i32?",
        ),
        (
            "MutableSliceIndex",
            "slice",
            "start..end",
            "mut ",
            "pos start: i32?, pos end: i32?",
        ),
    ] {
        let declaration = format!(
            r#"
            struct Item {{ mut value: bool; }}
            struct Values {{ mut value: Item; }}
            impl Values({protocol}<i32>) {{
                type Output = Item;
                {cap}func {method}({parameters}) -> &{cap}Item {{ return &{cap}self.value; }}
            }}
            func selected(pos values: &{cap}Values, pos start: i32, pos end: i32, result: &{cap}Item = &{cap}values[{selector}]) -> &{cap}Item {{ return result; }}
        "#
        );

        let provider = compilation(&format!("module api;\n{declaration}"));

        assert!(
            !provider.check_diagnostics().has_errors(),
            "{protocol}: {:?}",
            provider.check_diagnostics()
        );

        let operations = export(&provider)
            .semantics()
            .checked_templates()
            .iter()
            .flat_map(|template| template.nodes())
            .map(|node| node.operation());

        let call = operations
            .filter_map(|operation| match operation {
                InterfaceCheckedTemplateOperation::Index { call, .. }
                | InterfaceCheckedTemplateOperation::Slice { call, .. } => call.as_ref(),
                _ => None,
            })
            .next()
            .unwrap();

        assert!(matches!(
            call.dispatch,
            bray_bound_tree::CheckedTemplateIndexDispatch::Implementation(..)
        ));

        assert_eq!(
            call.borrow_kind,
            if cap.is_empty() {
                bray_symbols::BorrowKind::Shared
            } else {
                bray_symbols::BorrowKind::Mutable
            }
        );

        let source_body = format!(
            "func check(pos values: &{cap}Values) -> &{cap}Item {{ return selected(values, 0, 1); }}"
        );

        let source = compilation(&format!("module api;\n{declaration}\n{source_body}"));

        let imported_body = source_body
            .replace("Values", "example.package.api.Values")
            .replace("Item", "example.package.api.Item")
            .replace("selected(", "example.package.api.selected(");

        let imported = execution_consumer(
            &provider,
            &format!("module app;\nusing example.package.api;\n{imported_body}"),
        );

        for consumer in [&source, &imported] {
            assert!(
                !consumer.check_diagnostics().has_errors(),
                "{protocol}: {:?}",
                consumer.check_diagnostics()
            );
        }
    }
}

#[test]
fn indexed_declaration_templates_preserve_predicates_contracts_and_statics() {
    let declarations = r#"
        predicate ready(values: [bool; 2]) = values[..][1];
        func selected(pos values: [bool; 2]) -> bool
            requires(values[0])
            ensures(result == values[1])
        { return values[1]; }
        static Selected: i32 = [19, 37][1..][0];
        @thread_local static ThreadSelected: i32 = [19, 37][..][1];
    "#;

    let provider = compilation(&format!("module api;\n{declarations}"));

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    let _ = export(&provider);

    let imported = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api;
        func check() -> bool requires(example.package.api.ready([false, true])) {
            return example.package.api.selected([true, true]);
        }
    "#,
    );

    assert!(
        !imported.check_diagnostics().has_errors(),
        "{:?}",
        imported.check_diagnostics()
    );
}

#[test]
fn constrained_index_defaults_preserve_abstract_dispatch() {
    let provider = compilation(
        r#"
        module api;
        func selected<T>(pos values: &T, pos selector: i32, result: &T(ElementIndex<i32>).Output = &values[selector]) -> &T(ElementIndex<i32>).Output
            with(T: ElementIndex<i32>)
        { return result; }
    "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    assert!(export(&provider).semantics().checked_templates().iter().flat_map(|template| template.nodes()).any(|node| matches!(node.operation(),
        InterfaceCheckedTemplateOperation::Index { call: Some(call), .. }
            if matches!(call.dispatch, bray_bound_tree::CheckedTemplateIndexDispatch::Constraint { .. })
    )));

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api;
        struct Values { value: bool; }
        impl Values(ElementIndex<i32>) {
            type Output = bool;
            func index(pos selector: &i32) -> &bool { return &self.value; }
        }
        func check(pos values: &Values) -> &bool { return example.package.api.selected<Values>(values, 0); }
    "#,
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:?}",
        consumer.check_diagnostics()
    );
}

#[test]
fn custom_indexing_is_rejected_during_constant_evaluation() {
    let source = compilation(
        r#"
        module api;
        struct Values { value: bool; }
        impl Values(ElementIndex<i32>) {
            type Output = bool;
            func index(pos selector: &i32) -> &bool { return &self.value; }
        }
        const func selected(pos values: Values, pos selector: i32) -> bool { return values[selector]; }
        const result: bool = selected(Values { value = true }, 0);
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        &source.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingInvalidConstantExpression,
    );
}
