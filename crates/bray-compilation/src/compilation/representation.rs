use bray_compiler_known::RepresentationRole;
use bray_target::TargetScalarKind;

pub(super) const fn target_scalar(role: RepresentationRole) -> Option<TargetScalarKind> {
    match role {
        RepresentationRole::ScalarBool => Some(TargetScalarKind::Bool),
        RepresentationRole::ScalarChar => Some(TargetScalarKind::Char),
        RepresentationRole::ScalarI8 => Some(TargetScalarKind::I8),
        RepresentationRole::ScalarI16 => Some(TargetScalarKind::I16),
        RepresentationRole::ScalarI32 => Some(TargetScalarKind::I32),
        RepresentationRole::ScalarI64 => Some(TargetScalarKind::I64),
        RepresentationRole::ScalarI128 => Some(TargetScalarKind::I128),
        RepresentationRole::ScalarU8 => Some(TargetScalarKind::U8),
        RepresentationRole::ScalarU16 => Some(TargetScalarKind::U16),
        RepresentationRole::ScalarU32 => Some(TargetScalarKind::U32),
        RepresentationRole::ScalarU64 => Some(TargetScalarKind::U64),
        RepresentationRole::ScalarU128 => Some(TargetScalarKind::U128),
        RepresentationRole::ScalarIsize => Some(TargetScalarKind::Isize),
        RepresentationRole::ScalarUsize => Some(TargetScalarKind::Usize),
        RepresentationRole::ScalarR16 => Some(TargetScalarKind::R16),
        RepresentationRole::ScalarR32 => Some(TargetScalarKind::R32),
        RepresentationRole::ScalarR64 => Some(TargetScalarKind::R64),
        RepresentationRole::ScalarR128 => Some(TargetScalarKind::R128),
        RepresentationRole::ScalarC32 => Some(TargetScalarKind::C32),
        RepresentationRole::ScalarC64 => Some(TargetScalarKind::C64),
        RepresentationRole::ScalarC128 => Some(TargetScalarKind::C128),
        RepresentationRole::ScalarC256 => Some(TargetScalarKind::C256),
        RepresentationRole::Unit
        | RepresentationRole::Never
        | RepresentationRole::String
        | RepresentationRole::RawPointer
        | RepresentationRole::DevicePointer
        | RepresentationRole::Result
        | RepresentationRole::RunResult
        | RepresentationRole::PanicReport
        | RepresentationRole::ConversionError
        | RepresentationRole::Future
        | RepresentationRole::Task
        | RepresentationRole::BooleanTrue
        | RepresentationRole::BooleanFalse
        | RepresentationRole::UnitValue
        | RepresentationRole::NoneValue => None,
    }
}
