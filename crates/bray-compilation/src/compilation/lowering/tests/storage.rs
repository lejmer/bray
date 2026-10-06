use std::collections::BTreeSet;

use bray_bound_tree::BoundUnitKind;
use bray_ir::{
    MirAggregateKind, MirImmediateValue, MirOperand, MirOperationKind, MirProjectionKind,
    MirStorageKind, MirStoreKind, MirTerminatorKind, MirUnit, MirValueOrigin,
};
use bray_symbols::{BorrowKind, TypeData};

use super::support::{declared_unit_key, lowered_mir};
use crate::test_support::{compilation, source_callable_body_key, source_function_body_key};

const STRUCTURED_LOWERING_SOURCE: &str = concat!(
    "module app;\n",
    "\n",
    "struct Pair\n",
    "{\n",
    "    first: i32;\n",
    "    second: i32 = 9;\n",
    "}\n",
    "\n",
    "union Choice\n",
    "{\n",
    "    Value(value: i32);\n",
    "    Empty;\n",
    "}\n",
    "\n",
    "func main() -> i64\n",
    "{\n",
    "    let tuple: (i32, i32) = (1, 2);\n",
    "    let array: [i32; 2] = [3, 4];\n",
    "    let repeated: [i32; 2] = [5; 2];\n",
    "    let pair: Pair = Pair { first = tuple.0, second = array[0] };\n",
    "    let defaulted: Pair = Pair { first = 9 };\n",
    "    let widened: (i64, i64) = tuple as (i64, i64);\n",
    "    let choice: Choice = Value(value = pair.first);\n",
    "    let empty: Choice = Empty;\n",
    "    let owned: box i32 = box(8);\n",
    "    let owned_tuple: (box i32,) = (owned,);\n",
    "    let moved: box i32 = owned_tuple.0;\n",
    "    return pair.first as i64;\n",
    "}\n",
);

#[test]
fn runtime_default_borrows_caller_owned_input_storage() {
    let compilation = compilation(
        r#"
            module app;

            func observe(pos first: bool, second: &bool = &first)
            {
            }
        "#,
    );

    let key = declared_unit_key(&compilation, BoundUnitKind::RuntimeDefault);

    let result = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("runtime default MIR must publish: {error:?}"));

    let mir = lowered_mir(&result);

    assert!(mir.storages_with_ids().any(|(_, storage)| matches!(
        storage.kind(),
        bray_ir::MirStorageKind::BorrowedParameter(0)
    )));

    assert!(
        !mir.storages_with_ids()
            .any(|(_, storage)| matches!(storage.kind(), bray_ir::MirStorageKind::Parameter(_)))
    );
}

#[test]
fn structured_values_lower_to_aggregates_construction_and_projections() {
    let compilation = compilation(STRUCTURED_LOWERING_SOURCE);
    let key = source_callable_body_key(&compilation);

    let result = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("structured MIR must be available: {error:?}"));

    let mir = lowered_mir(&result);

    let aggregate_kinds = mir.operations().iter().filter_map(|operation| {
        let MirOperationKind::Aggregate(aggregate) = operation.kind() else {
            return None;
        };

        Some(aggregate.kind())
    });

    assert_eq!(
        aggregate_kinds.collect::<Vec<_>>(),
        [
            MirAggregateKind::Tuple,
            MirAggregateKind::Array,
            MirAggregateKind::RepeatedArray,
            MirAggregateKind::Tuple,
        ]
    );

    assert!(
        mir.operations()
            .iter()
            .any(|operation| matches!(operation.kind(), MirOperationKind::Construct(_)))
    );

    let construction_targets = mir.operations().iter().filter_map(|operation| {
        let MirOperationKind::Construct(construction) = operation.kind() else {
            return None;
        };

        Some(construction.target())
    });

    assert_eq!(
        construction_targets
            .filter(|target| matches!(target, bray_bound_tree::ConstructionTarget::UnionVariant(_)))
            .count(),
        2
    );

    assert!(mir.operations().iter().any(|operation| matches!(
        operation.kind(),
        MirOperationKind::Construct(construction)
            if matches!(
                construction.target(),
                bray_bound_tree::ConstructionTarget::TypeForm { .. }
            )
    )));

    assert!(mir.operations().iter().any(|operation| matches!(operation.kind(),
        MirOperationKind::Call(call) if matches!(call.target(), bray_ir::MirCallTarget::DefaultValue { .. }))));

    assert!(mir.operations().iter().any(|operation| {
        let MirOperationKind::Construct(construction) = operation.kind() else {
            return false;
        };

        construction
            .inputs()
            .iter()
            .all(|input| matches!(input.value(), MirOperand::Move(_)))
    }));

    assert!(mir.operations().iter().any(|operation| matches!(
        operation.kind(),
        MirOperationKind::Store {
            value: MirOperand::Move(place),
            ..
        } if !place.projections().is_empty()
    )));

    assert!(mir.operations().iter().any(|operation| matches!(
        operation.kind(),
        MirOperationKind::Convert { conversion, .. }
            if conversion.source_type() != conversion.target_type()
    )));

    assert!(mir.operations().iter().any(|operation| matches!(
        operation.kind(),
        MirOperationKind::Convert { conversion, .. }
            if matches!(
                conversion.target(),
                bray_bound_tree::ConversionTarget::Composite(_)
            )
    )));

    let returns = mir
        .blocks()
        .iter()
        .filter_map(|block| match block.terminator().kind() {
            MirTerminatorKind::Return(value) => Some(value),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(
        matches!(returns.as_slice(), [Some(MirOperand::Move(place))] if mir.storage(place.storage()).unwrap().kind() == &bray_ir::MirStorageKind::Return),
        "cleanup continuations must preserve the single value return: {mir:?}"
    );
}

#[test]
fn custom_indexing_lowers_shared_and_mutable_access_as_places() {
    let compilation = compilation(
        r#"module app;

struct Item
{
    mut value: i32;
}

struct Values
{
    mut first: Item;
    mut second: Item;
}

impl Values(ElementIndex<i32>)
{
    type Output = Item;

    func index(pos selector: &i32) -> &Item
    {
        return &self.first;
    }
}

impl Values(MutableElementIndex<i32>)
{
    type Output = Item;

    mut func index(pos selector: &i32) -> &mut Item
    {
        return &mut self.second;
    }
}

impl Values(SliceIndex<i32>)
{
    type Output = Item;

    func slice(pos start: i32?, pos end: i32?) -> &Item
    {
        return &self.first;
    }
}

func observe(pos values: Values) -> i32
{
    return values[0].value;
}

func borrow_shared(pos values: &Values)
{
    let selected: &Item = &values[0];
}

func borrow_mutable(pos values: &mut Values)
{
    let selected: &mut Item = &mut values[0];
}

func assign(pos input: Values)
{
    let mut values: Values = input;
    values[0] = Item { value = 3 };
}

func nested_assign(pos input: Values)
{
    let mut values: Values = input;
    values[0].value = 4;
}

func compound_assign(pos input: Values)
{
    let mut values: Values = input;
    values[0].value += 1;
}

func lower_only(pos values: Values) -> i32
{
    return values[1..].value;
}

func upper_only(pos values: Values) -> i32
{
    return values[..2].value;
}

func both_bounds(pos values: Values) -> i32
{
    return values[1..2].value;
}

"#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    for function in [
        "observe",
        "borrow_shared",
        "borrow_mutable",
        "assign",
        "nested_assign",
        "compound_assign",
        "lower_only",
        "upper_only",
        "both_bounds",
    ] {
        let result = compilation
            .lowered_unit(source_function_body_key(&compilation, function))
            .unwrap_or_else(|error| panic!("{function} MIR must be available: {error:?}"));

        let mir = lowered_mir(&result);

        let protocol_calls = mir
            .operations()
            .iter()
            .filter(|operation| {
                matches!(operation.kind(), MirOperationKind::Call(call) if !call.witnesses().is_empty())
            })
            .count();

        assert_eq!(protocol_calls, 1, "{function}: {mir:#?}");
    }

    for (function, kind) in [
        ("borrow_shared", BorrowKind::Shared),
        ("borrow_mutable", BorrowKind::Mutable),
    ] {
        let result = compilation
            .lowered_unit(source_function_body_key(&compilation, function))
            .unwrap_or_else(|error| panic!("{function} MIR must be available: {error:?}"));

        let mir = lowered_mir(&result);

        let receiver = mir
            .operations()
            .iter()
            .find_map(|operation| match operation.kind() {
                MirOperationKind::Borrow { place, .. } => Some(place),
                _ => None,
            })
            .expect("custom index must borrow its receiver");

        assert!(
            matches!(
                compilation
                    .semantic_value_store()
                    .unwrap()
                    .type_data(receiver.ty())
                    .as_ref(),
                bray_symbols::TypeData::Named { .. }
            ),
            "custom index must borrow Values, not its parameter borrow: {mir:#?}"
        );

        assert!(
            mir.operations().iter().any(|operation| matches!(
                operation.kind(),
                MirOperationKind::Borrow { kind: actual, place }
                    if *actual == kind
                        && matches!(
                            place.projections().first().map(bray_ir::MirProjection::kind),
                            Some(MirProjectionKind::Dereference)
                        )
            )),
            "{function}: {mir:#?}"
        );
    }

    for function in ["assign", "nested_assign", "compound_assign"] {
        let assignment = compilation
            .lowered_unit(source_function_body_key(&compilation, function))
            .unwrap_or_else(|error| panic!("{function} MIR must be available: {error:?}"));

        let assignment = lowered_mir(&assignment);

        assert!(
            assignment.operations().iter().any(|operation| matches!(
                operation.kind(),
                MirOperationKind::Store {
                    kind: MirStoreKind::Assign,
                    destination,
                    ..
                } if matches!(
                    destination.projections().first().map(bray_ir::MirProjection::kind),
                    Some(MirProjectionKind::Dereference)
                )
            )),
            "{function}: {assignment:#?}"
        );
    }

    let lower_only = compilation
        .lowered_unit(source_function_body_key(&compilation, "lower_only"))
        .unwrap_or_else(|error| panic!("lower-only MIR must be available: {error:?}"));

    let upper_only = compilation
        .lowered_unit(source_function_body_key(&compilation, "upper_only"))
        .unwrap_or_else(|error| panic!("upper-only MIR must be available: {error:?}"));

    let both_bounds = compilation
        .lowered_unit(source_function_body_key(&compilation, "both_bounds"))
        .unwrap_or_else(|error| panic!("both-bound MIR must be available: {error:?}"));

    let lower_only = lowered_mir(&lower_only);
    let upper_only = lowered_mir(&upper_only);
    let both_bounds = lowered_mir(&both_bounds);

    let [_, lower_start, lower_end] = custom_index_call_arguments(lower_only) else {
        panic!("lower-only call must retain receiver, start, and end arguments");
    };

    let [_, upper_start, upper_end] = custom_index_call_arguments(upper_only) else {
        panic!("upper-only call must retain receiver, start, and end arguments");
    };

    let [_, both_start, both_end] = custom_index_call_arguments(both_bounds) else {
        panic!("both-bound call must retain receiver, start, and end arguments");
    };

    let lower_start = explicit_call_operand(lower_start);
    let lower_end = explicit_call_operand(lower_end);
    let upper_start = explicit_call_operand(upper_start);
    let upper_end = explicit_call_operand(upper_end);
    let both_start = explicit_call_operand(both_start);
    let both_end = explicit_call_operand(both_end);

    let lower_payload = nullable_payload(lower_only, lower_start)
        .unwrap_or_else(|| panic!("lower-only start must be present"));

    let upper_payload = nullable_payload(upper_only, upper_end)
        .unwrap_or_else(|| panic!("upper-only end must be present"));

    assert_eq!(nullable_payload(lower_only, lower_end), None);
    assert_eq!(nullable_payload(upper_only, upper_start), None);

    assert_eq!(
        nullable_payload(both_bounds, both_start),
        Some(lower_payload)
    );

    assert_eq!(nullable_payload(both_bounds, both_end), Some(upper_payload));

    assert_ne!(lower_payload, upper_payload);
}

#[test]
fn annotated_nullable_local_initializers_materialize_presence() {
    let compilation = compilation(
        r#"module app;

struct OwnedValue
{
    value: i32;
}

func from_literal() -> i32?
{
    let value: i32? = 1;

    return value;
}

func from_name(pos input: i32) -> i32?
{
    let value: i32? = input;

    return value;
}

func from_unit() -> unit?
{
    let value: unit? = unit;

    return value;
}

func from_owned() -> OwnedValue?
{
    let value: OwnedValue? = OwnedValue
    {
        value = 1
    };

    return value;
}

func return_literal() -> i32?
{
    return 1;
}
"#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    for name in [
        "from_literal",
        "from_name",
        "from_unit",
        "from_owned",
        "return_literal",
    ] {
        let key = source_function_body_key(&compilation, name);

        let lowered = compilation.lowered_unit(key).unwrap_or_else(|error| {
            panic!("nullable initializer MIR for {name} must be available: {error:?}")
        });

        let mir = lowered_mir(&lowered);

        assert!(
            mir.operations().iter().any(|operation| matches!(
                operation.kind(),
                MirOperationKind::Aggregate(aggregate)
                    if aggregate.kind() == MirAggregateKind::NullablePresent
            )),
            "{mir:#?}"
        );
    }
}

#[test]
fn annotated_nullable_local_constant_initializers_retain_nullable_storage() {
    let compilation = compilation(
        r#"module app;

func main() -> i32?
{
    const value: i32? = 1;

    return value;
}
"#,
    );

    let key = source_function_body_key(&compilation, "main");

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let lowered = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("nullable local constant MIR must be available: {error:?}"));

    let mir = lowered_mir(&lowered);

    let Some(MirTerminatorKind::Return(Some(MirOperand::Copy(place)))) =
        mir.blocks().last().map(|block| block.terminator().kind())
    else {
        panic!("nullable local constant must lower through typed storage: {mir:#?}");
    };

    let values = compilation
        .semantic_value_store()
        .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

    let ty = values.type_data(place.ty());

    assert!(matches!(ty.as_ref(), TypeData::Nullable(_)));
}

#[test]
fn nested_generic_static_indexes_retain_both_selected_instances() {
    let compilation = compilation(concat!(
        "module app;\n",
        "internal static VALUES<const N: usize>: [usize; 2] = [N, N];\n",
        "public func read() -> usize\n",
        "{\n",
        "    return VALUES<1>[VALUES<2>[0] - 2];\n",
        "}\n",
    ));

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "read"))
        .unwrap_or_else(|error| panic!("nested static indexes must lower: {error:?}"));

    let mir = lowered_mir(&lowered);

    let selections = mir
        .storages()
        .iter()
        .filter_map(|storage| match storage.kind() {
            MirStorageKind::Static(selection) => Some(selection),
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    assert_eq!(selections.len(), 2, "{mir:#?}");
}

fn custom_index_call_arguments(mir: &MirUnit) -> &[bray_ir::MirCallArgument] {
    mir.operations()
        .iter()
        .find_map(|operation| match operation.kind() {
            MirOperationKind::Call(call) if !call.witnesses().is_empty() => Some(call.arguments()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("custom index protocol call must be present: {mir:#?}"))
}

fn explicit_call_operand(argument: &bray_ir::MirCallArgument) -> &MirOperand {
    argument.value()
}

fn nullable_payload<'mir>(
    mir: &'mir MirUnit,
    operand: &'mir MirOperand,
) -> Option<&'mir MirOperand> {
    match operand {
        MirOperand::Immediate {
            value: MirImmediateValue::NullableAbsent,
            ..
        } => None,
        MirOperand::Value(value) => {
            let value = mir
                .value(*value)
                .unwrap_or_else(|| panic!("nullable value must exist: {value:?}"));

            let MirValueOrigin::Operation(operation) = value.origin() else {
                panic!("present nullable must be produced by an operation");
            };

            let operation = mir
                .operation(operation)
                .unwrap_or_else(|| panic!("nullable operation must exist: {operation:?}"));

            let MirOperationKind::Aggregate(aggregate) = operation.kind() else {
                panic!("present nullable must be an aggregate: {operation:#?}");
            };

            assert_eq!(aggregate.kind(), MirAggregateKind::NullablePresent);

            let [payload] = aggregate.operands() else {
                panic!("present nullable must retain exactly one payload");
            };

            Some(payload)
        }
        _ => panic!("slice bound must be a present or absent nullable: {operand:#?}"),
    }
}
