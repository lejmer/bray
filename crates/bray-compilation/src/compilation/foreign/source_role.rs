use bray_declarations::{DeclarationKind, DeclarationTable, ModulePath};
use bray_runtime_interface::SourceRoleBinding;
use bray_symbols::FunctionSymbolId;

use super::super::Compilation;
use super::{ForeignQueryFailure, ForeignSourceRole};
use crate::fact::FactQueryError;

pub(in crate::compilation) fn validate_source_roles(
    compilation: &Compilation,
) -> Result<(), FactQueryError> {
    if compilation.runtime_roles().is_empty() && compilation.platform_services().is_empty() {
        return Ok(());
    }

    let declarations = compilation.product_source_graph()?.declarations();

    require_source_role_bindings(declarations, compilation.runtime_roles());
    require_source_role_bindings(declarations, compilation.platform_services());

    Ok(())
}

fn require_source_role_bindings<Role: Copy>(
    declarations: &DeclarationTable,
    bindings: &[SourceRoleBinding<Role>],
) {
    for binding in bindings {
        let module = declarations.module_container(&ModulePath::new(binding.module()));

        let matched = module.is_some_and(|module| {
            module.declarations().iter().any(|declaration| {
                declarations
                    .declaration(*declaration)
                    .is_some_and(|declaration| {
                        declaration.kind() == DeclarationKind::Function
                            && declaration.name().and_then(|name| name.as_identifier())
                                == Some(binding.declaration())
                    })
            })
        });

        assert!(
            matched,
            "source role binding does not name a function: {}",
            binding.dotted_path()
        );
    }
}

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

#[cfg(test)]
mod tests {
    use bray_runtime_interface::{RuntimeAbiRole, RuntimeRoleSourceBinding};

    use super::validate_source_roles;
    use crate::{Compilation, CompilationRequest};

    #[test]
    #[should_panic(expected = "source role binding does not name a function: app.missing")]
    fn configured_source_roles_must_name_a_function_declaration() {
        let binding =
            RuntimeRoleSourceBinding::try_new(RuntimeAbiRole::RuntimeInitialization, "app.missing")
                .unwrap_or_else(|| panic!("test runtime binding must be valid"));

        let request = CompilationRequest::new(
            crate::test_support::package_identity(),
            vec![crate::test_support::source_input(
                "trusted internal module app;",
                0,
            )],
        )
        .with_runtime_roles([binding]);

        let compilation = Compilation::load(request)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

        validate_source_roles(&compilation)
            .unwrap_or_else(|error| panic!("source roles must validate: {error:?}"));
    }
}
