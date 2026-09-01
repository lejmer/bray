use bray_declarations::DeclarationKind;
use bray_runtime_interface::SourceRoleBinding;
use bray_symbols::FunctionSymbolId;

use super::super::Compilation;
use super::{ForeignQueryFailure, ForeignSourceRole};
use crate::fact::FactQueryError;

pub(in crate::compilation) fn has_source_role(
    compilation: &Compilation,
    function: FunctionSymbolId,
) -> Result<bool, FactQueryError> {
    let runtime = source_role(
        compilation,
        function,
        compilation.runtime_roles(),
        ForeignSourceRole::Runtime,
    )?;

    let platform = source_role(
        compilation,
        function,
        compilation.platform_services(),
        ForeignSourceRole::Platform,
    )?;

    if let (Some(runtime), Some(platform)) = (runtime, platform) {
        return Err(ForeignQueryFailure::ConflictingSourceRoles {
            function,
            runtime,
            platform,
        }
        .into());
    }

    Ok(runtime.is_some() || platform.is_some())
}

pub(super) fn source_role<Role: Copy>(
    compilation: &Compilation,
    function: FunctionSymbolId,
    bindings: &[SourceRoleBinding<Role>],
    foreign_role: impl Fn(Role) -> ForeignSourceRole,
) -> Result<Option<Role>, FactQueryError> {
    if bindings.is_empty() {
        return Ok(None);
    }

    let symbols = compilation.symbol_graph()?;
    let declarations = compilation.product_source_graph()?.declarations();

    let Some(declaration) = symbols
        .function(function)
        .and_then(bray_symbols::FunctionSymbol::declaration)
        .and_then(|declaration| declarations.declaration(declaration))
    else {
        return Ok(None);
    };

    if declaration.kind() != DeclarationKind::Function {
        return Ok(None);
    }

    let Some(name) = declaration
        .name()
        .and_then(bray_declarations::DeclarationName::as_identifier)
    else {
        return Ok(None);
    };

    let Some(module) = declarations
        .container(declaration.owning_container())
        .and_then(bray_declarations::ContainerRecord::module_path)
    else {
        return Ok(None);
    };

    let mut matched = bindings.iter().filter(|binding| {
        binding.declaration() == name
            && binding
                .module()
                .eq(module.segments().iter().map(String::as_str))
    });

    let role = matched.next().map(SourceRoleBinding::role);

    if let (Some(first), Some(duplicate)) = (role, matched.next().map(SourceRoleBinding::role)) {
        return Err(ForeignQueryFailure::DuplicateSourceRole {
            function,
            first: foreign_role(first),
            duplicate: foreign_role(duplicate),
        }
        .into());
    }

    Ok(role)
}
