use bray_bound_tree::{
    CheckedMemoryOperationKind, MemoryAddressKind, MemoryCopyKind, MemoryLayoutQueryKind,
    MemoryOffsetUnit,
};
use bray_compiler_known::ImplementationHook;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, DiagnosticMemoryOperation, SeverityKind,
};
use bray_source::SourceSpan;

use crate::diagnostic::diagnostic_id;

pub(crate) fn add_target_memory_operation_unavailable(
    span: SourceSpan,
    target: &str,
    operation: DiagnosticMemoryOperation,
    diagnostics: &mut DiagnosticBag,
) {
    diagnostics.add(
        Diagnostic::new(
            diagnostic_id(diagnostics.len()),
            DiagnosticKind::CheckingTargetMemoryOperationUnavailable,
            SeverityKind::Error,
        )
        .with_primary_span(span)
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::UnsupportedTargetRequirement,
            span,
        ))
        .with_arg(DiagnosticArg::target_triple(target))
        .with_arg(DiagnosticArg::memory_operation(operation)),
    );
}

pub(crate) const fn diagnostic_memory_operation(
    hook: ImplementationHook,
) -> Option<DiagnosticMemoryOperation> {
    if let Some(operation) = crate::atomic::diagnostic_hook(hook) {
        return Some(operation);
    }

    use DiagnosticMemoryOperation as Operation;
    use ImplementationHook as Hook;

    Some(match hook {
        Hook::AddressOf => Operation::AddressOf,
        Hook::AddressOfMut => Operation::MutableAddressOf,
        Hook::RawPointerNull => Operation::NullPointer,
        Hook::RawPointerIsNull => Operation::PointerNullCheck,
        Hook::RawPointerOffset => Operation::PointerElementOffset,
        Hook::RawPointerByteOffset => Operation::PointerByteOffset,
        Hook::RawPointerReinterpret => Operation::PointerReinterpretation,
        Hook::CallbackState => Operation::CallbackState,
        Hook::RawPointerRead => Operation::PointerRead,
        Hook::RawPointerWrite => Operation::PointerWrite,
        Hook::MemoryCopy => Operation::MemoryCopy,
        Hook::MemoryCopyOverlapping => Operation::OverlappingMemoryCopy,
        Hook::MemorySizeOf => Operation::SizeDetermination,
        Hook::MemoryAlignOf => Operation::AlignmentDetermination,
        Hook::MemoryStrideOf => Operation::StrideDetermination,
        Hook::MemoryLayoutOf => Operation::LayoutDetermination,
        Hook::RawAllocate => Operation::RawAllocation,
        Hook::RawDeallocate => Operation::RawDeallocation,
        Hook::Allocate => Operation::Allocation,
        Hook::Deallocate => Operation::Deallocation,
        Hook::RawBufferCapacity => Operation::RawBufferCapacity,
        Hook::RawBufferInitializedCount => Operation::RawBufferInitializedCount,
        Hook::RawBufferPointer => Operation::RawBufferPointer,
        Hook::RawBufferInitializedSlice => Operation::RawBufferInitializedSlice,
        Hook::RawBufferInitializedSliceMut => Operation::MutableRawBufferInitializedSlice,
        Hook::RawBufferSparePointer => Operation::RawBufferSparePointer,
        Hook::RawBufferSetInitializedCount => Operation::RawBufferSetInitializedCount,
        Hook::RawBufferRelease => Operation::RawBufferRelease,
        Hook::RawBufferReplace => Operation::RawBufferReplace,
        Hook::RawBufferRelocate => Operation::RawBufferRelocate,
        Hook::ByteBufferFill => Operation::ByteBufferFill,
        Hook::ByteSliceCopy => Operation::ByteSliceCopy,
        Hook::ByteBufferRead => Operation::ByteBufferRead,
        Hook::SliceLength => Operation::SliceLength,
        Hook::VolatileLoad | Hook::DeviceVolatileLoad => Operation::VolatileRead,
        Hook::VolatileStore | Hook::DeviceVolatileStore => Operation::VolatileWrite,
        Hook::PointerExposeAddress => Operation::PointerExposeAddress,
        Hook::PointerFromExposedAddress => Operation::PointerFromExposedAddress,
        Hook::PointerAddressEqual | Hook::PointerAddressLess => Operation::PointerAddressComparison,
        Hook::CompilerFence => Operation::CompilerFence,
        Hook::HardwareFence => Operation::HardwareFence,
        Hook::CatastrophicAbort => Operation::CatastrophicAbort,
        Hook::DebuggerTrap => Operation::DebuggerTrap,
        Hook::UnreachableTermination => Operation::UnreachableTermination,
        Hook::SpinLoopHint => Operation::SpinLoopHint,
        Hook::TargetFeatureEnabled => Operation::TargetFeatureCheck,
        Hook::InlineAssembly | Hook::DivergingInlineAssembly | Hook::BranchingInlineAssembly => {
            Operation::InlineAssembly
        }
        _ => return None,
    })
}

pub(crate) const fn diagnostic_checked_memory_operation(
    kind: CheckedMemoryOperationKind,
) -> DiagnosticMemoryOperation {
    if let Some(operation) = crate::atomic::diagnostic_checked_operation(kind) {
        return operation;
    }

    use DiagnosticMemoryOperation as Operation;

    match kind {
        CheckedMemoryOperationKind::Address {
            kind: MemoryAddressKind::Shared,
            ..
        } => Operation::AddressOf,
        CheckedMemoryOperationKind::Address {
            kind: MemoryAddressKind::Mutable,
            ..
        } => Operation::MutableAddressOf,
        CheckedMemoryOperationKind::Null { .. } => Operation::NullPointer,
        CheckedMemoryOperationKind::IsNull { .. } => Operation::PointerNullCheck,
        CheckedMemoryOperationKind::Offset {
            unit: MemoryOffsetUnit::Element,
            ..
        } => Operation::PointerElementOffset,
        CheckedMemoryOperationKind::Offset {
            unit: MemoryOffsetUnit::Byte,
            ..
        } => Operation::PointerByteOffset,
        CheckedMemoryOperationKind::Reinterpret { .. } => Operation::PointerReinterpretation,
        CheckedMemoryOperationKind::Read { .. } => Operation::PointerRead,
        CheckedMemoryOperationKind::Write { .. } => Operation::PointerWrite,
        CheckedMemoryOperationKind::Copy {
            kind: MemoryCopyKind::NonOverlapping,
            ..
        } => Operation::MemoryCopy,
        CheckedMemoryOperationKind::Copy {
            kind: MemoryCopyKind::Overlapping,
            ..
        } => Operation::OverlappingMemoryCopy,
        CheckedMemoryOperationKind::LayoutQuery {
            kind: MemoryLayoutQueryKind::Size,
            ..
        } => Operation::SizeDetermination,
        CheckedMemoryOperationKind::LayoutQuery {
            kind: MemoryLayoutQueryKind::Alignment,
            ..
        } => Operation::AlignmentDetermination,
        CheckedMemoryOperationKind::LayoutQuery {
            kind: MemoryLayoutQueryKind::Stride,
            ..
        } => Operation::StrideDetermination,
        CheckedMemoryOperationKind::LayoutQuery {
            kind: MemoryLayoutQueryKind::Layout,
            ..
        } => Operation::LayoutDetermination,
        CheckedMemoryOperationKind::RawAllocate => Operation::RawAllocation,
        CheckedMemoryOperationKind::RawDeallocate => Operation::RawDeallocation,
        CheckedMemoryOperationKind::Allocate => Operation::Allocation,
        CheckedMemoryOperationKind::Deallocate => Operation::Deallocation,
        CheckedMemoryOperationKind::RawBufferCapacity => Operation::RawBufferCapacity,
        CheckedMemoryOperationKind::RawBufferInitializedCount => {
            Operation::RawBufferInitializedCount
        }
        CheckedMemoryOperationKind::RawBufferPointer => Operation::RawBufferPointer,
        CheckedMemoryOperationKind::RawBufferInitializedSlice => {
            Operation::RawBufferInitializedSlice
        }
        CheckedMemoryOperationKind::RawBufferInitializedSliceMut => {
            Operation::MutableRawBufferInitializedSlice
        }
        CheckedMemoryOperationKind::RawBufferSparePointer { .. } => {
            Operation::RawBufferSparePointer
        }
        CheckedMemoryOperationKind::RawBufferSetInitializedCount => {
            Operation::RawBufferSetInitializedCount
        }
        CheckedMemoryOperationKind::RawBufferRelease { .. } => Operation::RawBufferRelease,
        CheckedMemoryOperationKind::RawBufferReplace { .. } => Operation::RawBufferReplace,
        CheckedMemoryOperationKind::RawBufferRelocate { .. } => Operation::RawBufferRelocate,
        CheckedMemoryOperationKind::ByteBufferFill => Operation::ByteBufferFill,
        CheckedMemoryOperationKind::ByteSliceCopy => Operation::ByteSliceCopy,
        CheckedMemoryOperationKind::ByteBufferRead => Operation::ByteBufferRead,
        CheckedMemoryOperationKind::SliceLength => Operation::SliceLength,
        CheckedMemoryOperationKind::CallbackState { .. } => Operation::CallbackState,
        CheckedMemoryOperationKind::VolatileRead { .. } => Operation::VolatileRead,
        CheckedMemoryOperationKind::VolatileWrite { .. } => Operation::VolatileWrite,
        CheckedMemoryOperationKind::ExposeAddress { .. } => Operation::PointerExposeAddress,
        CheckedMemoryOperationKind::FromExposedAddress { .. } => {
            Operation::PointerFromExposedAddress
        }
        CheckedMemoryOperationKind::CompareAddress { .. } => Operation::PointerAddressComparison,
        CheckedMemoryOperationKind::Fence {
            compiler_only: true,
            ..
        } => Operation::CompilerFence,
        CheckedMemoryOperationKind::Fence {
            compiler_only: false,
            ..
        } => Operation::HardwareFence,
        CheckedMemoryOperationKind::CatastrophicAbort => Operation::CatastrophicAbort,
        CheckedMemoryOperationKind::DebuggerTrap => Operation::DebuggerTrap,
        CheckedMemoryOperationKind::UnreachableTermination => Operation::UnreachableTermination,
        CheckedMemoryOperationKind::SpinLoopHint => Operation::SpinLoopHint,
        CheckedMemoryOperationKind::TargetFeatureEnabled { .. } => Operation::TargetFeatureCheck,
        CheckedMemoryOperationKind::InlineAssembly { .. } => Operation::InlineAssembly,
        CheckedMemoryOperationKind::AtomicInitialize { .. }
        | CheckedMemoryOperationKind::AtomicLoad { .. }
        | CheckedMemoryOperationKind::AtomicStore { .. }
        | CheckedMemoryOperationKind::AtomicExchange { .. }
        | CheckedMemoryOperationKind::AtomicCompareExchange { .. }
        | CheckedMemoryOperationKind::AtomicFetch { .. }
        | CheckedMemoryOperationKind::AtomicWait { .. }
        | CheckedMemoryOperationKind::AtomicNotify { .. } => {
            panic!("atomic diagnostic operation must be classified above")
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{CheckedMemoryOperationKind, MemoryOrder};
    use bray_compiler_known::ImplementationHook;

    use super::{diagnostic_checked_memory_operation, diagnostic_memory_operation};

    #[test]
    fn compiler_and_hardware_fences_keep_distinct_structured_categories() {
        let compiler = diagnostic_memory_operation(ImplementationHook::CompilerFence)
            .unwrap_or_else(|| panic!("compiler fence must have a diagnostic category"));

        let hardware = diagnostic_memory_operation(ImplementationHook::HardwareFence)
            .unwrap_or_else(|| panic!("hardware fence must have a diagnostic category"));

        assert_eq!(compiler.as_str(), "compiler_fence");
        assert_eq!(hardware.as_str(), "hardware_fence");
        assert_ne!(compiler, hardware);

        assert_eq!(
            diagnostic_checked_memory_operation(CheckedMemoryOperationKind::Fence {
                compiler_only: true,
                order: MemoryOrder::SequentiallyConsistent,
            }),
            compiler
        );

        assert_eq!(
            diagnostic_checked_memory_operation(CheckedMemoryOperationKind::Fence {
                compiler_only: false,
                order: MemoryOrder::SequentiallyConsistent,
            }),
            hardware
        );
    }
}
