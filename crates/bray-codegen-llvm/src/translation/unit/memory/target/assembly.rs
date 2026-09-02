use bray_bound_tree::{InlineAssemblyContract, InlineAssemblyOperand, InlineAssemblyOperandKind};
use bray_codegen::{CodegenCallSite, CodegenFailure, CodegenTypeKind};
use bray_ir::{
    MirBlockId, MirCallableReference, MirInlineAssemblyTerminator, MirMemoryOperation, MirOperand,
    MirOperation, MirOperationId,
};
use bray_symbols::{ConstantValueId, ConstantValueKind};
use bray_target::{InlineAssemblyOptions, TargetControlSupport};
use inkwell::InlineAsmDialect;
use inkwell::attributes::AttributeLoc;
use inkwell::basic_block::BasicBlock;
use inkwell::types::{BasicMetadataTypeEnum, BasicType};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum};

use crate::callbr::{CallBrError, build_callbr};
use crate::mapping::type_attribute;

use super::super::super::core::UnitTranslator;
use super::super::super::support::{extract_value, insert_value, llvm};
use super::constraint::{append_clobber, assembly_constraints, output_descriptors};

#[derive(Clone, Copy)]
enum AssemblySite {
    Operation(MirOperationId),
    Terminator(MirBlockId),
}

impl AssemblySite {
    const fn call_site(self, symbol: usize) -> CodegenCallSite {
        match self {
            Self::Operation(operation) => {
                CodegenCallSite::InlineAssemblyOperation { operation, symbol }
            }
            Self::Terminator(block) => CodegenCallSite::InlineAssemblyTerminator { block, symbol },
        }
    }
}

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(in crate::translation::unit::memory) fn translate_inline_assembly(
        &mut self,
        operation_id: MirOperationId,
        operation: &MirOperation,
        memory: &MirMemoryOperation,
        contract: InlineAssemblyContract,
        diverges: bool,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let [inputs] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let output = if diverges {
            None
        } else {
            Some(self.operation_result_type(operation)?)
        };

        self.translate_structural_assembly(
            contract,
            inputs,
            output,
            memory.inline_assembly_symbols(),
            AssemblySite::Operation(operation_id),
            None,
        )
    }

    pub(in crate::translation::unit) fn translate_inline_assembly_terminator(
        &mut self,
        block: MirBlockId,
        assembly: &MirInlineAssemblyTerminator,
    ) -> Result<(), CodegenFailure> {
        let normal = self
            .unit
            .block(assembly.normal())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let [parameter] = normal.parameters() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let phi = self
            .phis
            .get(parameter)
            .copied()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let normal = self.block(assembly.normal())?;

        let fallthrough = self
            .types
            .context()
            .append_basic_block(self.function, "target.inline_assembly.fallthrough");

        let alternates = assembly
            .alternates()
            .iter()
            .map(|alternate| self.block(*alternate))
            .collect::<Result<Vec<_>, _>>()?;

        let output = self
            .translate_structural_assembly(
                assembly.contract(),
                assembly.inputs(),
                Some(assembly.output_type()),
                assembly.symbols(),
                AssemblySite::Terminator(block),
                Some((fallthrough, &alternates)),
            )?
            .unwrap_or(self.types.map(assembly.output_type())?.const_zero());

        llvm(self.builder.build_unconditional_branch(normal))?;
        phi.add_incoming(&[(&output, fallthrough)]);

        Ok(())
    }

    fn translate_structural_assembly(
        &mut self,
        contract: InlineAssemblyContract,
        inputs: &MirOperand,
        output_type: Option<bray_symbols::TypeId>,
        symbols: &[MirCallableReference],
        site: AssemblySite,
        destinations: Option<(BasicBlock<'context>, &[BasicBlock<'context>])>,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let input_type = self.operand_type(inputs)?;
        let input_value = self.operand(inputs)?;
        let template = self.constant_string(contract.template())?.to_owned();
        let constraint_text = self.constant_string(contract.constraints())?.to_owned();
        let clobbers = self.constant_string(contract.clobbers())?.to_owned();
        let control = TargetControlSupport::for_profile(self.request.target().profile());
        let descriptors = contract.operands().collect::<Vec<_>>();
        let mut constraints = assembly_constraints(control, &constraint_text, &descriptors)?;

        for clobber in clobbers
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
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

        let mut parameter_types = Vec::new();
        let mut arguments = Vec::new();
        let mut parameter_attributes = Vec::new();
        let mut symbol_index = 0_usize;

        for descriptor in descriptors.iter().copied() {
            let argument = if descriptor.kind() == InlineAssemblyOperandKind::Immediate {
                let constant = descriptor
                    .constant()
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                self.constant(constant)?
            } else if descriptor.kind() == InlineAssemblyOperandKind::Symbol {
                let reference = symbols
                    .get(symbol_index)
                    .copied()
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let mapping = self
                    .request
                    .mappings()
                    .callable(self.instance.key(), site.call_site(symbol_index))
                    .filter(|mapping| mapping.reference() == reference)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let instance = mapping
                    .instance()
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let symbol = self
                    .request
                    .mappings()
                    .instance_symbol(instance)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let function = self
                    .module
                    .get_function(symbol.name().as_str())
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                symbol_index += 1;

                function.as_global_value().as_pointer_value().into()
            } else if let Some(input) = descriptor.runtime_input() {
                self.structural_operand(input_value, input_type, input)?
            } else {
                continue;
            };

            let parameter =
                crate::conversion::resource_limit(arguments.len(), "assembly_parameter_count")?;

            if descriptor.kind() == InlineAssemblyOperandKind::Memory {
                let target = match self
                    .type_mapping(descriptor.ty())
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?
                    .kind()
                {
                    CodegenTypeKind::Pointer { target, .. } => *target,
                    _ => return Err(CodegenFailure::GeneratedModuleInvariant),
                };

                parameter_attributes.push((
                    parameter,
                    type_attribute("elementtype", target, &mut self.types)?,
                ));
            }

            parameter_types.push(BasicMetadataTypeEnum::from(argument.get_type()));
            arguments.push(BasicMetadataValueEnum::from(argument));
        }

        if symbol_index != symbols.len() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let outputs = output_descriptors(&descriptors);

        let function_type = match outputs.as_slice() {
            [] => self
                .types
                .context()
                .void_type()
                .fn_type(&parameter_types, false),
            [output] => self
                .types
                .map(output.ty())?
                .fn_type(&parameter_types, false),
            outputs => {
                let fields = outputs
                    .iter()
                    .map(|output| self.types.map(output.ty()))
                    .collect::<Result<Vec<_>, _>>()?;

                self.types
                    .context()
                    .struct_type(&fields, false)
                    .fn_type(&parameter_types, false)
            }
        };

        let dialect = options.intel_dialect().then_some(InlineAsmDialect::Intel);

        let side_effects = output_type.is_none() || !options.pure();

        let assembly = self.types.context().create_inline_asm(
            function_type,
            template,
            constraints,
            side_effects,
            options.aligned_stack(),
            dialect,
            options.may_unwind(),
        );

        if destinations.is_some() {
            self.clear_moved_places()?;
        }

        let fallthrough = destinations.map(|(default, _)| default);

        let raw_output = match destinations {
            Some((default, alternates)) => build_callbr(
                &self.builder,
                function_type,
                assembly,
                default,
                alternates,
                &arguments,
                &parameter_attributes,
                "target.inline_assembly",
            )
            .map_err(|error| match error {
                CallBrError::ResourceLimit { resource, actual } => {
                    CodegenFailure::resource_limit(resource, actual)
                }
                error => CodegenFailure::generated_module_invariant(error),
            })?,
            None => {
                let call = llvm(self.builder.build_indirect_call(
                    function_type,
                    assembly,
                    &arguments,
                    "target.inline_assembly",
                ))?;

                for &(index, attribute) in &parameter_attributes {
                    call.add_attribute(AttributeLoc::Param(index), attribute);
                }

                call.try_as_basic_value().basic()
            }
        };

        if let Some(fallthrough) = fallthrough {
            self.builder.position_at_end(fallthrough);
        }

        let Some(raw_output) = raw_output else {
            return match output_type {
                Some(output) => Ok(Some(self.types.map(output)?.const_zero())),
                None => Ok(None),
            };
        };

        let output_type = output_type.ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let output = self.assembly_output(raw_output, output_type, &outputs)?;

        Ok(Some(output))
    }

    fn structural_operand(
        &mut self,
        aggregate: BasicValueEnum<'context>,
        aggregate_type: bray_symbols::TypeId,
        index: u16,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let mapping = self
            .type_mapping(aggregate_type)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Aggregate(fields) = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let index = self.aggregate_element(fields, usize::from(index))?;

        extract_value(&self.builder, aggregate, index)
    }

    fn assembly_output(
        &mut self,
        raw: BasicValueEnum<'context>,
        output_type: bray_symbols::TypeId,
        outputs: &[InlineAssemblyOperand],
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let mapped = self.types.map(output_type)?;

        let mapping = self
            .type_mapping(output_type)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Aggregate(fields) = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let mut result = mapped.const_zero();

        for (ordinal, _) in outputs.iter().enumerate() {
            let value = if outputs.len() == 1 {
                raw
            } else {
                extract_value(
                    &self.builder,
                    raw,
                    crate::conversion::resource_limit(ordinal, "assembly_argument_ordinal")?,
                )?
            };

            let destination = self.aggregate_element(fields, ordinal)?;

            let destination = crate::conversion::resource_limit(
                destination,
                "assembly_destination_ordinal",
            )?;

            result = insert_value(&self.builder, result, value, destination)?;
        }

        Ok(result)
    }

    pub(super) fn translate_empty_assembly(
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

        llvm(
            self.builder
                .build_indirect_call(function_type, assembly, &[], "target.control"),
        )?;

        Ok(())
    }

    pub(in crate::translation::unit) fn call_void_intrinsic(
        &mut self,
        name: &str,
    ) -> Result<(), CodegenFailure> {
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

    pub(super) fn constant_string(&self, value: ConstantValueId) -> Result<&str, CodegenFailure> {
        let mapping = self
            .request
            .mappings()
            .constant_data(value)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let ConstantValueKind::String(value) = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        Ok(value.as_ref())
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

#[cfg(test)]
mod tests {
    use bray_bound_tree::{InlineAssemblyOperand, InlineAssemblyOperandKind};
    use bray_symbols::{ConstantValueData, ConstantValueKind, SemanticValueStore, TypeData};
    use bray_target::{TargetArchitecture, TargetControlSupport};
    use inkwell::context::Context;

    use crate::callbr::build_callbr;

    use super::super::constraint::assembly_constraints;

    #[test]
    fn structural_operands_derive_complete_llvm_constraint_order() {
        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("test semantic values must be available: {error:?}"));

        let ty = values
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("test assembly type must intern: {error:?}"));

        let constant = values
            .intern_constant_value(ConstantValueData::new(
                ty,
                ConstantValueKind::Boolean(false),
            ))
            .unwrap_or_else(|error| panic!("test assembly constant must intern: {error:?}"));

        let operands = [
            InlineAssemblyOperand::new(
                InlineAssemblyOperandKind::InOut,
                ty,
                Some(0),
                Some(0),
                Some(0),
                None,
                None,
                0,
                4,
            ),
            InlineAssemblyOperand::new(
                InlineAssemblyOperandKind::EarlyInOut,
                ty,
                Some(1),
                Some(1),
                Some(1),
                None,
                None,
                5,
                5,
            ),
            InlineAssemblyOperand::new(
                InlineAssemblyOperandKind::Output,
                ty,
                None,
                None,
                Some(2),
                None,
                None,
                11,
                5,
            ),
            InlineAssemblyOperand::new(
                InlineAssemblyOperandKind::LateOutput,
                ty,
                None,
                None,
                Some(3),
                None,
                None,
                17,
                4,
            ),
            InlineAssemblyOperand::new(
                InlineAssemblyOperandKind::Input,
                ty,
                Some(2),
                Some(2),
                None,
                None,
                None,
                22,
                3,
            ),
            InlineAssemblyOperand::new(
                InlineAssemblyOperandKind::Immediate,
                ty,
                Some(3),
                None,
                None,
                Some(constant),
                None,
                26,
                1,
            ),
            InlineAssemblyOperand::new(
                InlineAssemblyOperandKind::Symbol,
                ty,
                Some(4),
                None,
                None,
                None,
                None,
                28,
                1,
            ),
            InlineAssemblyOperand::new(
                InlineAssemblyOperandKind::Memory,
                ty,
                Some(5),
                Some(3),
                None,
                None,
                None,
                30,
                1,
            ),
            InlineAssemblyOperand::new(
                InlineAssemblyOperandKind::Label,
                ty,
                None,
                None,
                None,
                None,
                None,
                32,
                5,
            ),
        ];

        let control = TargetControlSupport::for_architecture(TargetArchitecture::X86_64);

        assert_eq!(
            assembly_constraints(control, "+reg,+&reg,=&reg,=reg,reg,i,s,m,label", &operands,),
            Ok(String::from("=r,=&r,=&r,=r,0,1,r,i,s,*m,!i"))
        );

        let explicit = [
            InlineAssemblyOperand::new(
                InlineAssemblyOperandKind::InOut,
                ty,
                Some(0),
                Some(0),
                Some(0),
                None,
                None,
                0,
                6,
            ),
            InlineAssemblyOperand::new(
                InlineAssemblyOperandKind::Input,
                ty,
                Some(1),
                Some(1),
                None,
                None,
                None,
                7,
                5,
            ),
        ];

        assert_eq!(
            assembly_constraints(control, "+{rax},{rax}", &explicit),
            Ok(String::from("={rax},0,{rax}"))
        );

        let input = &explicit[1..];

        assert_eq!(
            assembly_constraints(control, "rax", input),
            Err(bray_codegen::CodegenFailure::GeneratedModuleInvariant)
        );

        assert_eq!(
            assembly_constraints(control, "{reg}", input),
            Err(bray_codegen::CodegenFailure::GeneratedModuleInvariant)
        );

        assert_eq!(
            assembly_constraints(control, "{bogus}", input),
            Err(bray_codegen::CodegenFailure::GeneratedModuleInvariant)
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

    #[test]
    fn callbr_reconstructs_outputs_only_in_its_fallthrough_block() {
        let context = Context::create();
        let module = context.create_module("target.control.callbr.test");
        let builder = context.create_builder();
        let integer = context.i32_type();
        let function_type = integer.fn_type(&[integer.into()], false);

        let assembly = context.create_inline_asm(
            function_type,
            String::new(),
            "=r,0,!i".to_owned(),
            false,
            false,
            None,
            false,
        );

        let function = module.add_function("test", function_type, None);
        let entry = context.append_basic_block(function, "entry");
        let fallthrough = context.append_basic_block(function, "fallthrough");
        let alternate = context.append_basic_block(function, "alternate");
        let normal = context.append_basic_block(function, "normal");

        builder.position_at_end(entry);

        let input = function
            .get_first_param()
            .unwrap_or_else(|| panic!("test function must have an input"));

        let output = build_callbr(
            &builder,
            function_type,
            assembly,
            fallthrough,
            &[alternate],
            &[input.into()],
            &[],
            "assembly",
        )
        .unwrap_or_else(|error| panic!("callbr must build: {error:?}"))
        .unwrap_or_else(|| panic!("callbr must retain its output"));

        builder.position_at_end(fallthrough);

        let outputs = builder
            .build_insert_value(
                context.struct_type(&[integer.into()], false).const_zero(),
                output,
                0,
                "outputs",
            )
            .unwrap_or_else(|error| panic!("output tuple must reconstruct: {error}"))
            .into_struct_value();

        builder
            .build_unconditional_branch(normal)
            .unwrap_or_else(|error| panic!("fallthrough must reach normal continuation: {error}"));

        builder.position_at_end(alternate);

        builder
            .build_unreachable()
            .unwrap_or_else(|error| panic!("alternate test block must terminate: {error}"));

        builder.position_at_end(normal);

        let phi = builder
            .build_phi(outputs.get_type(), "normal.outputs")
            .unwrap_or_else(|error| panic!("normal output phi must build: {error}"));

        phi.add_incoming(&[(&outputs, fallthrough)]);

        let result = builder
            .build_extract_value(phi.as_basic_value().into_struct_value(), 0, "result")
            .unwrap_or_else(|error| panic!("normal output must extract: {error}"));

        builder
            .build_return(Some(&result))
            .unwrap_or_else(|error| panic!("normal continuation must return: {error}"));

        let ir = module.print_to_string().to_string();

        let entry_ir = ir
            .split("entry:")
            .nth(1)
            .and_then(|body| body.split("fallthrough:").next())
            .unwrap_or_else(|| panic!("entry block must be rendered: {ir}"));

        let fallthrough_ir = ir
            .split("fallthrough:")
            .nth(1)
            .and_then(|body| body.split("alternate:").next())
            .unwrap_or_else(|| panic!("fallthrough block must be rendered: {ir}"));

        assert!(module.verify().is_ok(), "{ir}");
        assert!(entry_ir.contains("callbr i32"), "{ir}");
        assert!(!entry_ir.contains("insertvalue"), "{ir}");
        assert!(fallthrough_ir.contains("insertvalue"), "{ir}");
        assert!(fallthrough_ir.contains("br label %normal"), "{ir}");

        assert!(
            ir.contains("phi { i32 } [ %outputs, %fallthrough ]"),
            "{ir}"
        );
    }
}
