use bray_diagnostics::{DiagnosticFailureField, DiagnosticFailureValue};

use super::mir::push_mir_helper;
use crate::compilation::ProductQueryContext;
use crate::fact::diagnostic_context::{
    count_field, identity_field, natural_field, push_symbol, text_field,
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

            fields.push(DiagnosticFailureField::new(
                "module_path_segments",
                DiagnosticFailureValue::TextList(
                    path.segments()
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                ),
            ));

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
            fields.extend([
                text_field("target_triple", target.triple()),
                identity_field("target_identity", target.identity()),
                text_field("target_cpu", target.cpu()),
            ]);

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
        Context::MirOperation { source, operation } => {
            fields.push(identity_field("mir_source", source));
            push_mir_operation_identity(&mut fields, *operation);

            "mir_operation"
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

const fn product_kind(kind: bray_symbols::ProductKind) -> &'static str {
    match kind {
        bray_symbols::ProductKind::Executable => "executable",
        bray_symbols::ProductKind::Library => "library",
        bray_symbols::ProductKind::Test => "test",
    }
}
