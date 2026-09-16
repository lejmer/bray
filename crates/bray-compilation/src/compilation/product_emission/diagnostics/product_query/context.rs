use bray_diagnostics::DiagnosticFailureField;

use super::mir::push_mir_helper;
use crate::compilation::ProductQueryContext;
use crate::fact::diagnostic_context::{
    count_field, identity_field, natural_field, product_kind, push_symbol, text_field,
    text_list_field,
};

pub(super) fn product_query_context(context: &ProductQueryContext) -> Vec<DiagnosticFailureField> {
    use ProductQueryContext as Context;

    let mut fields = Vec::new();

    let kind = match context {
        Context::Product(product) => {
            fields.push(text_field("product_kind", product_kind(*product)));

            "product"
        }
        Context::Function(function) => {
            push_symbol(&mut fields, "symbol_kind", "symbol", (*function).into());

            "function"
        }
        Context::Symbol(symbol) => {
            push_symbol(&mut fields, "symbol_kind", "symbol", *symbol);

            "symbol"
        }
        Context::Declaration(declaration) => {
            fields.push(identity_field("declaration", declaration));

            "declaration"
        }
        Context::Container(container) => {
            fields.push(identity_field("container", container));

            "container"
        }
        Context::ModulePath { owner, path } => {
            push_symbol(
                &mut fields,
                "module_owner_kind",
                "module_owner",
                owner.into_any(),
            );

            fields.push(text_list_field("module_path_segments", path.segments()));

            if let Some(anchor) = path.recovery_anchor() {
                fields.push(identity_field("module_path_recovery_anchor", &anchor));
            }

            "module_path"
        }
        Context::SymbolKey(key) => {
            fields.push(identity_field("symbol_key", key));

            "symbol_key"
        }
        Context::Source(source) => {
            fields.push(count_field("source", u64::from(source.raw())));

            "source"
        }
        Context::Target(target) => {
            push_codegen_target(&mut fields, target);

            "target"
        }
        Context::Instance(instance) => {
            fields.push(identity_field("codegen_instance", instance));

            "instance"
        }
        Context::CallSite(site) => {
            push_codegen_call_site(&mut fields, *site);

            "call_site"
        }
        Context::CodegenUnit(unit) => {
            fields.push(identity_field("codegen_unit", unit));

            "codegen_unit"
        }
        Context::CodegenStatic(value) => {
            fields.push(identity_field("codegen_static", value));

            "codegen_static"
        }
        Context::CallableData(data) => {
            fields.push(identity_field("callable_data", data));

            "callable_data"
        }
        Context::Implementation(implementation) => {
            fields.push(identity_field("implementation", implementation));

            "implementation"
        }
        Context::StaticReference(reference) => {
            fields.push(identity_field("static_reference", reference));

            "static_reference"
        }
        Context::Substitution(substitution) => {
            fields.push(identity_field("substitution", substitution));

            "substitution"
        }
        Context::GenericOwner(owner) => {
            fields.push(identity_field("generic_owner", owner));

            "generic_owner"
        }
        Context::Type(ty) => {
            fields.push(identity_field("semantic_type", ty));

            "type"
        }
        Context::ImplementationRequirement(requirement) => {
            fields.push(identity_field("implementation_requirement", requirement));

            "implementation_requirement"
        }
        Context::MirUnit(unit) => {
            fields.push(identity_field("mir_unit", unit));

            "mir_unit"
        }
        Context::MirHelper(helper) => {
            push_mir_helper(&mut fields, helper);

            "mir_helper"
        }
        Context::UnaryRepresentation { role, argument } => {
            fields.extend([
                text_field("representation_role", role.as_str()),
                identity_field("representation_argument", argument),
            ]);

            "unary_representation"
        }
        Context::Operation {
            instance,
            operation,
        } => {
            fields.push(identity_field("codegen_instance", instance));
            push_mir_operation_identity(&mut fields, *operation);

            "operation"
        }
        Context::CallableDefinition(definition) => {
            push_symbol(
                &mut fields,
                "callable_definition_kind",
                "callable_definition",
                definition.symbol(),
            );

            "callable_definition"
        }
        Context::SourceLocation { source, offset } => {
            fields.extend([
                count_field("source", u64::from(source.raw())),
                count_field("source_offset", u64::from(offset.bytes())),
            ]);

            "source_location"
        }
        Context::CompilerKnownRepresentation(role) => {
            fields.push(text_field("representation_role", role.as_str()));

            "compiler_known_representation"
        }
        Context::CompilerKnownDeclaration(declaration) => {
            fields.push(text_field(
                "compiler_known_declaration",
                declaration.as_str(),
            ));

            "compiler_known_declaration"
        }
    };

    fields.insert(0, text_field("product_context_kind", kind));

    fields
}

fn push_codegen_target(
    fields: &mut Vec<DiagnosticFailureField>,
    target: &bray_codegen::CodegenTarget,
) {
    let machine = target.machine();

    fields.extend([
        text_field("target_triple", target.triple()),
        text_field("target_identity", target.identity().as_str()),
        text_field("target_architecture", machine.architecture().as_str()),
        text_field("target_object_format", machine.object_format().as_str()),
        text_field(
            "target_endianness",
            crate::fact::diagnostic_context::target_endianness(machine.endianness()),
        ),
        count_field(
            "target_pointer_width_bits",
            u64::from(machine.pointer_width_bits().get()),
        ),
        count_field(
            "target_pointer_alignment_bytes",
            u64::from(machine.pointer_alignment_bytes().get()),
        ),
        count_field(
            "target_stack_alignment_bytes",
            u64::from(machine.stack_alignment_bytes().get()),
        ),
        text_list_field(
            "target_scalar_layouts",
            target.data_layout().scalars().iter().map(|layout| {
                format!(
                    "{}:{}:{}",
                    target_scalar_kind(layout.kind()),
                    layout.size_bytes(),
                    layout.alignment_bytes(),
                )
            }),
        ),
        count_field(
            "target_aggregate_alignment_bytes",
            u64::from(target.data_layout().aggregate_alignment_bytes().get()),
        ),
        text_list_field(
            "target_address_spaces",
            target.data_layout().address_spaces().iter().map(|space| {
                format!(
                    "{}:{}",
                    target_address_space_kind(space.kind()),
                    space.number(),
                )
            }),
        ),
        text_list_field(
            "target_callable_abis",
            target.abi().mappings().iter().map(|mapping| {
                format!(
                    "{}:{}",
                    callable_abi(mapping.abi()),
                    mapping.convention().as_str(),
                )
            }),
        ),
        text_field("target_panic_abi", target.panic_abi().as_str()),
        text_field(
            "target_global_symbol_prefix",
            target.symbols().global_prefix(),
        ),
        text_field(
            "target_private_symbol_prefix",
            target.symbols().private_prefix(),
        ),
        text_list_field(
            "target_supported_linkages",
            target
                .symbols()
                .supported_linkages()
                .iter()
                .map(|linkage| codegen_linkage(*linkage).to_owned()),
        ),
        text_field(
            "target_profile_revision",
            target.compatibility().target_profile_revision(),
        ),
        count_field(
            "target_codegen_contract_revision",
            u64::from(target.compatibility().codegen_contract_revision()),
        ),
        text_list_field(
            "target_backend_requirements",
            target
                .compatibility()
                .backend_requirements()
                .map(str::to_owned),
        ),
        text_field(
            "target_relocation_model",
            relocation_model(target.relocation_model()),
        ),
        text_field("target_code_model", code_model(target.code_model())),
        text_field("target_cpu", target.cpu()),
        text_list_field("target_features", target.features().map(str::to_owned)),
    ]);

    crate::fact::push_selected_target_properties(fields, target.profile().properties());
}

fn target_scalar_kind(value: bray_codegen::TargetScalarKind) -> String {
    match value {
        bray_codegen::TargetScalarKind::Boolean => "boolean".to_owned(),
        bray_codegen::TargetScalarKind::Integer(width) => format!("integer_{}", width.get()),
        bray_codegen::TargetScalarKind::Float(width) => format!("float_{}", width.get()),
    }
}

const fn target_address_space_kind(value: bray_codegen::TargetAddressSpaceKind) -> &'static str {
    use bray_codegen::TargetAddressSpaceKind as Kind;

    match value {
        Kind::Default => "default",
        Kind::Function => "function",
        Kind::Global => "global",
        Kind::Constant => "constant",
        Kind::Stack => "stack",
        Kind::Heap => "heap",
        Kind::Device => "device",
    }
}

const fn callable_abi(value: bray_symbols::CallableAbi) -> &'static str {
    match value {
        bray_symbols::CallableAbi::Bray => "bray",
        bray_symbols::CallableAbi::C => "c",
        bray_symbols::CallableAbi::System => "system",
    }
}

const fn codegen_linkage(value: bray_codegen::CodegenLinkage) -> &'static str {
    use bray_codegen::CodegenLinkage as Linkage;

    match value {
        Linkage::Private => "private",
        Linkage::Internal => "internal",
        Linkage::External => "external",
        Linkage::Weak => "weak",
        Linkage::Fallback => "fallback",
        Linkage::LinkOnce => "link_once",
        Linkage::Common => "common",
        Linkage::Import => "import",
        Linkage::Export => "export",
    }
}

const fn relocation_model(value: bray_target::RelocationModel) -> &'static str {
    match value {
        bray_target::RelocationModel::Default => "default",
        bray_target::RelocationModel::Static => "static",
        bray_target::RelocationModel::PositionIndependent => "position_independent",
        bray_target::RelocationModel::DynamicNoPic => "dynamic_no_pic",
    }
}

const fn code_model(value: bray_target::CodeModel) -> &'static str {
    match value {
        bray_target::CodeModel::Default => "default",
        bray_target::CodeModel::Tiny => "tiny",
        bray_target::CodeModel::Small => "small",
        bray_target::CodeModel::Medium => "medium",
        bray_target::CodeModel::Large => "large",
        bray_target::CodeModel::Kernel => "kernel",
    }
}

fn push_codegen_call_site(
    fields: &mut Vec<DiagnosticFailureField>,
    site: bray_codegen::CodegenCallSite,
) {
    use bray_codegen::CodegenCallSite as Site;

    match site {
        Site::Operation(operation) => {
            fields.push(text_field("call_site_kind", "operation"));
            push_mir_operation_identity(fields, operation);
        }
        Site::Terminator(block) => {
            fields.push(text_field("call_site_kind", "terminator"));

            fields.extend([
                count_field("mir_unit", u64::from(block.unit().raw())),
                count_field("mir_block", u64::from(block.slot())),
            ]);
        }
        Site::InlineAssemblyOperation { operation, symbol } => {
            fields.push(text_field("call_site_kind", "inline_assembly_operation"));
            push_mir_operation_identity(fields, operation);
            fields.push(natural_field("assembly_symbol", symbol));
        }
        Site::InlineAssemblyTerminator { block, symbol } => {
            fields.push(text_field("call_site_kind", "inline_assembly_terminator"));

            fields.extend([
                count_field("mir_unit", u64::from(block.unit().raw())),
                count_field("mir_block", u64::from(block.slot())),
                natural_field("assembly_symbol", symbol),
            ]);
        }
    }
}

fn push_mir_operation_identity(
    fields: &mut Vec<DiagnosticFailureField>,
    operation: bray_ir::MirOperationId,
) {
    fields.extend([
        count_field("mir_unit", u64::from(operation.unit().raw())),
        count_field("mir_operation", u64::from(operation.slot())),
    ]);
}

#[cfg(test)]
mod tests {
    use super::product_query_context;
    use crate::compilation::ProductQueryContext;

    #[test]
    fn target_context_preserves_machine_layout_abi_and_selection() {
        let target =
            bray_codegen::CodegenTarget::for_native(bray_target::NativeTarget::X86_64WindowsMsvc);

        let fields = product_query_context(&ProductQueryContext::Target(target));
        let names: Vec<_> = fields.iter().map(|field| field.name()).collect();

        for required in [
            "target_architecture",
            "target_scalar_layouts",
            "target_address_spaces",
            "target_callable_abis",
            "target_panic_abi",
            "target_code_model",
            "target_relocation_model",
            "target_features",
            "target_property_name",
        ] {
            assert!(names.contains(&required), "missing target field {required}");
        }
    }
}
