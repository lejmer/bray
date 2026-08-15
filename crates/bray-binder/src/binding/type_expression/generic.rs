use std::sync::Arc;

use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    GenericArgumentTemplate, GenericOwnerId, GenericParameterSymbolId, GenericSubstitutionData,
    MemberLookupResult, NamedTypeSymbolId, StructSymbolId, TypeData, TypeExpressionTemplate,
    TypeId,
};
use bray_syntax::{
    GenericArgumentListSyntax, GenericArgumentSyntax, PathSyntax, TypeExpressionSyntax,
};

use super::core::TypeExpressionBinder;
use crate::{BindingQueryError, BindingQueryResult};

impl TypeExpressionBinder<'_> {
    /// Binds explicit call arguments against one candidate's generic parameter list.
    pub fn bind_call_generic_arguments(
        mut self,
        arguments: &[GenericArgumentSyntax],
        parameters: &[GenericParameterSymbolId],
    ) -> BindingQueryResult<bray_diagnostics::DiagnosticResult<Vec<GenericArgumentTemplate>>> {
        self.check_cancellation()?;

        let Some(parameters) = parameters.get(..arguments.len()) else {
            return Err(BindingQueryError::DependencyUnavailable);
        };

        let arguments = self.bind_generic_argument_syntaxes(arguments, parameters)?;

        self.check_cancellation()?;

        Ok(bray_diagnostics::DiagnosticResult::new(
            arguments,
            self.diagnostics,
        ))
    }

    pub(super) fn bind_generic_named_type(
        &mut self,
        syntax: &TypeExpressionSyntax,
    ) -> BindingQueryResult<TypeExpressionTemplate> {
        let mut nested = syntax.type_expressions();

        let Some(base) = nested.next() else {
            return Err(BindingQueryError::DependencyUnavailable);
        };

        if nested.next().is_some() {
            return Err(BindingQueryError::DependencyUnavailable);
        }

        let Some(path) = base.path() else {
            return Err(BindingQueryError::DependencyUnavailable);
        };

        let Some(arguments) = syntax.generic_argument_lists().next() else {
            return Err(BindingQueryError::DependencyUnavailable);
        };

        self.bind_named_path(&path, Some(&arguments))
    }

    pub(super) fn bind_named_path(
        &mut self,
        path: &PathSyntax,
        arguments: Option<&GenericArgumentListSyntax>,
    ) -> BindingQueryResult<TypeExpressionTemplate> {
        let resolved = self.bind_type_path(path)?;

        let MemberLookupResult::Found(crate::lookup::ResolvedTypeName::Named(definition)) =
            resolved
        else {
            return self.error_type_template();
        };

        self.bind_named_type(definition, arguments)
    }

    pub(super) fn bind_named_type(
        &mut self,
        definition: NamedTypeSymbolId,
        arguments: Option<&GenericArgumentListSyntax>,
    ) -> BindingQueryResult<TypeExpressionTemplate> {
        let parameters = self.named_type_parameters(definition)?;
        let arguments = self.bind_generic_arguments(arguments, &parameters)?;

        let resolved_arguments = arguments
            .iter()
            .map(GenericArgumentTemplate::resolved_argument)
            .collect::<Option<Vec<_>>>();

        let Some(resolved_arguments) = resolved_arguments else {
            return Ok(TypeExpressionTemplate::Named {
                definition,
                parameters: Arc::from(parameters),
                arguments: Arc::from(arguments),
            });
        };

        let Some(owner) = GenericOwnerId::try_new(definition.into_any()) else {
            return Err(BindingQueryError::DependencyUnavailable);
        };

        let substitution = GenericSubstitutionData::try_new(owner, parameters, resolved_arguments)
            .map_err(|_| BindingQueryError::DependencyUnavailable)?;

        let substitution = self
            .semantic_values
            .intern_generic_substitution(substitution)
            .map_err(|_| BindingQueryError::DependencyUnavailable)?;

        self.intern_type(TypeData::Named {
            definition,
            substitution,
        })
        .map(TypeExpressionTemplate::Resolved)
    }

    pub(super) fn bind_compiler_known_type(
        &mut self,
        role: RepresentationRole,
    ) -> BindingQueryResult<TypeExpressionTemplate> {
        let definition = self
            .symbols
            .compiler_known_provider()
            .role_registry()
            .representation_symbol::<StructSymbolId>(role)
            .ok_or(BindingQueryError::DependencyUnavailable)?;

        self.bind_named_type(definition.into(), None)
    }

    pub(super) fn bind_compiler_known_type_id(
        &mut self,
        role: RepresentationRole,
    ) -> BindingQueryResult<TypeId> {
        let template = self.bind_compiler_known_type(role)?;

        self.require_resolved_type(&template)
    }

    pub(super) fn bind_generic_arguments(
        &mut self,
        arguments: Option<&GenericArgumentListSyntax>,
        parameters: &[GenericParameterSymbolId],
    ) -> BindingQueryResult<Vec<GenericArgumentTemplate>> {
        let Some(arguments) = arguments else {
            return if parameters.is_empty() {
                Ok(Vec::new())
            } else {
                Err(BindingQueryError::DependencyUnavailable)
            };
        };

        let arguments = arguments.generic_arguments().collect::<Vec<_>>();

        self.bind_generic_argument_syntaxes(&arguments, parameters)
    }

    fn bind_generic_argument_syntaxes(
        &mut self,
        arguments: &[GenericArgumentSyntax],
        parameters: &[GenericParameterSymbolId],
    ) -> BindingQueryResult<Vec<GenericArgumentTemplate>> {
        if arguments.len() != parameters.len() {
            return Err(BindingQueryError::DependencyUnavailable);
        }

        arguments
            .iter()
            .zip(parameters.iter().copied())
            .map(|(argument, parameter)| match parameter {
                GenericParameterSymbolId::Type(_) => argument
                    .type_expressions()
                    .next()
                    .ok_or(BindingQueryError::DependencyUnavailable)
                    .and_then(|ty| self.bind_type(&ty))
                    .map(GenericArgumentTemplate::Type),
                GenericParameterSymbolId::Const(parameter) => self
                    .bind_generic_constant_argument(argument, parameter)
                    .map(GenericArgumentTemplate::Constant),
            })
            .collect()
    }
}
