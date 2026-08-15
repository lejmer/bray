use bray_bound_tree::{MemoryOrder, PointerAddressComparison, VolatileAddressSpace};
use bray_codegen::CodegenFailure;
use bray_ir::{MirMemoryOperation, MirOperation};
use bray_symbols::ConstantValueId;
use bray_target::{TargetArchitecture, TargetControlSupport};
use inkwell::values::{BasicValue, BasicValueEnum};

use super::super::super::core::UnitTranslator;
use super::super::super::support::{int_value, llvm};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(in crate::translation::unit::memory) fn translate_volatile_read(
        &mut self,
        memory: &MirMemoryOperation,
        pointee: bray_symbols::TypeId,
        address_space: VolatileAddressSpace,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [pointer] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let pointer = self.checked_volatile_pointer(pointer, address_space)?;

        let value = llvm(self.builder.build_load(
            self.types.map(pointee)?,
            pointer,
            "memory.volatile.read",
        ))?;

        value
            .as_instruction_value()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?
            .set_volatile(true)
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        Ok(value)
    }

    pub(in crate::translation::unit::memory) fn translate_volatile_write(
        &mut self,
        memory: &MirMemoryOperation,
        address_space: VolatileAddressSpace,
    ) -> Result<(), CodegenFailure> {
        let [pointer, value] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let pointer = self.checked_volatile_pointer(pointer, address_space)?;
        let value = self.operand(value)?;
        let store = llvm(self.builder.build_store(pointer, value))?;

        store
            .set_volatile(true)
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)
    }

    pub(in crate::translation::unit::memory) fn translate_expose_address(
        &mut self,
        memory: &MirMemoryOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [pointer] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let pointer = self.memory_pointer(pointer)?;

        llvm(self.builder.build_ptr_to_int(
            pointer,
            self.pointer_integer_type(),
            "pointer.exposed.address",
        ))
        .map(BasicValueEnum::from)
    }

    pub(in crate::translation::unit::memory) fn translate_from_exposed_address(
        &mut self,
        operation: &MirOperation,
        memory: &MirMemoryOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [address] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let address = self
            .operand(address)
            .and_then(|value| int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant))?;

        let result = self.operation_result_type(operation)?;

        let inkwell::types::BasicTypeEnum::PointerType(pointer) = self.types.map(result)? else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        llvm(
            self.builder
                .build_int_to_ptr(address, pointer, "pointer.reconstructed.address"),
        )
        .map(BasicValueEnum::from)
    }

    pub(in crate::translation::unit::memory) fn translate_address_comparison(
        &mut self,
        memory: &MirMemoryOperation,
        comparison: PointerAddressComparison,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [left, right] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let left = self
            .memory_pointer(left)
            .and_then(|value| self.pointer_address(value))?;

        let right = self
            .memory_pointer(right)
            .and_then(|value| self.pointer_address(value))?;

        let predicate = match comparison {
            PointerAddressComparison::Equal => inkwell::IntPredicate::EQ,
            PointerAddressComparison::Less => inkwell::IntPredicate::ULT,
        };

        llvm(
            self.builder
                .build_int_compare(predicate, left, right, "pointer.address.compare"),
        )
        .map(BasicValueEnum::from)
    }

    pub(in crate::translation::unit::memory) fn translate_fence(
        &mut self,
        order: MemoryOrder,
        compiler_only: bool,
    ) -> Result<(), CodegenFailure> {
        if order == MemoryOrder::Relaxed {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let order = llvm_memory_order(order);

        llvm(self.builder.build_fence(order, compiler_only, ""))?;

        Ok(())
    }

    fn checked_volatile_pointer(
        &mut self,
        operand: &bray_ir::MirOperand,
        address_space: VolatileAddressSpace,
    ) -> Result<inkwell::values::PointerValue<'context>, CodegenFailure> {
        let pointer = self.memory_pointer(operand)?;

        let role = match address_space {
            VolatileAddressSpace::Host => bray_codegen::TargetAddressSpaceKind::Default,
            VolatileAddressSpace::Device => bray_codegen::TargetAddressSpaceKind::Device,
        };

        let expected = self
            .request
            .target()
            .data_layout()
            .address_space(role)
            .ok_or(CodegenFailure::UnsupportedTarget)?;

        let expected = inkwell::AddressSpace::try_from(expected)
            .map_err(|()| CodegenFailure::UnsupportedTarget)?;

        if pointer.get_type().get_address_space() != expected {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        Ok(pointer)
    }

    pub(in crate::translation::unit::memory) fn translate_catastrophic_abort(
        &mut self,
    ) -> Result<(), CodegenFailure> {
        self.call_void_intrinsic("llvm.trap")
    }

    pub(in crate::translation::unit::memory) fn translate_debugger_trap(
        &mut self,
    ) -> Result<(), CodegenFailure> {
        self.call_void_intrinsic("llvm.debugtrap")
    }

    pub(in crate::translation::unit::memory) fn translate_spin_loop_hint(
        &mut self,
    ) -> Result<(), CodegenFailure> {
        let instruction = match self.request.target().profile().machine().architecture() {
            TargetArchitecture::X86 | TargetArchitecture::X86_64 => "pause",
            TargetArchitecture::Arm | TargetArchitecture::Aarch64 => "yield",
            TargetArchitecture::Riscv32 | TargetArchitecture::Riscv64 => "",
            TargetArchitecture::PowerPc64 => "or 27,27,27",
            TargetArchitecture::Wasm32 | TargetArchitecture::Wasm64 => "",
        };

        self.translate_empty_assembly(instruction, "", true)
    }

    pub(in crate::translation::unit::memory) fn translate_target_feature(
        &self,
        feature: ConstantValueId,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let feature = self.constant_string(feature)?;
        let control = TargetControlSupport::for_profile(self.request.target().profile());

        Ok(self
            .types
            .context()
            .bool_type()
            .const_int(u64::from(control.supports_feature(feature)), false)
            .into())
    }
}

pub(in crate::translation::unit::memory) const fn llvm_memory_order(
    order: MemoryOrder,
) -> inkwell::AtomicOrdering {
    match order {
        MemoryOrder::Relaxed => inkwell::AtomicOrdering::Monotonic,
        MemoryOrder::Acquire => inkwell::AtomicOrdering::Acquire,
        MemoryOrder::Release => inkwell::AtomicOrdering::Release,
        MemoryOrder::AcquireRelease => inkwell::AtomicOrdering::AcquireRelease,
        MemoryOrder::SequentiallyConsistent => inkwell::AtomicOrdering::SequentiallyConsistent,
    }
}
