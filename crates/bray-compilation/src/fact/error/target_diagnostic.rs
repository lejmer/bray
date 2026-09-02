use bray_diagnostics::{DiagnosticFailureField, DiagnosticFailureValue};

use super::diagnostic_context::{boolean_field, count_field, text_field};

#[derive(Clone, Copy)]
pub(crate) enum TargetContractSide {
    Expected,
    Actual,
}

struct TargetPropertyFieldNames {
    name: &'static str,
    boolean: &'static str,
    natural: &'static str,
    text: &'static str,
}

impl TargetContractSide {
    const fn property_field_names(self) -> TargetPropertyFieldNames {
        match self {
            Self::Expected => TargetPropertyFieldNames {
                name: "expected_target_property_name",
                boolean: "expected_target_property_boolean",
                natural: "expected_target_property_natural",
                text: "expected_target_property_text",
            },
            Self::Actual => TargetPropertyFieldNames {
                name: "actual_target_property_name",
                boolean: "actual_target_property_boolean",
                natural: "actual_target_property_natural",
                text: "actual_target_property_text",
            },
        }
    }
}

pub(crate) fn push_mir_target_contract(
    context: &mut Vec<DiagnosticFailureField>,
    side: TargetContractSide,
    target: &bray_ir::MirTargetContract,
) {
    let machine = target.machine();
    let version = target.runtime_abi();

    let names = match side {
        TargetContractSide::Expected => [
            "expected_target_identity",
            "expected_target_architecture",
            "expected_target_object_format",
            "expected_target_endianness",
            "expected_target_pointer_width_bits",
            "expected_target_pointer_alignment_bytes",
            "expected_target_stack_alignment_bytes",
            "expected_runtime_abi_major",
            "expected_runtime_abi_minor",
        ],
        TargetContractSide::Actual => [
            "actual_target_identity",
            "actual_target_architecture",
            "actual_target_object_format",
            "actual_target_endianness",
            "actual_target_pointer_width_bits",
            "actual_target_pointer_alignment_bytes",
            "actual_target_stack_alignment_bytes",
            "actual_runtime_abi_major",
            "actual_runtime_abi_minor",
        ],
    };

    context.extend([
        text_field(names[0], target.identity().as_str()),
        text_field(names[1], machine.architecture().as_str()),
        text_field(names[2], machine.object_format().as_str()),
        text_field(
            names[3],
            match machine.endianness() {
                bray_target::Endianness::Little => "little",
                bray_target::Endianness::Big => "big",
            },
        ),
        count_field(names[4], u64::from(machine.pointer_width_bits().get())),
        count_field(names[5], u64::from(machine.pointer_alignment_bytes().get())),
        count_field(names[6], u64::from(machine.stack_alignment_bytes().get())),
        count_field(names[7], u64::from(version.major())),
        count_field(names[8], u64::from(version.minor())),
    ]);

    push_target_properties(context, side, target.profile().properties());
}

fn push_target_properties(
    context: &mut Vec<DiagnosticFailureField>,
    side: TargetContractSide,
    properties: &bray_target::TargetProperties,
) {
    let names = side.property_field_names();
    let identity = properties.identity();

    push_text_property(context, &names, "identity.vendor", identity.vendor());
    push_text_property(context, &names, "identity.system", identity.system());

    push_text_property(
        context,
        &names,
        "identity.environment",
        identity.environment(),
    );

    push_text_property(context, &names, "identity.abi", identity.abi());

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

    push_boolean_property(context, &names, "operation.raw_memory", operations.raw_memory());
    push_boolean_property(context, &names, "operation.allocation", operations.allocation());

    push_boolean_property(
        context,
        &names,
        "operation.callable_addresses",
        operations.callable_addresses(),
    );

    let symbols = properties.native_symbols();

    push_boolean_property(context, &names, "native_symbol.ordinals", symbols.ordinals());
    push_boolean_property(context, &names, "native_symbol.versions", symbols.versions());

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
        push_boolean_property(
            context,
            names,
            format!("{abi_name}.{property_name}"),
            value,
        );
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

    use super::{TargetContractSide, push_mir_target_contract};

    #[test]
    fn mir_target_contract_context_preserves_machine_runtime_and_typed_properties() {
        let target = bray_ir::MirTargetContract::new(
            bray_target::test_support::test_target_profile(),
            bray_runtime_interface::RuntimeAbiVersion::new(3, 7),
        );

        let mut fields = Vec::new();

        push_mir_target_contract(&mut fields, TargetContractSide::Expected, &target);

        let leading_names: Vec<_> = fields[..9].iter().map(|field| field.name()).collect();

        assert_eq!(
            leading_names,
            [
                "expected_target_identity",
                "expected_target_architecture",
                "expected_target_object_format",
                "expected_target_endianness",
                "expected_target_pointer_width_bits",
                "expected_target_pointer_alignment_bytes",
                "expected_target_stack_alignment_bytes",
                "expected_runtime_abi_major",
                "expected_runtime_abi_minor",
            ]
        );

        assert_eq!(fields[7].value(), &DiagnosticFailureValue::Count(3));
        assert_eq!(fields[8].value(), &DiagnosticFailureValue::Count(7));

        assert_eq!(
            property_value(&fields, "identity.vendor"),
            &DiagnosticFailureValue::Text("unknown".to_owned())
        );
    }

    #[test]
    fn mir_target_contract_context_exposes_the_concrete_changed_property() {
        let baseline = bray_target::test_support::test_target_profile();

        let changed = bray_target::TargetProfile::try_new(
            baseline.identity().clone(),
            baseline.machine().clone(),
            baseline.properties().clone().with_operations(
                bray_target::TargetOperationSupport::new(true, false, false),
            ),
        )
        .unwrap_or_else(|error| panic!("changed test target profile must be valid: {error:?}"));

        let version = bray_runtime_interface::RuntimeAbiVersion::new(3, 7);
        let baseline = bray_ir::MirTargetContract::new(baseline, version);
        let changed = bray_ir::MirTargetContract::new(changed, version);
        let mut baseline_fields = Vec::new();
        let mut changed_fields = Vec::new();

        push_mir_target_contract(
            &mut baseline_fields,
            TargetContractSide::Expected,
            &baseline,
        );

        push_mir_target_contract(
            &mut changed_fields,
            TargetContractSide::Expected,
            &changed,
        );

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
                ("expected_target_property_name", DiagnosticFailureValue::Text(name))
                    if name == property =>
                {
                    Some(fields[1].value())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("target property {property} must be present"))
    }
}
