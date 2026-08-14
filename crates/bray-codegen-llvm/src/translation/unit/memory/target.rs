use bray_bound_tree::{InlineAssemblyContract, PointerAddressComparison};
use bray_codegen::CodegenFailure;
use bray_ir::{MirMemoryOperation, MirOperation};
use bray_symbols::{ConstantValueId, ConstantValueKind};
use bray_target::{InlineAssemblyOptions, TargetArchitecture, TargetControlFacts};
use inkwell::InlineAsmDialect;
use inkwell::types::{BasicMetadataTypeEnum, BasicType};
use inkwell::values::{BasicMetadataValueEnum, BasicValue, BasicValueEnum};

use super::super::core::UnitTranslator;
use super::super::support::{int_value, llvm};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_volatile_read(
        &mut self,
        memory: &MirMemoryOperation,
        pointee: bray_symbols::TypeId,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [pointer] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let pointer = self.memory_pointer(pointer)?;

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

    pub(super) fn translate_volatile_write(
        &mut self,
        memory: &MirMemoryOperation,
    ) -> Result<(), CodegenFailure> {
        let [pointer, value] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let pointer = self.memory_pointer(pointer)?;
        let value = self.operand(value)?;
        let store = llvm(self.builder.build_store(pointer, value))?;

        store
            .set_volatile(true)
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)
    }

    pub(super) fn translate_expose_address(
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

    pub(super) fn translate_from_exposed_address(
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

        llvm(self.builder.build_int_to_ptr(
            address,
            pointer,
            "pointer.reconstructed.address",
        ))
        .map(BasicValueEnum::from)
    }

    pub(super) fn translate_address_comparison(
        &mut self,
        memory: &MirMemoryOperation,
        comparison: PointerAddressComparison,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [left, right] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let left = self.memory_pointer(left).and_then(|value| self.pointer_address(value))?;
        let right = self.memory_pointer(right).and_then(|value| self.pointer_address(value))?;

        let predicate = match comparison {
            PointerAddressComparison::Equal => inkwell::IntPredicate::EQ,
            PointerAddressComparison::Less => inkwell::IntPredicate::ULT,
        };

        llvm(self.builder.build_int_compare(
            predicate,
            left,
            right,
            "pointer.address.compare",
        ))
        .map(BasicValueEnum::from)
    }

    pub(super) fn translate_compiler_fence(&mut self) -> Result<(), CodegenFailure> {
        self.translate_empty_assembly("", "~{memory}", true)
    }

    pub(super) fn translate_catastrophic_abort(&mut self) -> Result<(), CodegenFailure> {
        self.call_void_intrinsic("llvm.trap")
    }

    pub(super) fn translate_debugger_trap(&mut self) -> Result<(), CodegenFailure> {
        self.call_void_intrinsic("llvm.debugtrap")
    }

    pub(super) fn translate_spin_loop_hint(&mut self) -> Result<(), CodegenFailure> {
        let instruction = match self.request.target().profile().machine().architecture() {
            TargetArchitecture::X86 | TargetArchitecture::X86_64 => "pause",
            TargetArchitecture::Arm | TargetArchitecture::Aarch64 => "yield",
            TargetArchitecture::Riscv32 | TargetArchitecture::Riscv64 => "",
            TargetArchitecture::PowerPc64 => "or 27,27,27",
            TargetArchitecture::Wasm32 | TargetArchitecture::Wasm64 => "",
        };

        self.translate_empty_assembly(instruction, "", true)
    }

    pub(super) fn translate_target_feature(
        &self,
        feature: ConstantValueId,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let feature = self.constant_string(feature)?;
        let control = TargetControlFacts::for_profile(self.request.target().profile());

        Ok(self
            .types
            .context()
            .bool_type()
            .const_int(u64::from(control.supports_feature(feature)), false)
            .into())
    }

    pub(super) fn translate_inline_assembly(
        &mut self,
        operation: &MirOperation,
        memory: &MirMemoryOperation,
        contract: InlineAssemblyContract,
        diverges: bool,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let [_, _, _, _, _, input] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let template = self.constant_string(contract.template())?.to_owned();
        let constraints = self.constant_string(contract.constraints())?.to_owned();
        let clobbers = self.constant_string(contract.clobbers())?.to_owned();
        let control = TargetControlFacts::for_profile(self.request.target().profile());
        let mut constraints = assembly_constraints(control, &constraints)?;

        for clobber in clobbers.split(',').map(str::trim).filter(|value| !value.is_empty()) {
            if let Some(abi) = clobber.strip_prefix("abi:") {
                let registers = control
                    .abi_clobbers(abi)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                for register in registers {
                    append_clobber(&mut constraints, register);
                }
            } else {
                append_clobber(&mut constraints, clobber);
            }
        }

        let options = InlineAssemblyOptions::try_new(self.constant_integer(contract.options())?)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let input_type = self.types.map(memory.operand_types()[5])?;
        let parameter_types = [BasicMetadataTypeEnum::from(input_type)];

        let function_type = if diverges {
            self.types.context().void_type().fn_type(&parameter_types, false)
        } else {
            self.types
                .map(self.operation_result_type(operation)?)?
                .fn_type(&parameter_types, false)
        };

        let dialect = options
            .intel_dialect()
            .then_some(InlineAsmDialect::Intel);

        let assembly = self.types.context().create_inline_asm(
            function_type,
            template,
            constraints,
            !options.pure(),
            options.aligned_stack(),
            dialect,
            options.may_unwind(),
        );

        let input = self.operand(input)?;
        let arguments = [BasicMetadataValueEnum::from(input)];

        let call = llvm(self.builder.build_indirect_call(
            function_type,
            assembly,
            &arguments,
            "target.inline_assembly",
        ))?;

        Ok(call.try_as_basic_value().basic())
    }

    fn translate_empty_assembly(
        &mut self,
        instruction: &str,
        constraints: &str,
        side_effects: bool,
    ) -> Result<(), CodegenFailure> {
        let function_type = self.types.context().void_type().fn_type(&[], false);

        let assembly = self.types.context().create_inline_asm(
            function_type,
            instruction.to_owned(),
            constraints.to_owned(),
            side_effects,
            false,
            None,
            false,
        );

        llvm(self.builder.build_indirect_call(
            function_type,
            assembly,
            &[],
            "target.control",
        ))?;

        Ok(())
    }

    fn call_void_intrinsic(&mut self, name: &str) -> Result<(), CodegenFailure> {
        let function = match self.module.get_function(name) {
            Some(function) => function,
            None => self.module.add_function(
                name,
                self.types.context().void_type().fn_type(&[], false),
                None,
            ),
        };

        llvm(self.builder.build_call(function, &[], "target.termination"))?;

        Ok(())
    }

    fn constant_string(
        &self,
        value: ConstantValueId,
    ) -> Result<&str, CodegenFailure> {
        let mapping = self
            .request
            .mappings()
            .constant_data(value)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let ConstantValueKind::String(value) = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        Ok(value)
    }

    fn constant_integer(&self, value: ConstantValueId) -> Result<u64, CodegenFailure> {
        let mapping = self
            .request
            .mappings()
            .constant_data(value)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let ConstantValueKind::Integer(value) = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        value
            .to_u64()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)
    }
}

fn append_clobber(constraints: &mut String, clobber: &str) {
    if !constraints.is_empty() {
        constraints.push(',');
    }

    constraints.push_str("~{");
    constraints.push_str(clobber);
    constraints.push('}');
}

fn assembly_constraints(
    control: TargetControlFacts,
    constraints: &str,
) -> Result<String, CodegenFailure> {
    let mut normalized = String::new();

    for constraint in constraints.split(',').map(str::trim).filter(|value| !value.is_empty()) {
        if !normalized.is_empty() {
            normalized.push(',');
        }

        let (modifiers, class, tied_input) = if let Some(class) = constraint.strip_prefix("+&") {
            ("=&", class, true)
        } else if let Some(class) = constraint.strip_prefix('+') {
            ("=", class, true)
        } else if let Some(class) = constraint.strip_prefix("=&") {
            ("=&", class, false)
        } else if let Some(class) = constraint.strip_prefix('=') {
            ("=", class, false)
        } else {
            ("", constraint, false)
        };

        normalized.push_str(modifiers);

        if class.starts_with('{') || matches!(class, "r" | "i" | "s" | "m") {
            normalized.push_str(class);
        } else {
            normalized.push_str(
                control
                    .register_constraint(class)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?,
            );
        }

        if tied_input {
            normalized.push_str(",0");
        }
    }

    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use inkwell::context::Context;
    use bray_target::{TargetArchitecture, TargetControlFacts};

    use super::assembly_constraints;

    #[test]
    fn input_output_constraints_lower_to_an_output_and_tied_input() {
        let control = TargetControlFacts::for_architecture(TargetArchitecture::X86_64);

        assert_eq!(
            assembly_constraints(control, "+reg"),
            Ok("=r,0".to_owned())
        );

        assert_eq!(
            assembly_constraints(control, "+&{rax}"),
            Ok("=&{rax},0".to_owned())
        );
    }

    #[test]
    fn tied_input_output_inline_assembly_verifies() {
        let context = Context::create();
        let module = context.create_module("target.control.test");
        let builder = context.create_builder();
        let integer = context.i32_type();
        let function_type = integer.fn_type(&[integer.into()], false);

        let assembly = context.create_inline_asm(
            function_type,
            String::new(),
            "=r,0".to_owned(),
            false,
            false,
            None,
            false,
        );

        let function = module.add_function("test", function_type, None);
        let block = context.append_basic_block(function, "entry");

        builder.position_at_end(block);

        let input = function
            .get_first_param()
            .unwrap_or_else(|| panic!("test function must have an input"));

        let call = builder
            .build_indirect_call(function_type, assembly, &[input.into()], "assembly")
            .unwrap_or_else(|error| panic!("inline assembly call must build: {error}"));

        let output = call
            .try_as_basic_value()
            .basic()
            .unwrap_or_else(|| panic!("inline assembly call must have an output"));

        builder
            .build_return(Some(&output))
            .unwrap_or_else(|error| panic!("test return must build: {error}"));

        let ir = module.print_to_string().to_string();

        assert!(module.verify().is_ok(), "{ir}");
    }
}
