use bray_codegen::{
    CodegenFailure, CodegenIndirectParameterKind, CodegenIntegerExtension, CodegenLinkage,
    CodegenMappings, CodegenParameterMapping, CodegenResultMapping, CodegenSymbolMapping,
    CodegenTarget, CodegenValueAttribute,
};
use inkwell::DLLStorageClass;
use inkwell::attributes::{Attribute, AttributeLoc};
use inkwell::module::{Linkage, Module};
use inkwell::types::AnyType;
use inkwell::values::FunctionValue;

use super::LlvmTypeMappings;

pub(crate) fn declare_symbols<'context>(
    module: &Module<'context>,
    mappings: &CodegenMappings,
    target: &CodegenTarget,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<(), CodegenFailure> {
    for mapping in mappings.symbols() {
        let function_type = types.function_type(mapping.signature())?;
        let function = module.add_function(mapping.name().as_str(), function_type, None);

        apply_linkage(function, mapping, target)?;
        function.set_call_conventions(call_convention(mapping, target)?);
        apply_signature_attributes(function, mapping, types)?;
    }

    Ok(())
}

fn apply_linkage(
    function: FunctionValue<'_>,
    mapping: &CodegenSymbolMapping,
    target: &CodegenTarget,
) -> Result<(), CodegenFailure> {
    let linkage = match mapping.linkage() {
        CodegenLinkage::Private => Linkage::Private,
        CodegenLinkage::Internal => Linkage::Internal,
        CodegenLinkage::External | CodegenLinkage::Import | CodegenLinkage::Export => {
            Linkage::External
        }
        CodegenLinkage::Weak => Linkage::WeakAny,
        CodegenLinkage::LinkOnce => Linkage::LinkOnceODR,
        CodegenLinkage::Common => return Err(CodegenFailure::UnsupportedTarget),
    };

    function.set_linkage(linkage);

    if target.machine().object_format() == bray_target::ObjectFormat::Coff {
        match mapping.linkage() {
            CodegenLinkage::Import => function
                .as_global_value()
                .set_dll_storage_class(DLLStorageClass::Import),
            CodegenLinkage::Export => function
                .as_global_value()
                .set_dll_storage_class(DLLStorageClass::Export),
            CodegenLinkage::Private
            | CodegenLinkage::Internal
            | CodegenLinkage::External
            | CodegenLinkage::Weak
            | CodegenLinkage::LinkOnce
            | CodegenLinkage::Common => {}
        }
    }

    Ok(())
}

fn apply_signature_attributes(
    function: FunctionValue<'_>,
    mapping: &CodegenSymbolMapping,
    types: &mut LlvmTypeMappings<'_, '_>,
) -> Result<(), CodegenFailure> {
    let signature = mapping.signature();
    let mut parameter_index = 0_u32;

    match signature.result() {
        CodegenResultMapping::Void => {}
        CodegenResultMapping::Direct {
            extension,
            attributes,
            ..
        } => {
            apply_extension(function, AttributeLoc::Return, *extension, types)?;
            apply_value_attributes(function, AttributeLoc::Return, attributes, types)?;
        }
        CodegenResultMapping::Indirect {
            pointee,
            alignment,
            attributes,
            ..
        } => {
            apply_type_attribute(function, AttributeLoc::Param(0), "sret", *pointee, types)?;
            apply_alignment(function, AttributeLoc::Param(0), alignment.get(), types)?;
            apply_value_attributes(function, AttributeLoc::Param(0), attributes, types)?;

            parameter_index = 1;
        }
    }

    for parameter in signature.parameters() {
        let location = AttributeLoc::Param(parameter_index);

        match parameter {
            CodegenParameterMapping::Ignore => continue,
            CodegenParameterMapping::Direct {
                extension,
                attributes,
                ..
            } => {
                apply_extension(function, location, *extension, types)?;
                apply_value_attributes(function, location, attributes, types)?;
            }
            CodegenParameterMapping::Indirect {
                pointee,
                kind,
                alignment,
                attributes,
                ..
            } => {
                if *kind == CodegenIndirectParameterKind::ByValue {
                    apply_type_attribute(function, location, "byval", *pointee, types)?;
                }

                apply_alignment(function, location, alignment.get(), types)?;
                apply_value_attributes(function, location, attributes, types)?;
            }
        }

        parameter_index = parameter_index
            .checked_add(1)
            .ok_or(CodegenFailure::UnsupportedTarget)?;
    }

    Ok(())
}

fn apply_extension(
    function: FunctionValue<'_>,
    location: AttributeLoc,
    extension: Option<CodegenIntegerExtension>,
    types: &LlvmTypeMappings<'_, '_>,
) -> Result<(), CodegenFailure> {
    let name = match extension {
        Some(CodegenIntegerExtension::Sign) => "signext",
        Some(CodegenIntegerExtension::Zero) => "zeroext",
        None => return Ok(()),
    };

    apply_enum_attribute(function, location, name, 0, types)
}

fn apply_value_attributes(
    function: FunctionValue<'_>,
    location: AttributeLoc,
    attributes: &[CodegenValueAttribute],
    types: &LlvmTypeMappings<'_, '_>,
) -> Result<(), CodegenFailure> {
    for attribute in attributes {
        let name = match attribute {
            CodegenValueAttribute::InRegister => "inreg",
            CodegenValueAttribute::NoAlias => "noalias",
            CodegenValueAttribute::NonNull => "nonnull",
            CodegenValueAttribute::NoUndef => "noundef",
        };

        apply_enum_attribute(function, location, name, 0, types)?;
    }

    Ok(())
}

fn apply_alignment(
    function: FunctionValue<'_>,
    location: AttributeLoc,
    alignment: u64,
    types: &LlvmTypeMappings<'_, '_>,
) -> Result<(), CodegenFailure> {
    apply_enum_attribute(function, location, "align", alignment, types)
}

fn apply_enum_attribute(
    function: FunctionValue<'_>,
    location: AttributeLoc,
    name: &str,
    value: u64,
    types: &LlvmTypeMappings<'_, '_>,
) -> Result<(), CodegenFailure> {
    let kind = Attribute::get_named_enum_kind_id(name);

    if kind == 0 {
        return Err(CodegenFailure::UnsupportedTarget);
    }

    function.add_attribute(location, types.context().create_enum_attribute(kind, value));

    Ok(())
}

fn apply_type_attribute(
    function: FunctionValue<'_>,
    location: AttributeLoc,
    name: &str,
    pointee: bray_symbols::TypeId,
    types: &mut LlvmTypeMappings<'_, '_>,
) -> Result<(), CodegenFailure> {
    let kind = Attribute::get_named_enum_kind_id(name);

    if kind == 0 {
        return Err(CodegenFailure::UnsupportedTarget);
    }

    let pointee = types.map(pointee)?.as_any_type_enum();
    let attribute = types.context().create_type_attribute(kind, pointee);

    function.add_attribute(location, attribute);

    Ok(())
}

fn call_convention(
    mapping: &CodegenSymbolMapping,
    target: &CodegenTarget,
) -> Result<u32, CodegenFailure> {
    let Some(convention) = target.abi().convention(mapping.signature().abi()) else {
        return Err(CodegenFailure::UnsupportedTarget);
    };

    match convention.as_str() {
        "bray-x86_64" | "c" | "ccc" => Ok(0),
        "sysv64" => Ok(78),
        "win64" => Ok(79),
        _ => Err(CodegenFailure::UnsupportedTarget),
    }
}

#[cfg(test)]
mod tests {
    use std::num::{NonZeroU16, NonZeroU64};

    use bray_codegen::test_support::codegen_request;
    use bray_codegen::{
        CodegenCallableSignature, CodegenIndirectParameterKind, CodegenIntegerExtension,
        CodegenLinkage, CodegenMappings, CodegenParameterMapping, CodegenResultMapping,
        CodegenSymbolMapping, CodegenTypeKind, CodegenTypeMapping, CodegenValueAttribute,
        TargetAddressSpaceKind, TargetScalarKind,
    };
    use bray_symbols::{CallableAbi, SemanticValueStore, TypeData};
    use bray_target::{TargetLayoutContract, TargetValueLayout};
    use inkwell::context::Context;
    use inkwell::module::Linkage;

    use super::{apply_linkage, declare_symbols};
    use crate::machine::LlvmTargetMachine;
    use crate::mapping::LlvmTypeMappings;

    #[test]
    fn ordinary_weak_symbols_use_non_odr_llvm_linkage() {
        let fixture = codegen_request();
        let request = fixture.request();
        let mapping = &request.mappings().symbols()[0];

        let weak = CodegenSymbolMapping::new(
            mapping.key().clone(),
            mapping.name().clone(),
            CodegenLinkage::Weak,
            mapping.signature().clone(),
        );

        let context = Context::create();
        let module = context.create_module("weak");

        let function = module.add_function(
            weak.name().as_str(),
            context.void_type().fn_type(&[], false),
            None,
        );

        assert_eq!(apply_linkage(function, &weak, request.target()), Ok(()));
        assert_eq!(function.get_linkage(), Linkage::WeakAny);
    }

    #[test]
    fn declarations_apply_target_classified_pass_modes_and_attributes() {
        let fixture = codegen_request();
        let request = fixture.request();
        let mappings = request.mappings();

        let Ok(store) = SemanticValueStore::try_new() else {
            panic!("test semantic value store must be available");
        };

        let scalar = intern_type(&store, TypeData::Error);
        let pointer = intern_type(&store, TypeData::Slice(scalar));
        let four = NonZeroU64::new(4).unwrap_or(NonZeroU64::MIN);
        let eight = NonZeroU64::new(8).unwrap_or(NonZeroU64::MIN);
        let width = NonZeroU16::new(32).unwrap_or(NonZeroU16::MIN);

        let types = [
            CodegenTypeMapping::new(
                scalar,
                layout(4, four),
                CodegenTypeKind::Scalar(TargetScalarKind::Integer(width)),
            ),
            CodegenTypeMapping::new(
                pointer,
                layout(8, eight),
                CodegenTypeKind::Pointer {
                    target: scalar,
                    address_space: TargetAddressSpaceKind::Default,
                },
            ),
        ];

        let signature = CodegenCallableSignature::new(
            [
                CodegenParameterMapping::Ignore,
                CodegenParameterMapping::direct(
                    scalar,
                    Some(CodegenIntegerExtension::Sign),
                    [CodegenValueAttribute::NoUndef],
                ),
                CodegenParameterMapping::indirect(
                    pointer,
                    scalar,
                    CodegenIndirectParameterKind::ByValue,
                    four,
                    [CodegenValueAttribute::NonNull],
                ),
            ],
            CodegenResultMapping::indirect(pointer, scalar, four, [CodegenValueAttribute::NoAlias]),
            CallableAbi::Bray,
            true,
        );

        let symbols = mappings.symbols().iter().map(|mapping| {
            CodegenSymbolMapping::new(
                mapping.key().clone(),
                mapping.name().clone(),
                mapping.linkage(),
                signature.clone(),
            )
        });

        let Ok(mappings) = CodegenMappings::try_new(
            request.unit(),
            request.target(),
            mappings.types().iter().cloned().chain(types),
            symbols,
            mappings.constants().iter().cloned(),
            mappings.constant_terms().iter().copied(),
            mappings.callables().iter().cloned(),
            mappings.operations().iter().cloned(),
            mappings.terminators().iter().cloned(),
            mappings.debug_locations().iter().cloned(),
        ) else {
            panic!("target-classified test signature must validate");
        };

        let Ok(machine) = LlvmTargetMachine::create(request.target()) else {
            panic!("test target must construct an LLVM machine");
        };

        let target_data = machine.target_data();
        let context = Context::create();
        let module = context.create_module("attributes");
        let mut types = LlvmTypeMappings::new(&context, &mappings, request.target(), &target_data);

        assert_eq!(
            declare_symbols(&module, &mappings, request.target(), &mut types),
            Ok(())
        );

        let declaration = module.print_to_string().to_string();

        assert!(declaration.contains("sret(i32)"));
        assert!(declaration.contains("signext"));
        assert!(declaration.contains("byval(i32)"));
        assert!(declaration.contains("..."));
    }

    fn layout(size: u64, alignment: NonZeroU64) -> TargetValueLayout {
        TargetValueLayout::new(size, alignment, TargetLayoutContract::Default)
    }

    fn intern_type(store: &SemanticValueStore, data: TypeData) -> bray_symbols::TypeId {
        match store.intern_type(data) {
            Ok(ty) => ty,
            Err(error) => panic!("test type must intern: {error:?}"),
        }
    }
}
