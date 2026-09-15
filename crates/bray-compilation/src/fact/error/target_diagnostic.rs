use bray_diagnostics::{DiagnosticFailureField, DiagnosticFailureValue};

use super::diagnostic_context::{boolean_field, count_field, target_endianness, text_field};

struct TargetPropertyFieldNames {
    name: &'static str,
    boolean: &'static str,
    natural: &'static str,
    text: &'static str,
}

pub(crate) fn push_selected_target_properties(
    context: &mut Vec<DiagnosticFailureField>,
    properties: &bray_target::TargetProperties,
) {
    push_target_properties_with_names(
        context,
        &TargetPropertyFieldNames {
            name: "target_property_name",
            boolean: "target_property_boolean",
            natural: "target_property_natural",
            text: "target_property_text",
        },
        properties,
    );
}

pub(crate) fn push_target_profile(
    context: &mut Vec<DiagnosticFailureField>,
    target: &bray_target::TargetProfile,
) {
    let machine = target.machine();

    context.extend([
        text_field("target_identity", target.identity().as_str()),
        text_field("target_architecture", machine.architecture().as_str()),
        text_field("target_object_format", machine.object_format().as_str()),
        text_field("target_endianness", target_endianness(machine.endianness())),
        count_field(
            "target_pointer_width_bits",
            u64::from(machine.pointer_width_bits().get()),
        ),
        count_field(
            "target_pointer_alignment_bytes",
            u64::from(machine.pointer_alignment_bytes().get()),
        ),
        count_field(
            "target_stack_alignment_bytes",
            u64::from(machine.stack_alignment_bytes().get()),
        ),
    ]);

    push_selected_target_properties(context, target.properties());
}

fn push_target_properties_with_names(
    context: &mut Vec<DiagnosticFailureField>,
    names: &TargetPropertyFieldNames,
    properties: &bray_target::TargetProperties,
) {
    let identity = properties.identity();

    push_text_property(context, names, "identity.vendor", identity.vendor());
    push_text_property(context, names, "identity.system", identity.system());

    push_text_property(
        context,
        names,
        "identity.environment",
        identity.environment(),
    );

    push_text_property(context, names, "identity.abi", identity.abi());

    let scalars = properties.scalars();

    for scalar in bray_target::TargetScalarKind::ALL {
        let scalar_name = scalar.as_str();

        push_boolean_property(
            context,
            &names,
            format!("scalar.{scalar_name}.available"),
            scalars.supports(scalar),
        );

        push_natural_property(
            context,
            &names,
            format!("scalar.{scalar_name}.alignment"),
            scalars.alignment(scalar).get(),
        );
    }

    push_atomic_properties(context, &names, properties.atomics());
    push_abi_properties(context, &names, "abi.c", properties.abis().c_contract());

    push_abi_properties(
        context,
        &names,
        "abi.system",
        properties.abis().system_contract(),
    );

    push_c_properties(context, &names, properties.c_abi());

    push_boolean_property(
        context,
        &names,
        "address_space.host",
        properties.address_spaces().host(),
    );

    push_boolean_property(
        context,
        &names,
        "address_space.device",
        properties.address_spaces().device(),
    );

    push_natural_property(
        context,
        &names,
        "alignment.max_storage",
        properties.alignments().max_storage().get(),
    );

    push_natural_property(
        context,
        &names,
        "alignment.max_allocation",
        properties.alignments().max_allocation().get(),
    );

    let operations = properties.operations();

    push_boolean_property(
        context,
        &names,
        "operation.raw_memory",
        operations.raw_memory(),
    );

    push_boolean_property(
        context,
        &names,
        "operation.allocation",
        operations.allocation(),
    );

    push_boolean_property(
        context,
        &names,
        "operation.callable_addresses",
        operations.callable_addresses(),
    );

    let symbols = properties.native_symbols();

    push_boolean_property(
        context,
        &names,
        "native_symbol.ordinals",
        symbols.ordinals(),
    );

    push_boolean_property(
        context,
        &names,
        "native_symbol.versions",
        symbols.versions(),
    );

    push_boolean_property(
        context,
        &names,
        "native_symbol.weak_binding",
        symbols.weak_binding(),
    );

    push_boolean_property(
        context,
        &names,
        "native_symbol.optional_data",
        symbols.optional_data(),
    );

    push_boolean_property(
        context,
        &names,
        "platform.dynamic_loading",
        properties.dynamic_loading(),
    );

    push_boolean_property(
        context,
        &names,
        "platform.native_threads",
        properties.native_threads(),
    );
}

fn push_atomic_properties(
    context: &mut Vec<DiagnosticFailureField>,
    names: &TargetPropertyFieldNames,
    atomics: bray_target::TargetAtomicSupport,
) {
    for representation in bray_target::TargetAtomicRepresentation::ALL {
        let representation_name = representation.as_str();
        let support = atomics.representation(representation);
        let operations = support.operations();

        for (operation_name, available) in [
            ("load_store", operations.load_store()),
            ("exchange", operations.exchange()),
            ("compare_exchange", operations.compare_exchange()),
            ("fetch_arithmetic", operations.fetch_arithmetic()),
            ("fetch_bitwise", operations.fetch_bitwise()),
        ] {
            push_boolean_property(
                context,
                names,
                format!("atomic.{representation_name}.{operation_name}"),
                available,
            );
        }

        push_natural_property(
            context,
            names,
            format!("atomic.{representation_name}.alignment"),
            support.required_alignment().get(),
        );

        push_boolean_property(
            context,
            names,
            format!("atomic.{representation_name}.always_lock_free"),
            support.always_lock_free(),
        );

        push_boolean_property(
            context,
            names,
            format!("atomic.{representation_name}.wait_notify"),
            support.wait_notify(),
        );

        push_boolean_property(
            context,
            names,
            format!("atomic.{representation_name}.cross_process"),
            support.cross_process(),
        );
    }
}

fn push_abi_properties(
    context: &mut Vec<DiagnosticFailureField>,
    names: &TargetPropertyFieldNames,
    abi_name: &'static str,
    contract: Option<bray_target::TargetForeignAbiContract>,
) {
    push_boolean_property(
        context,
        names,
        format!("{abi_name}.available"),
        contract.is_some(),
    );

    let Some(contract) = contract else {
        return;
    };

    for scalar in bray_target::TargetScalarKind::ALL {
        push_boolean_property(
            context,
            names,
            format!("{abi_name}.scalar.{}", scalar.as_str()),
            contract.scalars().supports(scalar),
        );
    }

    for (property_name, value) in [
        ("raw_pointers", contract.raw_pointers()),
        ("qualified_callables", contract.qualified_callables()),
        ("c_layout", contract.c_layout()),
        ("transparent_layout", contract.transparent_layout()),
        ("variadic", contract.variadic()),
    ] {
        push_boolean_property(context, names, format!("{abi_name}.{property_name}"), value);
    }

    push_natural_property(
        context,
        names,
        format!("{abi_name}.max_alignment"),
        contract.max_alignment().get(),
    );
}

fn push_c_properties(
    context: &mut Vec<DiagnosticFailureField>,
    names: &TargetPropertyFieldNames,
    model: bray_target::TargetCDataModel,
) {
    for scalar in bray_target::TargetCScalarKind::ALL {
        let c_name = scalar.as_str();
        let mapping = model.mapping(scalar);

        push_boolean_property(
            context,
            names,
            format!("c.{c_name}.available"),
            mapping.is_some(),
        );

        if let Some(mapping) = mapping {
            push_text_property(
                context,
                names,
                format!("c.{c_name}.mapping"),
                mapping.as_str(),
            );
        }
    }
}

fn push_boolean_property(
    context: &mut Vec<DiagnosticFailureField>,
    names: &TargetPropertyFieldNames,
    property: impl Into<String>,
    value: bool,
) {
    context.push(text_field(names.name, property));
    context.push(boolean_field(names.boolean, value));
}

fn push_natural_property(
    context: &mut Vec<DiagnosticFailureField>,
    names: &TargetPropertyFieldNames,
    property: impl Into<String>,
    value: u64,
) {
    context.push(text_field(names.name, property));

    context.push(DiagnosticFailureField::new(
        names.natural,
        DiagnosticFailureValue::Natural(value.to_string()),
    ));
}

fn push_text_property(
    context: &mut Vec<DiagnosticFailureField>,
    names: &TargetPropertyFieldNames,
    property: impl Into<String>,
    value: impl Into<String>,
) {
    context.push(text_field(names.name, property));
    context.push(text_field(names.text, value));
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticFailureField, DiagnosticFailureValue};

    use super::push_target_profile;

    #[test]
    fn target_profile_context_preserves_machine_and_typed_properties() {
        let target = bray_target::test_support::test_target_profile();

        let mut fields = Vec::new();

        push_target_profile(&mut fields, &target);

        let leading_names: Vec<_> = fields[..7].iter().map(|field| field.name()).collect();

        assert_eq!(
            leading_names,
            [
                "target_identity",
                "target_architecture",
                "target_object_format",
                "target_endianness",
                "target_pointer_width_bits",
                "target_pointer_alignment_bytes",
                "target_stack_alignment_bytes",
            ]
        );

        assert_eq!(
            property_value(&fields, "identity.vendor"),
            &DiagnosticFailureValue::Text("unknown".to_owned())
        );
    }

    #[test]
    fn target_profile_context_exposes_the_concrete_changed_property() {
        let baseline = bray_target::test_support::test_target_profile();

        let changed = bray_target::TargetProfile::try_new(
            baseline.identity().clone(),
            baseline.machine().clone(),
            baseline
                .properties()
                .clone()
                .with_operations(bray_target::TargetOperationSupport::new(true, false, false)),
        )
        .unwrap_or_else(|error| panic!("changed test target profile must be valid: {error:?}"));

        let mut baseline_fields = Vec::new();
        let mut changed_fields = Vec::new();

        push_target_profile(&mut baseline_fields, &baseline);
        push_target_profile(&mut changed_fields, &changed);

        assert_eq!(
            property_value(&baseline_fields, "operation.raw_memory"),
            &DiagnosticFailureValue::Boolean(false)
        );

        assert_eq!(
            property_value(&changed_fields, "operation.raw_memory"),
            &DiagnosticFailureValue::Boolean(true)
        );
    }

    fn property_value<'a>(
        fields: &'a [DiagnosticFailureField],
        property: &str,
    ) -> &'a DiagnosticFailureValue {
        fields
            .windows(2)
            .find_map(|fields| match (fields[0].name(), fields[0].value()) {
                ("target_property_name", DiagnosticFailureValue::Text(name))
                    if name == property =>
                {
                    Some(fields[1].value())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("target property {property} must be present"))
    }
}
