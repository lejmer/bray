use bray_ir::{MirOperand, MirOperationKind, MirProjectionKind, MirTerminatorKind};
use bray_symbols::{BorrowKind, ConstantValueKind, TypeData};

use super::support::lowered_mir;
use crate::test_support::{
    compilation, source_callable_body_key, source_function_body_key,
    source_trait_callable_fulfillment_body_key,
};

const BORROW_LOWERING_SOURCE: &str = concat!(
    "module app;\n",
    "\n",
    "func main()\n",
    "{\n",
    "    let number: i32 = 1;\n",
    "    let reference: &i32 = &number;\n",
    "}\n",
);

#[test]
fn borrowed_atomic_lock_loop_lowers() {
    let compilation = compilation(
        r#"module app;

using core.atomic;

trusted func wait(pos lock: &core.atomic.Atomic<u32>, pos expected: u32)
{
}

trusted func acquire(pos lock: &core.atomic.Atomic<u32>) -> bool
{
    let (_, acquired) = core.atomic.compare_exchange<u32, 1, 0>(lock, expected = 0, desired = 1);

    if acquired
    {
        return true;
    }

    let mut observed: u32 = core.atomic.exchange<u32, 1>(lock, value = 2);

    while observed != 0
    {
        trusted wait(lock, expected = 2);
        observed = core.atomic.exchange<u32, 1>(lock, value = 2);
    }

    return true;
}
"#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let key = source_function_body_key(&compilation, "acquire");

    let lowered = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("borrowed atomic lock loop must lower: {error:?}"));

    assert!(
        lowered.diagnostics().is_empty(),
        "{:#?}",
        lowered.diagnostics()
    );

    assert!(lowered.value().is_some());
}

#[test]
fn shared_string_literal_borrows_lower_as_static_constants() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func text(pos selected: bool) -> &string\n",
        "{\n",
        "    if selected\n",
        "    {\n",
        "        return &\"static text\";\n",
        "    }\n",
        "\n",
        "    return &\"other text\";\n",
        "}\n",
    ));

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "text"))
        .unwrap_or_else(|error| panic!("borrowed literal must lower: {error:?}"));

    assert!(
        lowered.diagnostics().is_empty(),
        "{:#?}",
        lowered.diagnostics()
    );

    let values = compilation
        .semantic_value_store()
        .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

    let constants = lowered_mir(&lowered)
        .operations()
        .iter()
        .filter_map(|operation| {
            let MirOperationKind::Store {
                value: MirOperand::Constant { value, ty },
                destination,
                ..
            } = operation.kind()
            else {
                return None;
            };

            matches!(
                lowered_mir(&lowered)
                    .storage(destination.storage())
                    .expect("fixture expression must retain its checked type or storage")
                    .kind(),
                bray_ir::MirStorageKind::Return
            )
            .then_some((*value, *ty))
        })
        .collect::<Vec<_>>();

    assert_eq!(constants.len(), 2);

    for (value, ty) in constants {
        let ty = values.type_data(ty);

        let value = values.constant_value_data(value);

        let TypeData::Borrow {
            kind: BorrowKind::Shared,
            target,
        } = ty.as_ref()
        else {
            panic!("literal must use a shared borrow representation");
        };

        assert_eq!(value.ty(), *target);
        assert!(matches!(value.kind(), ConstantValueKind::String(_)));
    }
}

#[test]
fn shared_nonliteral_borrows_continue_through_storage() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func accept(pos text: &string)\n",
        "{\n",
        "}\n",
        "\n",
        "func forward(pos text: string)\n",
        "{\n",
        "    accept(&text);\n",
        "}\n",
    ));

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "forward"))
        .unwrap_or_else(|error| panic!("nonliteral borrow must lower: {error:?}"));

    assert!(
        lowered.diagnostics().is_empty(),
        "{:#?}",
        lowered.diagnostics()
    );

    assert!(lowered_mir(&lowered).operations().iter().any(|operation| {
        matches!(
            operation.kind(),
            MirOperationKind::Borrow {
                kind: BorrowKind::Shared,
                ..
            }
        )
    }));
}

#[test]
fn checked_borrows_lower_to_explicit_borrow_operations() {
    let compilation = compilation(BORROW_LOWERING_SOURCE);
    let key = source_callable_body_key(&compilation);

    let result = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("borrow MIR must be available: {error:?}"));

    let mir = lowered_mir(&result);

    assert!(mir.operations().iter().any(|operation| matches!(
        operation.kind(),
        MirOperationKind::Borrow {
            kind: BorrowKind::Shared,
            ..
        }
    )));
}

#[test]
fn shared_receiver_storage_is_read_through_its_borrowed_representation() {
    let compilation = compilation(
        r#"module app;

trait Read
{
    func read() -> i32;
}

impl I32Read = i32(Read)
{
    func read() -> i32
    {
        return self;
    }
}
"#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let lowered = compilation
        .lowered_unit(source_trait_callable_fulfillment_body_key(
            &compilation,
            "read",
        ))
        .unwrap_or_else(|error| panic!("shared receiver body must lower: {error:?}"));

    let mir = lowered_mir(&lowered);

    assert!(
        mir.blocks().iter().any(|block| matches!(
            block.terminator().kind(),
            MirTerminatorKind::Return { value: Some(MirOperand::Copy(place)), report: None }
                if matches!(
                    place.projections().first().map(bray_ir::MirProjection::kind),
                    Some(MirProjectionKind::Dereference)
                )
        )),
        "{mir:#?}"
    );
}

#[test]
fn nested_borrowed_fields_insert_each_implicit_dereference() {
    let compilation = compilation(
        r#"module app;

struct Value
{
    number: i32;
}

struct Owner
{
    value: &Value;
}

func read(pos owner: &Owner) -> i32
{
    return owner.value.number;
}
"#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "read"))
        .unwrap_or_else(|error| panic!("nested borrowed fields must lower: {error:?}"));

    let mir = lowered_mir(&lowered);

    let projections = mir
        .blocks()
        .iter()
        .find_map(|block| match block.terminator().kind() {
            MirTerminatorKind::BeginCleanup(cleanup)
            | MirTerminatorKind::ContinueCleanup(cleanup) => cleanup
                .edge()
                .arguments()
                .iter()
                .find_map(|argument| match argument {
                    MirOperand::Copy(place) => Some(place.projections()),
                    _ => None,
                }),
            MirTerminatorKind::Return {
                value: Some(MirOperand::Copy(place)),
                report: None,
            } => Some(place.projections()),
            _ => None,
        });

    let Some(projections) = projections else {
        panic!("nested field read must return from a place: {mir:#?}");
    };

    assert_eq!(
        projections
            .iter()
            .filter(|projection| projection.kind() == &MirProjectionKind::Dereference)
            .count(),
        2,
        "{mir:#?}"
    );
}
#[test]
fn returned_nullable_borrow_outlives_argument_evaluation() {
    let compilation = crate::test_support::compilation(
        r#"
        module app;
        struct Receipt {
            mut value: (&mut u32)?;
            destruct() {
                if let ?value = self.value { value = 1; }
            }
        }
        func receipt(pos value: &mut u32) -> Receipt {
            let mut result: Receipt = { value = none };
            result.value = value;
            return result;
        }
        func probe() {
            let mut value: u32 = 0;
            let returned: Receipt = {
                yield receipt(&mut value);
            };
        }
    "#,
    );

    let key = crate::test_support::source_function_body_key(&compilation, "probe");

    let lowered = compilation
        .lowered_unit(key)
        .expect("fixture callable must lower successfully");

    assert!(!lowered.diagnostics().has_errors(), "{lowered:#?}");
}

#[test]
fn returned_nullable_borrow_cannot_outlive_local_storage() {
    for body in [
        "let mut value: u32 = 0; return receipt(&mut value);",
        "let returned: Receipt = { let mut value: u32 = 0; yield receipt(&mut value); }; return returned;",
    ] {
        let compilation = crate::test_support::compilation(&format!(
            r#"
            module app;
            struct Receipt {{
                mut value: (&mut u32)?;
                destruct() {{
                    if let ?value = self.value {{ value = 1; }}
                }}
            }}
            func receipt(pos value: &mut u32) -> Receipt {{
                let mut result: Receipt = {{ value = none }};
                result.value = value;
                return result;
            }}
            func probe() -> Receipt {{ {body} }}
        "#
        ));

        let lowered = compilation
            .lowered_unit(crate::test_support::source_function_body_key(
                &compilation,
                "probe",
            ))
            .expect("fixture callable must lower successfully");

        bray_testing::assert_goal_state_diagnostic_kind(
            lowered.diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
        );
    }
}
