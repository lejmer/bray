use bray_bound_tree::BoundUnitKind;
use bray_ir::{MirOperand, MirOperationKind, MirTerminatorKind};
use bray_lowering::LoweredUnit;
use bray_symbols::ConstantValueKind;

use super::support::{declared_unit_key, lowered_mir};
use crate::test_support::{compilation, source_callable_body_key, source_function_body_key};

const UNIT_ROOT_LOWERING_SOURCE: &str = concat!(
    "module app;\n",
    "\n",
    "const value: i32 = 1;\n",
    "\n",
    "func defaults(value: i32 = 1)\n",
    "{\n",
    "}\n",
    "\n",
    "func main()\n",
    "{\n",
    "    lambda() -> i32\n",
    "    {\n",
    "        return 1;\n",
    "    };\n",
    "}\n",
);

const CONSTANT_REFERENCE_LOWERING_SOURCE: &str = concat!(
    "module app;\n",
    "\n",
    "const value: i32 = 7;\n",
    "\n",
    "func main() -> i32\n",
    "{\n",
    "    return value;\n",
    "}\n",
);

#[test]
fn negative_integer_literals_publish_representable_mir_constants() {
    let compilation = compilation(
        r#"
            module app;

            func minimum() -> i8
            {
                return -128;
            }

            func ordinary() -> i8
            {
                return -42;
            }

            func zero() -> u8
            {
                return -0;
            }
        "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let values = compilation.semantic_value_store().unwrap();

    for (name, sign, magnitude) in [
        ("minimum", bray_symbols::IntegerSign::Negative, vec![128]),
        ("ordinary", bray_symbols::IntegerSign::Negative, vec![42]),
        ("zero", bray_symbols::IntegerSign::NonNegative, vec![]),
    ] {
        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, name))
            .unwrap();

        let mir = lowered_mir(&lowered);

        let value = mir
            .blocks()
            .iter()
            .find_map(|block| match block.terminator().kind() {
                MirTerminatorKind::Return(Some(MirOperand::Constant { value, .. })) => Some(*value),
                _ => None,
            })
            .expect("negative literal must lower directly to a typed constant");

        assert_eq!(
            values.constant_value_data(value).kind(),
            &ConstantValueKind::Integer(bray_symbols::IntegerConstant::new(sign, magnitude))
        );

        assert!(
            !mir.operations()
                .iter()
                .any(|operation| matches!(operation.kind(), MirOperationKind::Unary { .. }))
        );
    }
}

#[test]
fn compile_time_units_are_classified_without_demanding_runtime_queries() {
    let compilation = compilation(UNIT_ROOT_LOWERING_SOURCE);
    let key = declared_unit_key(&compilation, BoundUnitKind::ConstantTemplate);

    assert_eq!(
        compilation.state.checked_control_flow.is_published(&key),
        Ok(false)
    );

    assert_eq!(
        compilation.state.expression_semantics.is_published(&key),
        Ok(false)
    );

    let result = compilation
        .lowered_unit(key.clone())
        .unwrap_or_else(|error| panic!("compile-time classification must publish: {error:?}"));

    let Some(LoweredUnit::CompileTime(unit)) = result.value() else {
        panic!("constant templates must be classified as compile-time-only");
    };

    assert_eq!(unit.key(), &key);

    assert_eq!(
        compilation.state.checked_control_flow.is_published(&key),
        Ok(false)
    );

    assert_eq!(
        compilation.state.expression_semantics.is_published(&key),
        Ok(false)
    );
}

#[test]
fn runtime_default_expression_roots_lower_to_mir() {
    let compilation = compilation(UNIT_ROOT_LOWERING_SOURCE);
    let key = declared_unit_key(&compilation, BoundUnitKind::RuntimeDefault);

    let result = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("runtime default MIR must publish: {error:?}"));

    assert!(matches!(
        lowered_mir(&result)
            .blocks()
            .last()
            .map(|block| block.terminator().kind()),
        Some(MirTerminatorKind::Return(Some(MirOperand::Constant { .. })))
    ));
}

#[test]
fn constant_references_lower_to_closed_mir_operands() {
    let compilation = compilation(CONSTANT_REFERENCE_LOWERING_SOURCE);
    let key = source_callable_body_key(&compilation);

    let result = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("constant reference MIR must publish: {error:?}"));

    assert!(matches!(
        lowered_mir(&result)
            .blocks()
            .last()
            .map(|block| block.terminator().kind()),
        Some(MirTerminatorKind::Return(Some(MirOperand::Constant { .. })))
    ));
}

#[test]
fn anonymous_callable_values_and_nested_bodies_lower_independently() {
    let compilation = compilation(UNIT_ROOT_LOWERING_SOURCE);

    let (outer, nested) = compilation
        .declared_unit_keys_for_test()
        .unwrap_or_else(|error| panic!("declared units must be available: {error:?}"))
        .into_iter()
        .filter(|key| key.kind() == BoundUnitKind::CallableBody)
        .find_map(|key| {
            let unit = compilation.bound_unit(key.clone()).ok()?;
            let nested = unit.value().nested_units().first()?.clone();

            Some((key, nested))
        })
        .unwrap_or_else(|| panic!("test source must contain one nested anonymous callable"));

    let outer_result = compilation
        .lowered_unit(outer)
        .unwrap_or_else(|error| panic!("outer callable MIR must publish: {error:?}"));

    assert!(
        lowered_mir(&outer_result)
            .operations()
            .iter()
            .any(|operation| matches!(
                operation.kind(),
                MirOperationKind::AnonymousCallable(
                    bray_ir::MirAnonymousCallableReference::Bound(key)
                ) if key == &nested
            ))
    );

    let nested_result = compilation
        .lowered_unit(nested.clone())
        .unwrap_or_else(|error| panic!("nested callable MIR must publish: {error:?}"));

    assert!(matches!(
        lowered_mir(&nested_result).key(),
        bray_ir::MirUnitKey::Bound(key) if key == &nested
    ));
}
