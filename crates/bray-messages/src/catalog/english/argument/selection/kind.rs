use bray_diagnostics::{
    DiagnosticAlignmentKind, DiagnosticCallableAbi, DiagnosticTargetRepresentation,
};

pub(crate) const fn format_english_callable_abi(abi: DiagnosticCallableAbi) -> &'static str {
    match abi {
        DiagnosticCallableAbi::Bray => "Bray",
        DiagnosticCallableAbi::C => "C",
        DiagnosticCallableAbi::System => "system",
    }
}

pub(crate) const fn format_english_alignment_kind(kind: DiagnosticAlignmentKind) -> &'static str {
    match kind {
        DiagnosticAlignmentKind::Storage => "storage",
        DiagnosticAlignmentKind::Allocation => "allocation",
        DiagnosticAlignmentKind::CallableAbi => "callable ABI",
    }
}

pub(crate) const fn format_english_target_representation(
    kind: DiagnosticTargetRepresentation,
) -> &'static str {
    match kind {
        DiagnosticTargetRepresentation::Bool => "bool",
        DiagnosticTargetRepresentation::Char => "char",
        DiagnosticTargetRepresentation::I8 => "i8",
        DiagnosticTargetRepresentation::I16 => "i16",
        DiagnosticTargetRepresentation::I32 => "i32",
        DiagnosticTargetRepresentation::I64 => "i64",
        DiagnosticTargetRepresentation::I128 => "i128",
        DiagnosticTargetRepresentation::U8 => "u8",
        DiagnosticTargetRepresentation::U16 => "u16",
        DiagnosticTargetRepresentation::U32 => "u32",
        DiagnosticTargetRepresentation::U64 => "u64",
        DiagnosticTargetRepresentation::U128 => "u128",
        DiagnosticTargetRepresentation::Isize => "isize",
        DiagnosticTargetRepresentation::Usize => "usize",
        DiagnosticTargetRepresentation::R16 => "r16",
        DiagnosticTargetRepresentation::R32 => "r32",
        DiagnosticTargetRepresentation::R64 => "r64",
        DiagnosticTargetRepresentation::R128 => "r128",
        DiagnosticTargetRepresentation::C32 => "c32",
        DiagnosticTargetRepresentation::C64 => "c64",
        DiagnosticTargetRepresentation::C128 => "c128",
        DiagnosticTargetRepresentation::C256 => "c256",
        DiagnosticTargetRepresentation::RawPointer => "raw pointer",
        DiagnosticTargetRepresentation::AbiQualifiedCallable => "ABI-qualified callable",
        DiagnosticTargetRepresentation::DefaultLayoutAggregate => "default-layout aggregate",
        DiagnosticTargetRepresentation::StableLayoutAggregate => "stable-layout aggregate",
        DiagnosticTargetRepresentation::CLayoutAggregate => "C-layout aggregate",
        DiagnosticTargetRepresentation::TransparentLayoutAggregate => {
            "transparent-layout aggregate"
        }
    }
}

pub(crate) const fn format_english_selection_kind(
    kind: bray_diagnostics::DiagnosticSelectionKind,
) -> &'static str {
    use bray_diagnostics::DiagnosticSelectionKind;

    match kind {
        DiagnosticSelectionKind::Callable => "callable",
        DiagnosticSelectionKind::Member => "member",
        DiagnosticSelectionKind::Operator => "operator",
        DiagnosticSelectionKind::Index => "index operation",
        DiagnosticSelectionKind::Construction => "construction operation",
        DiagnosticSelectionKind::Conversion => "conversion",
        DiagnosticSelectionKind::Implementation => "implementation",
        DiagnosticSelectionKind::IterationSource => "iteration source",
    }
}
