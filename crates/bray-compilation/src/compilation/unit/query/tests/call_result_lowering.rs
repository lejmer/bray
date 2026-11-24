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

    let mir = lowered
        .value()
        .as_ref()
        .expect("fixture optional value must be present")
        .mir()
        .expect("fixture instance must retain its MIR");

    let values = compilation
        .semantic_value_store()
        .expect("fixture compilation must provide its semantic value store");

    let length = mir
        .operations()
        .iter()
        .find_map(|operation| match operation.kind() {
            bray_ir::MirOperationKind::Memory(memory)
                if memory.kind()
                    == &bray_bound_tree::CheckedMemoryOperationKind::SequenceLength =>
            {
                Some(memory)
            }
            _ => None,
        })
        .expect("slice length operation must exist");

    let [receiver] = length.operands() else {
        panic!("slice length must have one receiver");
    };

    let place = match receiver {
        bray_ir::MirOperand::Copy(place) | bray_ir::MirOperand::Move(place) => place,
        bray_ir::MirOperand::Value(receiver) => {
            let borrow = mir.operations().iter()
                .find(|operation| operation.result() == Some(*receiver))
                .expect("fixture lookup must find its expected value");

            let bray_ir::MirOperationKind::Borrow { place, .. } = borrow.kind() else {
                panic!("evaluated slice receiver must be a borrow");
            };

            place
        }
        _ => panic!("slice receiver must read the result storage: {receiver:?}"),
    };

    assert!(
        matches!(values.type_data(place.ty()).as_ref(), TypeData::Slice(_)),
        "receiver must borrow the slice result, not the anchor array: {place:?}"
    );
}
