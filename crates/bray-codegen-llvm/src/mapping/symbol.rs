use bray_codegen::{
    CodegenFailure, CodegenIndirectParameterKind, CodegenInstance, CodegenIntegerExtension,
    CodegenLinkage, CodegenMappings, CodegenParameterMapping, CodegenResultMapping,
    CodegenSymbolMapping, CodegenTarget, CodegenValueAttribute,
};
use bray_ir::MirUnitKey;
use bray_symbols::SymbolKind;
use inkwell::DLLStorageClass;
use inkwell::GlobalVisibility;
use inkwell::attributes::AttributeLoc;
use inkwell::module::{Linkage, Module};
use inkwell::values::{CallSiteValue, FunctionValue};

use super::LlvmTypeMappings;
use super::attribute::{enum_attribute, type_attribute, value_attribute_name};

pub(crate) fn declare_symbols<'context, 'mappings>(
    module: &Module<'context>,
    mappings: &'mappings CodegenMappings,
    target: &CodegenTarget,
    types: &mut LlvmTypeMappings<'context, 'mappings>,
) -> Result<(), CodegenFailure> {
    for mapping in mappings.symbols() {
        let defines_symbol = match mapping.key() {
            bray_codegen::CodegenSymbolKey::Instance(instance) => {
                mappings.unit().instances().contains(instance)
                    && mapping.linkage() != CodegenLinkage::Import
            }
            bray_codegen::CodegenSymbolKey::Runtime(_)
            | bray_codegen::CodegenSymbolKey::ProtectedFrame { .. } => false,
        };

        declare_symbol(module, mapping, target, defines_symbol, types)?;

        if !defines_symbol {
            declare_native_entry(module, mapping, target, false, types)?;
        }
    }

    super::static_storage::declare_static_storages(module, mappings, target, types)?;

    Ok(())
}

fn declare_symbol<'context>(
    module: &Module<'context>,
    mapping: &CodegenSymbolMapping,
    target: &CodegenTarget,
    defines_symbol: bool,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let native_type = crate::native::symbol_function_type(types.context(), target, mapping.key());

    let function_type = native_type.unwrap_or(types.function_type(mapping.signature())?);
    let function = module.add_function(mapping.name().as_str(), function_type, None);

    apply_linkage(module, function, mapping, target, defines_symbol)?;

    if native_type.is_some() {
        function.set_call_conventions(0);
        apply_native_attributes(function, mapping, target, types)?;
    } else {
        function.set_call_conventions(call_convention(mapping.signature(), target)?);
        apply_signature_attributes(function, mapping, types)?;
    }

    Ok(function)
}

pub(crate) fn declare_native_entry<'context>(
    module: &Module<'context>,
    mapping: &CodegenSymbolMapping,
    target: &CodegenTarget,
    defines_symbol: bool,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<Option<FunctionValue<'context>>, CodegenFailure> {
    let Some(native_entry) = mapping.native_entry() else {
        return Ok(None);
    };

    let entry_mapping = CodegenSymbolMapping::new(
        mapping.key().clone(),
        native_entry.name().clone(),
        native_entry.linkage(),
        mapping.signature().clone(),
    );

    declare_symbol(module, &entry_mapping, target, defines_symbol, types).map(Some)
}

pub(crate) fn apply_instance_optimization_attributes(
    function: FunctionValue<'_>,
    instance: &CodegenInstance,
    types: &LlvmTypeMappings<'_, '_>,
) -> Result<(), CodegenFailure> {
    if is_static_trait_fulfillment(instance.key().template()) {
        apply_enum_attribute(function, AttributeLoc::Function, "inlinehint", 0, types)?;
    }

    Ok(())
}

fn is_static_trait_fulfillment(template: &MirUnitKey) -> bool {
    match template {
        MirUnitKey::Bound(key) => {
            key.declared_owner().kind() == SymbolKind::TraitCallableFulfillment
        }
        MirUnitKey::ImportedExecutable(key) => {
            key.owner().kind() == SymbolKind::TraitCallableFulfillment
        }
        MirUnitKey::ExecutableHost(_)
        | MirUnitKey::GeneratedLifecycle(_)
        | MirUnitKey::CompilerProvidedCallable(_)
        | MirUnitKey::ExternalCallable(_)
        | MirUnitKey::ExternalRuntimeDefault(_) => false,
    }
}

fn apply_native_attributes(
    function: FunctionValue<'_>,
    mapping: &CodegenSymbolMapping,
    target: &CodegenTarget,
    types: &LlvmTypeMappings<'_, '_>,
) -> Result<(), CodegenFailure> {
    if let bray_codegen::CodegenSymbolKey::Runtime(reference) = mapping.key() {
        for (location, attribute) in
            crate::native::runtime_attributes(types.context(), target, reference.role())?
        {
            function.add_attribute(location, attribute);
        }

        return Ok(());
    }

    if !matches!(
        mapping.key(),
        bray_codegen::CodegenSymbolKey::ProtectedFrame {
            operation: bray_runtime_interface::ProtectedFrameOperation::MoveBeforeStart,
            ..
        }
    ) {
        return Ok(());
    }

    let Some(result) = crate::native::indirect_result_type(types.context(), target, mapping.key())
    else {
        return Ok(());
    };

    function.add_attribute(
        AttributeLoc::Param(0),
        crate::native::indirect_result_attribute(types.context(), result)?,
    );

    Ok(())
}

fn apply_linkage(
    module: &Module<'_>,
    function: FunctionValue<'_>,
    mapping: &CodegenSymbolMapping,
    target: &CodegenTarget,
    defines_symbol: bool,
) -> Result<(), CodegenFailure> {
    let linkage = match (mapping.linkage(), defines_symbol) {
        (CodegenLinkage::Weak | CodegenLinkage::Fallback | CodegenLinkage::LinkOnce, false) => {
            Linkage::External
        }
        (CodegenLinkage::Private, _) => Linkage::Private,
        (CodegenLinkage::Internal, _) => Linkage::External,
        (CodegenLinkage::External | CodegenLinkage::Import | CodegenLinkage::Export, _) => {
            Linkage::External
        }
        (CodegenLinkage::Fallback, true)
            if target.machine().object_format() == bray_target::ObjectFormat::Coff =>
        {
            Linkage::WeakODR
        }
        (CodegenLinkage::Weak | CodegenLinkage::Fallback, true) => Linkage::WeakAny,
        (CodegenLinkage::LinkOnce, true) => Linkage::WeakODR,
        (CodegenLinkage::Common, _) => return Err(CodegenFailure::UnsupportedTarget),
    };

    function.set_linkage(linkage);

    if matches!(
        mapping.linkage(),
        CodegenLinkage::Fallback | CodegenLinkage::LinkOnce
    ) && defines_symbol
        && target.machine().object_format() == bray_target::ObjectFormat::Coff
    {
        if mapping.linkage() == CodegenLinkage::LinkOnce {
            crate::comdat::attach_any(
                module,
                function.as_global_value(),
                mapping.name().as_str(),
                target.machine().object_format(),
            );
        } else {
            crate::comdat::attach(
                module,
                function.as_global_value(),
                mapping.name().as_str(),
                target.machine().object_format(),
            );
        }
    }

    if matches!(
        mapping.linkage(),
        CodegenLinkage::Internal | CodegenLinkage::LinkOnce
    ) {
        function
            .as_global_value()
            .set_visibility(GlobalVisibility::Hidden);
    }

    if defines_symbol && target.machine().object_format() == bray_target::ObjectFormat::Coff {
        match mapping.linkage() {
            CodegenLinkage::Export => function
                .as_global_value()
                .set_dll_storage_class(DLLStorageClass::Export),
            CodegenLinkage::Private
            | CodegenLinkage::Internal
            | CodegenLinkage::External
            | CodegenLinkage::Weak
            | CodegenLinkage::Fallback
            | CodegenLinkage::Import
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

pub(crate) fn apply_signature_call_attributes(
    call: CallSiteValue<'_>,
    signature: &bray_codegen::CodegenCallableSignature,
    types: &mut LlvmTypeMappings<'_, '_>,
) -> Result<(), CodegenFailure> {
    let mut parameter_index = 0_u32;

    match signature.result() {
        CodegenResultMapping::Void => {}
        CodegenResultMapping::Direct {
            extension,
            attributes,
            ..
        } => {
            apply_call_extension(call, AttributeLoc::Return, *extension, types)?;
            apply_call_value_attributes(call, AttributeLoc::Return, attributes, types)?;
        }
        CodegenResultMapping::Indirect {
            pointee,
            alignment,
            attributes,
            ..
        } => {
            apply_call_type_attribute(call, AttributeLoc::Param(0), "sret", *pointee, types)?;
            apply_call_alignment(call, AttributeLoc::Param(0), alignment.get(), types)?;
            apply_call_value_attributes(call, AttributeLoc::Param(0), attributes, types)?;

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
                apply_call_extension(call, location, *extension, types)?;
                apply_call_value_attributes(call, location, attributes, types)?;
            }
            CodegenParameterMapping::Indirect {
                pointee,
                kind,
                alignment,
                attributes,
                ..
            } => {
                if *kind == CodegenIndirectParameterKind::ByValue {
                    apply_call_type_attribute(call, location, "byval", *pointee, types)?;
                }

                apply_call_alignment(call, location, alignment.get(), types)?;
                apply_call_value_attributes(call, location, attributes, types)?;
            }
        }

        parameter_index = parameter_index
            .checked_add(1)
            .ok_or(CodegenFailure::UnsupportedTarget)?;
    }

    Ok(())
}

fn apply_call_extension(
    call: CallSiteValue<'_>,
    location: AttributeLoc,
    extension: Option<CodegenIntegerExtension>,
    types: &LlvmTypeMappings<'_, '_>,
) -> Result<(), CodegenFailure> {
    let name = match extension {
        Some(CodegenIntegerExtension::Sign) => "signext",
        Some(CodegenIntegerExtension::Zero) => "zeroext",
        None => return Ok(()),
    };

    apply_call_enum_attribute(call, location, name, 0, types)
}

fn apply_call_value_attributes(
    call: CallSiteValue<'_>,
    location: AttributeLoc,
    attributes: &[CodegenValueAttribute],
    types: &LlvmTypeMappings<'_, '_>,
) -> Result<(), CodegenFailure> {
    for attribute in attributes {
        apply_call_enum_attribute(call, location, value_attribute_name(*attribute), 0, types)?;
    }

    Ok(())
}

fn apply_call_alignment(
    call: CallSiteValue<'_>,
    location: AttributeLoc,
    alignment: u64,
    types: &LlvmTypeMappings<'_, '_>,
) -> Result<(), CodegenFailure> {
    apply_call_enum_attribute(call, location, "align", alignment, types)
}

fn apply_call_enum_attribute(
    call: CallSiteValue<'_>,
    location: AttributeLoc,
    name: &str,
    value: u64,
    types: &LlvmTypeMappings<'_, '_>,
) -> Result<(), CodegenFailure> {
    call.add_attribute(location, enum_attribute(name, value, types.context())?);

    Ok(())
}

fn apply_call_type_attribute(
    call: CallSiteValue<'_>,
    location: AttributeLoc,
    name: &str,
    pointee: bray_symbols::TypeId,
    types: &mut LlvmTypeMappings<'_, '_>,
) -> Result<(), CodegenFailure> {
    call.add_attribute(location, type_attribute(name, pointee, types)?);

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
        apply_enum_attribute(
            function,
            location,
            value_attribute_name(*attribute),
            0,
            types,
        )?;
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
    function.add_attribute(location, enum_attribute(name, value, types.context())?);

    Ok(())
}

fn apply_type_attribute(
    function: FunctionValue<'_>,
    location: AttributeLoc,
    name: &str,
    pointee: bray_symbols::TypeId,
    types: &mut LlvmTypeMappings<'_, '_>,
) -> Result<(), CodegenFailure> {
    function.add_attribute(location, type_attribute(name, pointee, types)?);

    Ok(())
}

pub(crate) fn call_convention(
    signature: &bray_codegen::CodegenCallableSignature,
    target: &CodegenTarget,
) -> Result<u32, CodegenFailure> {
    let Some(convention) = target.abi().convention(signature.abi()) else {
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

    use bray_bound_tree::BoundUnitKey;
    use bray_codegen::test_support::codegen_request;
    use bray_codegen::{
        CodegenCallableSignature, CodegenIndirectParameterKind, CodegenInstance,
        CodegenIntegerExtension, CodegenLinkage, CodegenMappings, CodegenNativeEntryMapping,
        CodegenParameterMapping, CodegenResultMapping, CodegenSymbolMapping, CodegenTarget,
        CodegenTypeKind, CodegenTypeMapping, CodegenValueAttribute, TargetAddressSpaceKind,
    };
    use bray_ir::{
        MirBlockKind, MirExecutableTemplateId, MirFrameDescriptor, MirFrameState, MirFrameStateId,
        MirImportedExecutableKey, MirSourceAnchor, MirTerminatorKind, MirUnitBuilder, MirUnitId,
        MirUnitKey, MirUnitKind,
    };
    use bray_runtime_interface::{
        BinarySymbolName, ProtectedAsyncFrameId, ProtectedFrameAbiVersions, RuntimeAbiVersion,
    };
    use bray_symbols::testing::{intern_type, source_function_key};
    use bray_symbols::{
        CallableAbi, SemanticValueStore, SymbolId, SymbolKey, SymbolKind,
        TraitCallableFulfillmentSymbolId, TypeData,
    };
    use bray_target::{NativeTarget, TargetLayoutContract, TargetValueLayout};
    use inkwell::DLLStorageClass;
    use inkwell::attributes::AttributeLoc;
    use inkwell::context::Context;
    use inkwell::module::Linkage;

    use super::{
        apply_instance_optimization_attributes, apply_linkage, declare_native_entry,
        declare_symbols, is_static_trait_fulfillment,
    };
    use crate::machine::LlvmTargetMachine;
    use crate::mapping::LlvmTypeMappings;

    #[test]
    fn mapped_runtime_declarations_use_native_byte_extension() {
        let fixture = codegen_request();
        let request = fixture.request();
        let context = Context::create();
        let module = context.create_module("mapped.runtime");
        let target = CodegenTarget::for_native(NativeTarget::X86_64LinuxGnu);
        let machine = LlvmTargetMachine::create(&target).unwrap();
        let target_data = machine.target_data();
        let mut types = LlvmTypeMappings::new(&context, request.mappings(), &target, &target_data);
        let role = bray_runtime_interface::RuntimeAbiRole::CurrentRunCancellationObservation;

        let mapping = CodegenSymbolMapping::new(
            bray_codegen::CodegenSymbolKey::Runtime(bray_ir::MirRuntimeReference::new(
                role,
                RuntimeAbiVersion::new(1, 0),
            )),
            BinarySymbolName::try_new(role.native_symbol().unwrap()).unwrap(),
            CodegenLinkage::Import,
            request.mappings().symbols()[0].signature().clone(),
        );

        let function =
            super::declare_symbol(&module, &mapping, &target, false, &mut types).unwrap();

        let zero_extend = inkwell::attributes::Attribute::get_named_enum_kind_id("zeroext");

        assert!(
            function
                .get_enum_attribute(AttributeLoc::Return, zero_extend)
                .is_some()
        );

        module.verify().unwrap();
    }

    #[test]
    fn weak_linkage_is_emitted_only_by_the_defining_unit() {
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

        assert_eq!(
            apply_linkage(&module, function, &weak, request.target(), true),
            Ok(())
        );

        assert_eq!(function.get_linkage(), Linkage::WeakAny);

        assert_eq!(
            function.as_global_value().get_dll_storage_class(),
            DLLStorageClass::Default
        );

        let reference_module = context.create_module("weak-reference");

        let reference = reference_module.add_function(
            weak.name().as_str(),
            context.void_type().fn_type(&[], false),
            None,
        );

        assert_eq!(
            apply_linkage(&reference_module, reference, &weak, request.target(), false,),
            Ok(())
        );

        assert_eq!(reference.get_linkage(), Linkage::External);
    }

    #[test]
    fn referenced_callbacks_declare_their_native_entry() {
        let fixture = codegen_request();
        let request = fixture.request();
        let mapping = &request.mappings().symbols()[0];

        let entry = BinarySymbolName::try_new("native_callback")
            .unwrap_or_else(|| panic!("callback symbol name must validate"));

        let callback = CodegenSymbolMapping::new(
            mapping.key().clone(),
            mapping.name().clone(),
            CodegenLinkage::LinkOnce,
            mapping.signature().clone(),
        )
        .with_native_entry(CodegenNativeEntryMapping::new(
            entry.clone(),
            CodegenLinkage::Export,
        ));

        let Ok(machine) = LlvmTargetMachine::create(request.target()) else {
            panic!("test target must construct an LLVM machine");
        };

        let target_data = machine.target_data();
        let context = Context::create();
        let module = context.create_module("callback-reference");

        let mut types =
            LlvmTypeMappings::new(&context, request.mappings(), request.target(), &target_data);

        let declaration =
            declare_native_entry(&module, &callback, request.target(), false, &mut types)
                .unwrap_or_else(|error| panic!("callback entry must declare: {error:?}"))
                .unwrap_or_else(|| panic!("callback entry must exist"));

        assert_eq!(declaration.get_name().to_str(), Ok(entry.as_str()));
        assert_eq!(declaration.get_linkage(), Linkage::External);
        assert_eq!(declaration.count_basic_blocks(), 0);
    }

    #[test]
    fn coff_fallback_definitions_use_comdat_linkage() {
        let fixture = codegen_request();
        let mapping = &fixture.request().mappings().symbols()[0];

        let fallback = CodegenSymbolMapping::new(
            mapping.key().clone(),
            mapping.name().clone(),
            CodegenLinkage::Fallback,
            mapping.signature().clone(),
        );

        let target = CodegenTarget::for_native(NativeTarget::X86_64WindowsMsvc);
        let context = Context::create();
        let module = context.create_module("coff-fallback");

        let function = module.add_function(
            fallback.name().as_str(),
            context.void_type().fn_type(&[], false),
            None,
        );

        assert_eq!(
            apply_linkage(&module, function, &fallback, &target, true),
            Ok(())
        );

        assert_eq!(function.get_linkage(), Linkage::WeakODR);

        assert!(
            module
                .print_to_string()
                .to_string()
                .contains("comdat exactmatch")
        );
    }

    #[test]
    fn coff_link_once_definitions_use_comdat_linkage() {
        let fixture = codegen_request();
        let mapping = &fixture.request().mappings().symbols()[0];

        let link_once = CodegenSymbolMapping::new(
            mapping.key().clone(),
            mapping.name().clone(),
            CodegenLinkage::LinkOnce,
            mapping.signature().clone(),
        );

        let target = CodegenTarget::for_native(NativeTarget::X86_64WindowsMsvc);
        let context = Context::create();
        let module = context.create_module("coff-link-once");

        let function = module.add_function(
            link_once.name().as_str(),
            context.void_type().fn_type(&[], false),
            None,
        );

        assert_eq!(
            apply_linkage(&module, function, &link_once, &target, true),
            Ok(())
        );

        assert_eq!(function.get_linkage(), Linkage::WeakODR);

        assert!(module.print_to_string().to_string().contains("comdat any"));
    }

    #[test]
    fn deduplicated_symbols_remain_available_to_other_codegen_units() {
        let fixture = codegen_request();
        let request = fixture.request();
        let mapping = &request.mappings().symbols()[0];

        let link_once = CodegenSymbolMapping::new(
            mapping.key().clone(),
            mapping.name().clone(),
            CodegenLinkage::LinkOnce,
            mapping.signature().clone(),
        );

        let context = Context::create();
        let module = context.create_module("link-once");

        let function = module.add_function(
            link_once.name().as_str(),
            context.void_type().fn_type(&[], false),
            None,
        );

        assert_eq!(
            apply_linkage(&module, function, &link_once, request.target(), true),
            Ok(())
        );

        assert_eq!(function.get_linkage(), Linkage::WeakODR);

        assert_eq!(
            function.as_global_value().get_visibility(),
            inkwell::GlobalVisibility::Hidden
        );
    }

    #[test]
    fn coff_imports_remain_ordinary_external_references() {
        let fixture = codegen_request();
        let mapping = &fixture.request().mappings().symbols()[0];

        let imported = CodegenSymbolMapping::new(
            mapping.key().clone(),
            mapping.name().clone(),
            CodegenLinkage::Import,
            mapping.signature().clone(),
        );

        let context = Context::create();
        let module = context.create_module("coff-import");

        let function = module.add_function(
            imported.name().as_str(),
            context.void_type().fn_type(&[], false),
            None,
        );

        let target = CodegenTarget::for_native(NativeTarget::X86_64WindowsMsvc);

        assert_eq!(
            apply_linkage(&module, function, &imported, &target, false),
            Ok(())
        );

        assert_eq!(
            function.as_global_value().get_dll_storage_class(),
            DLLStorageClass::Default
        );
    }

    #[test]
    fn local_trait_fulfillments_are_static_dispatch_candidates() {
        let template = source_template(SymbolKind::TraitCallableFulfillment);

        assert!(is_static_trait_fulfillment(&template));
    }

    #[test]
    fn imported_trait_fulfillments_are_inlining_candidates() {
        let owner = TraitCallableFulfillmentSymbolId::from_symbol_id(SymbolId::new(7));

        let template = MirUnitKey::ImportedExecutable(MirImportedExecutableKey::new(
            owner.into(),
            MirExecutableTemplateId::ROOT,
        ));

        assert!(is_static_trait_fulfillment(&template));
    }

    #[test]
    fn ordinary_functions_keep_the_target_inlining_policy() {
        let template = source_template(SymbolKind::Function);

        assert!(!is_static_trait_fulfillment(&template));
    }

    #[test]
    fn protected_trait_fulfillments_receive_inline_hints() {
        let fixture = codegen_request();
        let request = fixture.request();

        let Ok(machine) = LlvmTargetMachine::create(request.target()) else {
            panic!("test target must construct an LLVM machine");
        };

        let target_data = machine.target_data();
        let context = Context::create();
        let module = context.create_module("instance-attributes");

        let types =
            LlvmTypeMappings::new(&context, request.mappings(), request.target(), &target_data);

        let fulfillment = protected_trait_fulfillment_instance();

        let fulfillment_function = module.add_function(
            "trait_fulfillment",
            context.void_type().fn_type(&[], false),
            None,
        );

        assert_eq!(
            apply_instance_optimization_attributes(fulfillment_function, &fulfillment, &types),
            Ok(())
        );

        let ordinary = CodegenInstance::non_generic(bray_testing::test_mir_unit(8));

        let ordinary_function =
            module.add_function("ordinary", context.void_type().fn_type(&[], false), None);

        assert_eq!(
            apply_instance_optimization_attributes(ordinary_function, &ordinary, &types),
            Ok(())
        );

        let inline_hint = inkwell::attributes::Attribute::get_named_enum_kind_id("inlinehint");

        assert!(
            fulfillment_function
                .get_enum_attribute(AttributeLoc::Function, inline_hint)
                .is_some()
        );

        assert!(
            ordinary_function
                .get_enum_attribute(AttributeLoc::Function, inline_hint)
                .is_none()
        );
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
                CodegenTypeKind::SignedInteger(width),
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
            mappings.instance_types().iter().cloned(),
            symbols,
            mappings.constants().iter().cloned(),
            mappings.constant_terms().iter().cloned(),
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

    fn source_template(kind: SymbolKind) -> MirUnitKey {
        let source_function = source_function_key();

        let Some(declaration) = source_function.source_declaration_id() else {
            panic!("test source function must retain its declaration identity");
        };

        let Some(owner) = SymbolKey::source_declaration(source_function, kind, declaration) else {
            panic!("test callable owner kind must support source declarations");
        };

        let source = bray_testing::test_bound_unit(0).key().source();

        let Some(key) = BoundUnitKey::callable_body(owner, source) else {
            panic!("test callable owner must admit a body");
        };

        MirUnitKey::Bound(key)
    }

    fn protected_trait_fulfillment_instance() -> CodegenInstance {
        let frame = ProtectedAsyncFrameId::new([7; 32]);
        let owner = TraitCallableFulfillmentSymbolId::from_symbol_id(SymbolId::new(9));
        let key = MirImportedExecutableKey::new(owner.into(), MirExecutableTemplateId::ROOT);

        let mut builder = MirUnitBuilder::for_imported_executable(
            MirUnitId::new(9),
            key,
            MirUnitKind::ProtectedAsyncFrame(frame),
            bray_testing::test_mir_target(),
        );

        let source = MirSourceAnchor::imported_executable(key);

        let Ok(entry) = builder.push_block(source.clone(), MirBlockKind::Ordinary) else {
            panic!("test protected-frame block must validate");
        };

        let Ok(()) = builder.set_terminator(entry, source, MirTerminatorKind::Return(None)) else {
            panic!("test protected-frame terminator must validate");
        };

        let state = MirFrameState::new(MirFrameStateId::new(0), entry, [], []);
        let version = RuntimeAbiVersion::new(1, 0);

        let Ok(descriptor) = MirFrameDescriptor::try_new(
            frame,
            version,
            ProtectedFrameAbiVersions::uniform(version),
            bray_testing::test_mir_type(),
            [state],
        ) else {
            panic!("test protected-frame descriptor must validate");
        };

        if let Err(error) = builder.set_frame_descriptor(descriptor) {
            panic!("test protected-frame descriptor must commit: {error:?}");
        }

        let Ok(unit) = builder.finish(entry) else {
            panic!("test protected-frame MIR must validate");
        };

        CodegenInstance::non_generic(unit)
    }
}
