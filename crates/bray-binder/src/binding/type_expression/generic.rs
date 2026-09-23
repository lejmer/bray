use std::sync::Arc;

use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    CallableContractSymbolId, GenericArgumentTemplate, GenericOwnerId, GenericParameterSymbolId,
    GenericSubstitutionData, GenericSubstitutionShapeError, MemberLookupResult, NamedTypeSymbolId,
    StructSymbolId, TypeData, TypeExpressionTemplate, TypeId,
};
use bray_syntax::{
    GenericArgumentListSyntax, GenericArgumentSyntax, PathSyntax, SourceSyntaxNode,
    TypeExpressionSyntax,
};

use super::core::TypeExpressionBinder;
use super::super::name::is_bytes_type_path;
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
            self.diagnostics.add(
                super::diagnostic::generic_diagnostic(
                    &base,
                    bray_diagnostics::DiagnosticKind::BindingGenericApplicationRequiresName,
                )
                .with_note(bray_diagnostics::DiagnosticNote::new(
                    bray_diagnostics::DiagnosticNoteKind::GenericApplicationRequiresDeclaredName,
                )),
            );

            return self.error_type_template();
        };

        let Some(arguments) = syntax.generic_argument_lists().next() else {
            return Err(syntax_contract(syntax));
        };

        if is_bytes_type_path(&path) {
            let mut values = arguments.generic_arguments();

            let Some(argument) = values.next() else {
                return self.error_type_template();
            };

            if values.next().is_some() {
                return self.error_type_template();
            }

            let Some(length) = argument.expressions().next() else {
                return self.error_type_template();
            };

            let element = self.bind_compiler_known_type(RepresentationRole::ScalarU8)?;
            let length = self.bind_array_length(&length)?;

            return Ok(TypeExpressionTemplate::Array {
                element: Arc::new(element),
                length,
            });
        }

        self.bind_named_path(&path, Some(&arguments))
    }

    pub(super) fn bind_named_path(
        &mut self,
        path: &PathSyntax,
        arguments: Option<&GenericArgumentListSyntax>,
    ) -> BindingQueryResult<TypeExpressionTemplate, Upstream> {
        let resolved = self.bind_type_path(path)?;

        let expected = match &resolved {
            MemberLookupResult::Found(crate::lookup::ResolvedTypeName::Named(definition)) => {
                self.named_type_parameters(*definition)?.len()
            }
            MemberLookupResult::Found(crate::lookup::ResolvedTypeName::CallableContract(
                definition,
            )) => self.callable_contract_parameters(*definition)?.len(),
            MemberLookupResult::Found(
                crate::lookup::ResolvedTypeName::GenericParameter(_)
                | crate::lookup::ResolvedTypeName::TraitMember(_),
            ) => 0,
            _ => return self.error_type_template(),
        };

        if !self.validate_generic_argument_count(path, arguments, expected)? {
            return self.error_type_template();
        }

        match resolved {
            MemberLookupResult::Found(crate::lookup::ResolvedTypeName::Named(definition)) => {
                self.bind_named_type(definition, arguments)
            }
            MemberLookupResult::Found(crate::lookup::ResolvedTypeName::CallableContract(
                definition,
            )) => self.bind_callable_contract(definition, arguments),
            MemberLookupResult::Found(crate::lookup::ResolvedTypeName::GenericParameter(
                parameter,
            )) => self
                .intern_type(TypeData::TypeParameter(parameter))
                .map(TypeExpressionTemplate::Resolved),
            MemberLookupResult::Found(crate::lookup::ResolvedTypeName::TraitMember(member)) => {
                self.bind_contextual_trait_type_member(member)
            }
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

    pub(super) fn validate_generic_argument_count(
        &mut self,
        syntax: &impl SourceSyntaxNode,
        arguments: Option<&GenericArgumentListSyntax>,
        expected: usize,
    ) -> BindingQueryResult<bool, Upstream> {
        let actual = arguments.map_or(0, |arguments| arguments.generic_arguments().count());

        if expected == actual {
            return Ok(true);
        }

        self.diagnostics.add(
            super::diagnostic::generic_argument_count_diagnostic(syntax, expected, actual)
                .map_err(|cause| {
                    BindingQueryError::Binding(BindingError::GenericSubstitution(cause))
                })?,
        );

        Ok(false)
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
                GenericParameterSymbolId::Type(_) => {
                    let ty = if let Some(ty) = argument.type_expressions().next() {
                        self.bind_type(&ty)?
                    } else {
                        self.diagnostics.add(
                            super::diagnostic::generic_diagnostic(
                                argument,
                                bray_diagnostics::DiagnosticKind::BindingGenericArgumentMustBeType,
                            )
                            .with_note(bray_diagnostics::DiagnosticNote::new(
                                bray_diagnostics::DiagnosticNoteKind::GenericArgumentRequiresType,
                            )),
                        );

                        self.error_type_template()?
                    };

                    Ok(GenericArgumentTemplate::Type(ty))
                }
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
