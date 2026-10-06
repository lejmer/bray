use crate::test_support::{compilation, source_function_body_key};

use bray_symbols::TypeData;

#[test]
fn returned_borrow_aliases_lower_inside_nested_scopes() {
    let compilation = compilation(
        r#"
                module app;

                struct Holder
                {
                    value: bool;
                }

                func same(pos value: &Holder) -> &Holder
                {
                    return value;
                }

                func inspect(pos value: &Holder) -> bool
                {
                    let alias = same(value);
                    let second = alias;
                    let mut count: i32 = 0;

                    while count < 2
                    {
                        if second.value
                        {
                            count += 1;
                        }
                        else
                        {
                            return false;
                        }
                    }

                    return true;
                }
            "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );

    let key = source_function_body_key(&compilation, "inspect");

    let lowered = compilation
        .lowered_unit(key)
        .expect("stored borrow aliases must lower");

    assert!(
        !lowered.diagnostics().has_errors(),
        "{:?}",
        lowered.diagnostics()
    );
}

#[test]
fn chained_nullable_receiver_evaluates_its_producer_once() {
    let compilation = compilation(
        r#"
                module app;
                            struct Counter<T> {
                                mut calls: i32;
                                value: T;
                                mut func take() -> T? with(T: Copyable) {
                                    self.calls += 1;
                                    return self.value;
                                }
                                mut func remove() -> bool with(T: Copyable) {
                                    return self.take().is_present();
                                }
                            }
                            func inspect(pos counter: &mut Counter<i32>) -> bool {
                                return counter.take().is_present();
                            }
            "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );

    let key = crate::test_support::source_type_callable_member_body_key(&compilation, "remove");
    let lowered = compilation.lowered_unit(key).unwrap();
    let mir = lowered.value().as_ref().unwrap().mir().unwrap();
    let calls: Vec<_> = mir.operations().iter().filter(|operation| matches!(operation.kind(), bray_ir::MirOperationKind::Call(call) if matches!(call.target(), bray_ir::MirCallTarget::Direct(_)))).collect();

    assert_eq!(calls.len(), 1, "{calls:#?}");
}

#[test]
fn returned_slice_receiver_uses_the_call_result_representation() {
    let compilation = compilation(
        r#"
                module app;

                struct Buffer
                {
                    values: [bool; 2];

                    func as_slice() -> &[bool]
                    {
                        return &self.values[..];
                    }
                }

                func inspect() -> usize
                {
                    let buffer = Buffer
                    {
                        values = [true, false]
                    };

                    return buffer.as_slice().length();
                }
            "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );

    let key = source_function_body_key(&compilation, "inspect");

    let lowered = compilation
        .lowered_unit(key)
        .expect("chained returned slice must lower");

    let mir = lowered.value().as_ref().unwrap().mir().unwrap();
    let values = compilation.semantic_value_store().unwrap();

    let length = mir
        .operations()
        .iter()
        .find_map(|operation| match operation.kind() {
            bray_ir::MirOperationKind::Memory(memory)
                if memory.kind() == bray_bound_tree::CheckedMemoryOperationKind::SequenceLength =>
            {
                Some(memory)
            }
            _ => None,
        })
        .expect("slice length operation must exist");

    let [bray_ir::MirOperand::Value(receiver)] = length.operands() else {
        panic!("slice receiver must be evaluated");
    };

    let borrow = mir
        .operations()
        .iter()
        .find(|operation| operation.result() == Some(*receiver))
        .unwrap();

    let bray_ir::MirOperationKind::Borrow { place, .. } = borrow.kind() else {
        panic!("slice receiver must be reborrowed");
    };

    assert!(
        matches!(values.type_data(place.ty()).as_ref(), TypeData::Slice(_)),
        "receiver must borrow the slice result, not the anchor array: {place:?}"
    );
}
