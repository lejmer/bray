use bray_diagnostics::{DiagnosticFailureField, DiagnosticFailureValue};

use crate::fact::diagnostic_context::{count_field, identity_field, push_symbol, text_field};

pub(super) fn push_mir_helper(
    fields: &mut Vec<DiagnosticFailureField>,
    helper: &bray_ir::MirHelperReference,
) {
    use bray_ir::MirHelperReference as Helper;

    fields.push(text_field("helper_kind", helper.kind_name()));
    fields.push(text_field("helper_abi", callable_abi(helper.abi())));

    if let Some(role) = helper.runtime_role() {
        fields.push(text_field("helper_runtime_role", role.as_str()));
    }

    match helper {
        Helper::AnonymousCallable(reference) => match reference {
            bray_ir::MirAnonymousCallableReference::Bound(unit) => {
                fields.push(text_field("helper_callable_kind", "bound"));
                fields.push(identity_field("helper_callable", unit));
            }
            bray_ir::MirAnonymousCallableReference::Imported(key) => {
                fields.push(text_field("helper_callable_kind", "imported"));
                fields.push(identity_field("helper_callable", key));
            }
        },
        Helper::DeclaredCallable(reference) => {
            fields.push(identity_field(
                "helper_callable_instance",
                &reference.instance(),
            ));
        }
        Helper::CallableDefault(provider) => {
            fields.push(identity_field("helper_default_provider", provider));
        }
        Helper::ConstructionDefault(provider) => {
            push_symbol(
                fields,
                "helper_default_provider_kind",
                "helper_default_provider",
                provider.symbol(),
            );
        }
        Helper::TypeForm(instance) | Helper::Conversion(instance) => {
            fields.push(identity_field("helper_callable_instance", instance));
        }
        Helper::StandardLibrary(helper) => {
            fields.push(text_field(
                "helper_standard_library_operation",
                mir_standard_library_helper(*helper),
            ));
        }
        Helper::Finalize(ty)
        | Helper::StaticFinalize(ty)
        | Helper::Destroy(ty)
        | Helper::Abandon { ty, .. } => {
            fields.push(identity_field("helper_type", ty));
        }
        Helper::Cleanup { phase, ty } => {
            fields.push(text_field(
                "helper_cleanup_phase",
                mir_cleanup_phase(*phase),
            ));

            fields.push(identity_field("helper_type", ty));
        }
        Helper::CreateFrame(frame) | Helper::ComposeAwaitedFrame(frame) => {
            push_mir_frame_reference(fields, *frame);
        }
        Helper::BeginGenerator
        | Helper::PushGenerator
        | Helper::FinishGenerator
        | Helper::PanicReport
        | Helper::DestroyTerminalTask => {}
    }
}

pub(super) fn push_mir_call_target(
    fields: &mut Vec<DiagnosticFailureField>,
    target: &bray_ir::MirCallTarget,
) {
    use bray_ir::MirCallTarget as Target;

    fields.push(text_field(
        "call_target_kind",
        match target {
            Target::Direct(_) => "direct",
            Target::Runtime(_) => "runtime",
            Target::Indirect { .. } => "indirect",
        },
    ));

    fields.push(text_field("call_target_abi", callable_abi(target.abi())));

    match target {
        Target::Direct(reference) => {
            fields.push(identity_field(
                "call_target_callable_instance",
                &reference.instance(),
            ));
        }
        Target::Runtime(reference) => {
            let version = reference.abi_version();

            fields.extend([
                text_field("call_target_runtime_role", reference.role().as_str()),
                count_field("call_target_runtime_abi_major", u64::from(version.major())),
                count_field("call_target_runtime_abi_minor", u64::from(version.minor())),
            ]);
        }
        Target::Indirect { callee, .. } => push_mir_operand(fields, callee),
    }
}

fn push_mir_operand(fields: &mut Vec<DiagnosticFailureField>, operand: &bray_ir::MirOperand) {
    use bray_ir::MirOperand as Operand;

    match operand {
        Operand::Value(value) => {
            fields.push(text_field("call_target_callee_kind", "value"));
            fields.push(identity_field("call_target_callee_value", value));
        }
        Operand::Constant { value, ty } => {
            fields.push(text_field("call_target_callee_kind", "constant"));
            fields.push(identity_field("call_target_callee_value", value));
            fields.push(identity_field("call_target_callee_type", ty));
        }
        Operand::Immediate { value, ty } => {
            fields.push(text_field(
                "call_target_callee_kind",
                match value {
                    bray_ir::MirImmediateValue::Boolean(false) => "boolean_false",
                    bray_ir::MirImmediateValue::Boolean(true) => "boolean_true",
                    bray_ir::MirImmediateValue::Unit => "unit",
                    bray_ir::MirImmediateValue::NullableAbsent => "nullable_absent",
                },
            ));

            fields.push(identity_field("call_target_callee_type", ty));
        }
        Operand::Copy(place) | Operand::Move(place) => {
            fields.push(text_field(
                "call_target_callee_kind",
                if matches!(operand, Operand::Copy(_)) {
                    "copy"
                } else {
                    "move"
                },
            ));

            fields.push(identity_field("call_target_callee_place", place));
            fields.push(identity_field("call_target_callee_type", &place.ty()));
        }
    }
}

fn push_mir_frame_reference(
    fields: &mut Vec<DiagnosticFailureField>,
    frame: bray_ir::MirFrameReference,
) {
    match frame {
        bray_ir::MirFrameReference::Known(frame) => {
            fields.push(text_field("helper_frame_kind", "known"));

            fields.push(DiagnosticFailureField::new(
                "helper_frame",
                DiagnosticFailureValue::Identity(frame.digest()),
            ));
        }
        bray_ir::MirFrameReference::Erased => {
            fields.push(text_field("helper_frame_kind", "erased"));
        }
    }
}

const fn mir_standard_library_helper(helper: bray_ir::MirStandardLibraryHelper) -> &'static str {
    use bray_ir::MirStandardLibraryHelper as Helper;

    match helper {
        Helper::MemoryAllocate => "memory_allocate",
        Helper::MemoryDeallocate => "memory_deallocate",
        Helper::StringScalarCount => "string_scalar_count",
        Helper::StringEquals => "string_equals",
        Helper::StringScalarAt => "string_scalar_at",
        Helper::StringScalarSlice => "string_scalar_slice",
        Helper::StringFromUtf8 => "string_from_utf8",
        Helper::CharacterScalarValue => "character_scalar_value",
        Helper::CharacterFromScalarValue => "character_from_scalar_value",
        Helper::CharacterUtf8Length => "character_utf8_length",
        Helper::CharacterUtf8Byte => "character_utf8_byte",
        Helper::CharacterIsAlphabetic => "character_is_alphabetic",
        Helper::CharacterIsNumeric => "character_is_numeric",
        Helper::CharacterIsWhitespace => "character_is_whitespace",
    }
}

const fn mir_cleanup_phase(phase: bray_ir::MirCleanupPhase) -> &'static str {
    match phase {
        bray_ir::MirCleanupPhase::TaskCancellation => "task_cancellation",
        bray_ir::MirCleanupPhase::LifecycleResolution => "lifecycle_resolution",
    }
}

const fn callable_abi(abi: bray_symbols::CallableAbi) -> &'static str {
    match abi {
        bray_symbols::CallableAbi::Bray => "bray",
        bray_symbols::CallableAbi::C => "c",
        bray_symbols::CallableAbi::System => "system",
    }
}
