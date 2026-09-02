use std::sync::Arc;

use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    CallableContractSymbolId, GenericArgumentTemplate, GenericOwnerId, GenericParameterSymbolId,
    GenericSubstitutionData, GenericSubstitutionShapeError, MemberLookupResult, NamedTypeSymbolId,
    StructSymbolId, TypeData, TypeExpressionTemplate, TypeId,
};
use bray_syntax::{
    GenericArgumentListSyntax, GenericArgumentSyntax, PathSyntax, TypeExpressionSyntax,
};

use super::core::TypeExpressionBinder;
use crate::{BindingError, BindingQueryError, BindingQueryResult};

impl<Upstream> TypeExpressionBinder<'_, Upstream> {
    /// Binds explicit call arguments against one candidate's generic parameter list.
    pub fn bind_call_generic_arguments(
        mut self,
        arguments: &[GenericArgumentSyntax],
        parameters: &[GenericParameterSymbolId],
    ) -> BindingQueryResult<
        bray_diagnostics::DiagnosticResult<Vec<GenericArgumentTemplate>>,
        Upstream,
    > {
        self.check_cancellation()?;

        let Some(parameters) = parameters.get(..arguments.len()) else {
            return Err(BindingQueryError::Binding(
                BindingError::GenericSubstitution(
                    GenericSubstitutionShapeError::ArgumentCountMismatch {
                        parameter_count: parameters.len(),
                        argument_count: arguments.len(),
                    },
                ),
            ));
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
    ) -> BindingQueryResult<TypeExpressionTemplate, Upstream> {
        let mut nested = syntax.type_expressions();

        let Some(base) = nested.next() else {
            return Err(syntax_contract(syntax));
        };

        if nested.next().is_some() {
            return Err(syntax_contract(syntax));
        }

        let Some(path) = base.path() else {
            return Err(syntax_contract(syntax));
        };

        let Some(arguments) = syntax.generic_argument_lists().next() else {
            return Err(syntax_contract(syntax));
        };

        self.bind_named_path(&path, Some(&arguments))
    }

    pub(super) fn bind_named_path(
        &mut self,
        path: &PathSyntax,
        arguments: Option<&GenericArgumentListSyntax>,
    ) -> BindingQueryResult<TypeExpressionTemplate, Upstream> {
        let resolved = self.bind_type_path(path)?;

        match resolved {
            MemberLookupResult::Found(crate::lookup::ResolvedTypeName::Named(definition)) => {
                self.bind_named_type(definition, arguments)
            }
            MemberLookupResult::Found(crate::lookup::ResolvedTypeName::CallableContract(
                definition,
            )) => self.bind_callable_contract(definition, arguments),
            MemberLookupResult::Found(crate::lookup::ResolvedTypeName::GenericParameter(
                parameter,
            )) if arguments.is_none() => self
                .intern_type(TypeData::TypeParameter(parameter))
                .map(TypeExpressionTemplate::Resolved),
            MemberLookupResult::Found(_)
            | MemberLookupResult::NotFound
            | MemberLookupResult::WrongKind(_)
            | MemberLookupResult::Ambiguous(_)
            | MemberLookupResult::Inaccessible(_)
            | MemberLookupResult::Malformed(_) => self.error_type_template(),
        }
    }

    pub(super) fn bind_callable_contract(
        &mut self,
        definition: CallableContractSymbolId,
        arguments: Option<&GenericArgumentListSyntax>,
    ) -> BindingQueryResult<TypeExpressionTemplate, Upstream> {
        let parameters = self.callable_contract_parameters(definition)?;
        let arguments = self.bind_generic_arguments(arguments, &parameters)?;
        let target = self.imports.callable_contract_type(definition)?;

        self.diagnostics
            .add_range(target.diagnostics().iter().cloned());

        if parameters.is_empty() {
            return Ok(target.value().clone());
        }

        Ok(TypeExpressionTemplate::CallableContract {
            definition,
            target: Arc::new(target.value().clone()),
            parameters: Arc::from(parameters),
            arguments: Arc::from(arguments),
        })
    }

    pub(super) fn bind_named_type(
        &mut self,
        definition: NamedTypeSymbolId,
        arguments: Option<&GenericArgumentListSyntax>,
    ) -> BindingQueryResult<TypeExpressionTemplate, Upstream> {
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
            return Err(BindingQueryError::Binding(
                BindingError::GenericOwnerUnavailable(definition.into_any()),
            ));
        };

        let substitution = GenericSubstitutionData::try_new(owner, parameters, resolved_arguments)
            .map_err(|error| {
                BindingQueryError::Binding(BindingError::GenericSubstitution(error))
            })?;

        let substitution = self
            .semantic_values
            .intern_generic_substitution(substitution)
            .map_err(BindingQueryError::SemanticValue)?;

        self.intern_type(TypeData::Named {
            definition,
            substitution,
        })
        .map(TypeExpressionTemplate::Resolved)
    }

    pub(super) fn bind_compiler_known_type(
        &mut self,
        role: RepresentationRole,
    ) -> BindingQueryResult<TypeExpressionTemplate, Upstream> {
        let definition = self
            .symbols
            .compiler_known_provider()
            .role_registry()
            .representation_symbol::<StructSymbolId>(role)
            .ok_or(BindingQueryError::Binding(
                BindingError::CompilerKnownRepresentationUnavailable(role),
            ))?;

        self.bind_named_type(definition.into(), None)
    }

    pub(super) fn bind_compiler_known_type_id(
        &mut self,
        role: RepresentationRole,
    ) -> BindingQueryResult<TypeId, Upstream> {
        let template = self.bind_compiler_known_type(role)?;

        self.require_resolved_type(&template)
    }

    pub(super) fn bind_generic_arguments(
        &mut self,
        arguments: Option<&GenericArgumentListSyntax>,
        parameters: &[GenericParameterSymbolId],
    ) -> BindingQueryResult<Vec<GenericArgumentTemplate>, Upstream> {
        let Some(arguments) = arguments else {
            return if parameters.is_empty() {
                Ok(Vec::new())
            } else {
                Err(BindingQueryError::Binding(
                    BindingError::GenericSubstitution(
                        GenericSubstitutionShapeError::ArgumentCountMismatch {
                            parameter_count: parameters.len(),
                            argument_count: 0,
                        },
                    ),
                ))
            };
        };

        let arguments = arguments.generic_arguments().collect::<Vec<_>>();

        self.bind_generic_argument_syntaxes(&arguments, parameters)
    }

    fn bind_generic_argument_syntaxes(
        &mut self,
        arguments: &[GenericArgumentSyntax],
        parameters: &[GenericParameterSymbolId],
    ) -> BindingQueryResult<Vec<GenericArgumentTemplate>, Upstream> {
        if arguments.len() != parameters.len() {
            return Err(BindingQueryError::Binding(
                BindingError::GenericSubstitution(
                    GenericSubstitutionShapeError::ArgumentCountMismatch {
                        parameter_count: parameters.len(),
                        argument_count: arguments.len(),
                    },
                ),
            ));
        }

        arguments
            .iter()
            .zip(parameters.iter().copied())
            .map(|(argument, parameter)| match parameter {
                GenericParameterSymbolId::Type(_) => argument
                    .type_expressions()
                    .next()
                    .ok_or_else(|| {
                        BindingQueryError::Binding(BindingError::SyntaxContract(
                            bray_declarations::SyntaxAnchor::from_node(argument),
                        ))
                    })
                    .and_then(|ty| self.bind_type(&ty))
                    .map(GenericArgumentTemplate::Type),
                GenericParameterSymbolId::Const(parameter) => self
                    .bind_generic_constant_argument(argument, parameter)
                    .map(GenericArgumentTemplate::Constant),
            })
            .collect()
    }
}

fn syntax_contract<Upstream>(syntax: &TypeExpressionSyntax) -> BindingQueryError<Upstream> {
    BindingQueryError::Binding(BindingError::SyntaxContract(
        bray_declarations::SyntaxAnchor::from_node(syntax),
    ))
}
