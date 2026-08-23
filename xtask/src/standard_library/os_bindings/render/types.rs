use std::fmt::Write;

use super::super::model::{
    ParameterDescription, TargetDescription, TypeDescription, abi_integer_representation,
};

// Formatting through String's fmt::Write implementation has no failure path.
pub(super) fn render_abi_type_contracts(source: &mut String, target: &TargetDescription) {
    for ty in &target.types {
        match ty {
            TypeDescription::Structure(aggregate) | TypeDescription::Union(aggregate) => {
                for field in &aggregate.fields {
                    render_abi_type_contract(
                        source,
                        target,
                        &format!("{}.{}", aggregate.name, field.name),
                        &field.ty,
                        &field.native_type,
                    );
                }
            }
            TypeDescription::Flexible(flexible) => {
                for field in &flexible.fields {
                    render_abi_type_contract(
                        source,
                        target,
                        &format!("{}.{}", flexible.name, field.name),
                        &field.ty,
                        &field.native_type,
                    );
                }

                render_abi_type_contract(
                    source,
                    target,
                    &format!("{}.{}", flexible.name, flexible.tail.name),
                    &flexible.tail.ty,
                    &flexible.tail.native_type,
                );
            }
            TypeDescription::Bitfields(bitfields) => {
                render_abi_type_contract(
                    source,
                    target,
                    &format!("{} backing storage", bitfields.name),
                    &bitfields.backing_type,
                    &bitfields.native_backing_type,
                );

                for field in &bitfields.fields {
                    render_abi_type_contract(
                        source,
                        target,
                        &format!("{}.{}", bitfields.name, field.name),
                        &field.ty,
                        &field.native_type,
                    );
                }
            }
            TypeDescription::Incomplete(_) | TypeDescription::Opaque(_) => {}
        }
    }

    for callback in &target.callbacks {
        render_callable_type_contracts(
            source,
            target,
            &callback.name,
            &callback.parameters,
            &callback.result,
            &callback.native_result,
        );
    }

    for function in &target.functions {
        render_callable_type_contracts(
            source,
            target,
            &function.name,
            &function.parameters,
            &function.result,
            &function.native_result,
        );
    }

    for static_ in &target.statics {
        render_abi_type_contract(
            source,
            target,
            &static_.name,
            &static_.ty,
            &static_.native_type,
        );
    }
}

fn render_callable_type_contracts(
    source: &mut String,
    target: &TargetDescription,
    callable: &str,
    parameters: &[ParameterDescription],
    result: &str,
    native_result: &str,
) {
    for parameter in parameters {
        render_abi_type_contract(
            source,
            target,
            &format!("{callable} parameter {}", parameter.name),
            &parameter.ty,
            &parameter.native_type,
        );
    }

    if result != "unit" {
        render_abi_type_contract(
            source,
            target,
            &format!("{callable} result"),
            result,
            native_result,
        );
    }
}

fn render_abi_type_contract(
    source: &mut String,
    target: &TargetDescription,
    context: &str,
    bray: &str,
    native: &str,
) {
    if bray.starts_with("RawPointer<") {
        writeln!(
            source,
            "_Static_assert(sizeof({native}) == sizeof(void *), \"{context} Bray pointer representation\");"
        )
        .expect("writing to a string must succeed");

        return;
    }

    if let Some((size, signed)) = abi_integer_representation(bray) {
        writeln!(
            source,
            "_Static_assert(sizeof({native}) == {size}, \"{context} Bray integer size\");"
        )
        .expect("writing to a string must succeed");

        if !(bray == "u8" && native == "char") {
            writeln!(
                source,
                "_Static_assert((({native})-1 < ({native})0) == {}, \"{context} Bray integer signedness\");",
                u8::from(signed)
            )
            .expect("writing to a string must succeed");
        }

        return;
    }

    if let Some(canonical) = declared_native_type(target, bray) {
        writeln!(
            source,
            "_Static_assert(__builtin_types_compatible_p({native}, {canonical}), \"{context} Bray named type\");"
        )
        .expect("writing to a string must succeed");

        return;
    }

    let canonical = match bray {
        "r32" => "float",
        "r64" => "double",
        _ => unreachable!("validated ABI types have native representations"),
    };

    writeln!(
        source,
        "_Static_assert(__builtin_types_compatible_p({native}, {canonical}), \"{context} Bray floating-point type\");"
    )
    .expect("writing to a string must succeed");
}

fn declared_native_type<'a>(target: &'a TargetDescription, name: &str) -> Option<&'a str> {
    if let Some(callback) = target
        .callbacks
        .iter()
        .find(|callback| callback.name == name)
    {
        return Some(&callback.native);
    }

    target.types.iter().find_map(|ty| match ty {
        TypeDescription::Structure(description) | TypeDescription::Union(description)
            if description.name == name =>
        {
            Some(description.native.as_str())
        }
        TypeDescription::Incomplete(description) if description.name == name => {
            Some(description.native.as_str())
        }
        TypeDescription::Opaque(description) if description.name == name => {
            Some(description.native.as_str())
        }
        TypeDescription::Flexible(description) if description.name == name => {
            Some(description.native.as_str())
        }
        TypeDescription::Bitfields(description) if description.name == name => {
            Some(description.native.as_str())
        }
        _ => None,
    })
}
