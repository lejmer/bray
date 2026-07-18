use std::collections::BTreeMap;

use bray_base::Cancellation;
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    BorrowKind, CallableConstness, CallableDependencyContracts, CallableExecution,
    CallableParameterData, CallableParameterMode, CallableParameterName,
    CallableParameterSignature, CallableParameterSymbolId, CallablePosition, CallableSignature,
    CallableSymbolId, CallableTrust, CallableTypeData, DependencyContractTemplateData,
    GenericArgument, GenericOwnerId, GenericSubstitutionData, GenericTypeParameterSymbolId,
    MemberLookupResult, ModuleSymbolId, NamedTypeSymbolId, ReceiverParameterSignature,
    ReceiverParameterSymbolId, SelfTypeContext, SemanticValueStore, StructSymbolId, SymbolGraph,
    SymbolName, TraitApplicationId, TypeData, TypeId,
};
use bray_syntax::{
    GenericArgumentListSyntax, ImplementationSubjectSyntax, ParameterListSyntax, ParameterSyntax,
    PathSyntax, SourceSyntaxNode, SyntaxToken, TraitApplicationSyntax, TypeExpressionSyntax,
};

use super::contract::{CallableTypeQualifiers, TypeParameterBinding};
use crate::{BinderFactError, BinderFactResult};

/// Binds ordinary type-expression and trait-application syntax into canonical semantic values.
pub struct TypeExpressionBinder<'facts> {
    pub(super) symbols: &'facts SymbolGraph,
    pub(super) semantic_values: &'facts SemanticValueStore,
    pub(super) module: Option<ModuleSymbolId>,
    pub(super) type_parameters: BTreeMap<SymbolName, GenericTypeParameterSymbolId>,
    pub(super) self_type: Option<SelfTypeContext>,
    pub(super) cancellation: &'facts dyn Cancellation,
    pub(super) diagnostics: DiagnosticBag,
}

impl<'facts> TypeExpressionBinder<'facts> {
    /// Creates a binder for one declaration surface and its lexical generic scope.
    pub fn new(
        symbols: &'facts SymbolGraph,
        semantic_values: &'facts SemanticValueStore,
        module: Option<ModuleSymbolId>,
        type_parameters: impl IntoIterator<Item = TypeParameterBinding>,
        self_type: Option<SelfTypeContext>,
        cancellation: &'facts dyn Cancellation,
    ) -> Self {
        let type_parameters = type_parameters
            .into_iter()
            .map(|binding| (binding.name, binding.symbol))
            .collect();

        Self {
            symbols,
            semantic_values,
            module,
            type_parameters,
            self_type,
            cancellation,
            diagnostics: DiagnosticBag::new(),
        }
    }

    /// Binds one type expression and publishes its diagnostics atomically with the value.
    pub fn bind_type_expression(
        mut self,
        syntax: &TypeExpressionSyntax,
    ) -> BinderFactResult<DiagnosticResult<TypeId>> {
        self.check_cancellation()?;

        let ty = self.bind_type(syntax)?;

        self.check_cancellation()?;

        Ok(DiagnosticResult::new(ty, self.diagnostics))
    }

    /// Binds one trait application and publishes its diagnostics atomically with the value.
    pub fn bind_trait_application(
        mut self,
        syntax: &TraitApplicationSyntax,
    ) -> BinderFactResult<DiagnosticResult<TraitApplicationId>> {
        self.check_cancellation()?;

        let application = self.bind_trait(syntax)?;

        self.check_cancellation()?;

        Ok(DiagnosticResult::new(application, self.diagnostics))
    }

    /// Binds one implementation subject through ordinary named-type rules.
    pub fn bind_implementation_subject(
        mut self,
        syntax: &ImplementationSubjectSyntax,
    ) -> BinderFactResult<DiagnosticResult<TypeId>> {
        self.check_cancellation()?;

        let arguments = syntax.generic_argument_lists().next();
        let mut ty = self.bind_named_path(&syntax.path(), arguments.as_ref())?;

        if syntax.ampersand_token().is_some() {
            let kind = if syntax.mut_token().is_some() {
                BorrowKind::Mutable
            } else {
                BorrowKind::Shared
            };

            ty = self.intern_type(TypeData::Borrow { kind, target: ty })?;
        }

        self.check_cancellation()?;

        Ok(DiagnosticResult::new(ty, self.diagnostics))
    }

    /// Binds one declaration callable signature through ordinary type-expression rules.
    pub fn bind_callable_signature(
        mut self,
        callable: CallableSymbolId,
        parameter_symbols: &[CallableParameterSymbolId],
        receiver: Option<ReceiverParameterSymbolId>,
        parameter_list: &ParameterListSyntax,
        result_type: Option<&TypeExpressionSyntax>,
        qualifiers: CallableTypeQualifiers,
    ) -> BinderFactResult<DiagnosticResult<CallableSignature>> {
        self.check_cancellation()?;

        let parameters = parameter_list.parameters().collect::<Vec<_>>();
        let parameters_match_owner = parameter_symbols.iter().all(|parameter| {
            self.symbols
                .callable_parameter(*parameter)
                .is_some_and(|record| record.owner() == callable)
        });

        let receiver_matches_owner = receiver.is_none_or(|parameter| {
            self.symbols
                .receiver_parameter(parameter)
                .is_some_and(|record| record.owner() == callable)
        });

        if parameters.len() != parameter_symbols.len()
            || !parameters_match_owner
            || !receiver_matches_owner
            || receiver.is_some() != qualifiers.receiver_mode.is_some()
        {
            return Err(BinderFactError::DependencyUnavailable);
        }

        let mut callable_parameters = Vec::with_capacity(parameters.len());
        let mut signature_parameters = Vec::with_capacity(parameters.len());

        for (syntax, symbol) in parameters.iter().zip(parameter_symbols.iter().copied()) {
            let parameter = self.bind_callable_parameter(syntax)?;

            signature_parameters.push(CallableParameterSignature::new(symbol, parameter.ty()));
            callable_parameters.push(parameter);
        }

        let result = match result_type {
            Some(result) => self.bind_type(result)?,
            None => self.bind_compiler_known_type(RepresentationRole::Unit)?,
        };

        let receiver = match (receiver, qualifiers.receiver_mode, self.self_type) {
            (Some(parameter), Some(mode), Some(context)) => {
                let ty = self.intern_type(TypeData::ContextualSelf(context))?;

                Some(ReceiverParameterSignature::new(parameter, ty, mode))
            }
            (None, None, _) => None,
            _ => return Err(BinderFactError::DependencyUnavailable),
        };

        let dependency_contract = self.empty_dependency_contract()?;

        let callable_type = self.intern_type(TypeData::Callable(CallableTypeData::new(
            callable_parameters,
            result,
            qualifiers.constness,
            qualifiers.trust,
            qualifiers.abi,
            CallableDependencyContracts::for_execution(
                qualifiers.execution,
                dependency_contract,
                dependency_contract,
            ),
        )))?;

        self.check_cancellation()?;

        Ok(DiagnosticResult::new(
            CallableSignature::new(callable_type, receiver, signature_parameters, result),
            self.diagnostics,
        ))
    }

    pub(super) fn bind_type(&mut self, syntax: &TypeExpressionSyntax) -> BinderFactResult<TypeId> {
        self.check_cancellation()?;

        if syntax.is_recovered() {
            return self.error_type();
        }

        if syntax.func_keyword().is_some() {
            return self.bind_callable_type(syntax);
        }

        if syntax.unit_keyword().is_some() {
            return self.bind_compiler_known_type(RepresentationRole::Unit);
        }

        if syntax.box_keyword().is_some() {
            return self.bind_box_type(syntax);
        }

        if syntax.view_keyword().is_some() {
            return self.bind_view_type(syntax);
        }

        if syntax.self_keyword().is_some() {
            return match self.self_type {
                Some(context) => self.intern_type(TypeData::ContextualSelf(context)),
                None => Err(BinderFactError::DependencyUnavailable),
            };
        }

        if syntax.ampersand_token().is_some() {
            return self.bind_borrow_type(syntax);
        }

        if syntax.question_token().is_some() {
            return self.bind_unary_type(syntax, TypeData::Nullable);
        }

        if syntax.dot_token().is_some() {
            // TODO(BRA-202): Select the associated type through the checker request context.
            return self.error_type();
        }

        if syntax.generic_argument_lists().next().is_some() {
            return self.bind_generic_named_type(syntax);
        }

        if let Some(path) = syntax.path() {
            return self.bind_path_type(&path);
        }

        if syntax.open_paren_token().is_some() {
            return self.bind_grouped_or_tuple_type(syntax);
        }

        if syntax.open_bracket_token().is_some() {
            return if syntax.semicolon_token().is_some() {
                self.bind_array_type(syntax)
            } else {
                self.bind_unary_type(syntax, TypeData::Slice)
            };
        }

        Err(BinderFactError::DependencyUnavailable)
    }

    fn bind_borrow_type(&mut self, syntax: &TypeExpressionSyntax) -> BinderFactResult<TypeId> {
        let target = self.bind_only_nested_type(syntax)?;
        let kind = if syntax.mut_token().is_some() {
            BorrowKind::Mutable
        } else {
            BorrowKind::Shared
        };

        self.intern_type(TypeData::Borrow { kind, target })
    }

    fn bind_unary_type(
        &mut self,
        syntax: &TypeExpressionSyntax,
        make: impl FnOnce(TypeId) -> TypeData,
    ) -> BinderFactResult<TypeId> {
        let target = self.bind_only_nested_type(syntax)?;

        self.intern_type(make(target))
    }

    pub(super) fn bind_only_nested_type(
        &mut self,
        syntax: &TypeExpressionSyntax,
    ) -> BinderFactResult<TypeId> {
        let mut nested = syntax.type_expressions();

        let Some(target) = nested.next() else {
            return Err(BinderFactError::DependencyUnavailable);
        };

        if nested.next().is_some() {
            return Err(BinderFactError::DependencyUnavailable);
        }

        self.bind_type(&target)
    }

    fn bind_path_type(&mut self, path: &PathSyntax) -> BinderFactResult<TypeId> {
        let resolved = self.bind_type_path(path);

        match resolved {
            MemberLookupResult::Found(crate::lookup::ResolvedTypeName::Named(definition)) => {
                self.bind_named_type(definition, None)
            }
            MemberLookupResult::Found(crate::lookup::ResolvedTypeName::GenericParameter(
                parameter,
            )) => self.intern_type(TypeData::TypeParameter(parameter)),
            MemberLookupResult::Found(_)
            | MemberLookupResult::NotFound
            | MemberLookupResult::WrongKind(_)
            | MemberLookupResult::Ambiguous(_)
            | MemberLookupResult::Inaccessible(_)
            | MemberLookupResult::Malformed(_) => self.error_type(),
        }
    }

    fn bind_generic_named_type(
        &mut self,
        syntax: &TypeExpressionSyntax,
    ) -> BinderFactResult<TypeId> {
        let mut nested = syntax.type_expressions();
        let Some(base) = nested.next() else {
            return Err(BinderFactError::DependencyUnavailable);
        };

        if nested.next().is_some() {
            return Err(BinderFactError::DependencyUnavailable);
        }

        let Some(path) = base.path() else {
            return Err(BinderFactError::DependencyUnavailable);
        };

        let Some(arguments) = syntax.generic_argument_lists().next() else {
            return Err(BinderFactError::DependencyUnavailable);
        };

        self.bind_named_path(&path, Some(&arguments))
    }

    fn bind_named_path(
        &mut self,
        path: &PathSyntax,
        arguments: Option<&GenericArgumentListSyntax>,
    ) -> BinderFactResult<TypeId> {
        let resolved = self.bind_type_path(path);
        let MemberLookupResult::Found(crate::lookup::ResolvedTypeName::Named(definition)) =
            resolved
        else {
            return self.error_type();
        };

        self.bind_named_type(definition, arguments)
    }

    pub(super) fn bind_named_type(
        &mut self,
        definition: NamedTypeSymbolId,
        arguments: Option<&GenericArgumentListSyntax>,
    ) -> BinderFactResult<TypeId> {
        let parameters = self.named_type_parameters(definition)?;
        let arguments = self.bind_generic_arguments(arguments)?;

        let Some(owner) = GenericOwnerId::try_new(definition.into_any()) else {
            return Err(BinderFactError::DependencyUnavailable);
        };

        let substitution = GenericSubstitutionData::try_new(owner, parameters, arguments)
            .map_err(|_| BinderFactError::DependencyUnavailable)?;

        let substitution = self
            .semantic_values
            .intern_generic_substitution(substitution)
            .map_err(|_| BinderFactError::DependencyUnavailable)?;

        self.intern_type(TypeData::Named {
            definition,
            substitution,
        })
    }

    fn bind_compiler_known_type(&mut self, role: RepresentationRole) -> BinderFactResult<TypeId> {
        let definition = self
            .symbols
            .compiler_known_provider()
            .role_registry()
            .representation_symbol::<StructSymbolId>(role)
            .ok_or(BinderFactError::DependencyUnavailable)?;

        self.bind_named_type(definition.into(), None)
    }

    pub(super) fn bind_generic_arguments(
        &mut self,
        arguments: Option<&GenericArgumentListSyntax>,
    ) -> BinderFactResult<Vec<GenericArgument>> {
        let Some(arguments) = arguments else {
            return Ok(Vec::new());
        };

        arguments
            .generic_arguments()
            .map(|argument| {
                if let Some(ty) = argument.type_expressions().next() {
                    return self.bind_type(&ty).map(GenericArgument::Type);
                }

                if argument.expressions().next().is_some() {
                    // TODO(BRA-202): Request the constant checker and retain its checked open term.
                    return self.bind_recovery_generic_argument();
                }

                Err(BinderFactError::DependencyUnavailable)
            })
            .collect()
    }

    fn bind_callable_type(&mut self, syntax: &TypeExpressionSyntax) -> BinderFactResult<TypeId> {
        let parameters = match syntax.parameter_lists().next() {
            Some(list) => list
                .parameters()
                .map(|parameter| self.bind_callable_parameter(&parameter))
                .collect::<Result<Vec<_>, _>>()?,
            None => Vec::new(),
        };

        let result = match syntax.callable_result_clauses().next() {
            Some(result) => self.bind_type(&result.type_expression())?,
            None => self.bind_compiler_known_type(RepresentationRole::Unit)?,
        };

        let modifiers = syntax.callable_modifiers().next();
        let constness = if modifiers
            .as_ref()
            .is_some_and(|value| value.const_token().is_some())
        {
            CallableConstness::Constant
        } else {
            CallableConstness::Runtime
        };

        let execution = if modifiers
            .as_ref()
            .is_some_and(|value| value.async_token().is_some())
        {
            CallableExecution::Asynchronous
        } else {
            CallableExecution::Synchronous
        };

        let trust = if modifiers
            .as_ref()
            .is_some_and(|value| value.trusted_token().is_some())
        {
            CallableTrust::Trusted
        } else {
            CallableTrust::Safe
        };

        let directives = syntax.callable_directives().next();
        let abi = self.bind_optional_callable_abi(directives.as_ref())?;
        let dependency_contract = self.empty_dependency_contract()?;

        let callable = CallableTypeData::new(
            parameters,
            result,
            constness,
            trust,
            abi,
            CallableDependencyContracts::for_execution(
                execution,
                dependency_contract,
                dependency_contract,
            ),
        );

        self.intern_type(TypeData::Callable(callable))
    }

    fn bind_callable_parameter(
        &mut self,
        syntax: &ParameterSyntax,
    ) -> BinderFactResult<CallableParameterData> {
        let name = token_text(syntax.source(), &syntax.identifier_token())
            .and_then(CallableParameterName::try_new)
            .ok_or(BinderFactError::DependencyUnavailable)?;

        let modifiers = syntax.parameter_modifiers();
        let position = if modifiers.pos_token().is_some() {
            CallablePosition::PositionalOrNamed
        } else {
            CallablePosition::NamedOnly
        };

        let mode = if modifiers.mut_token().is_some() {
            CallableParameterMode::Mutable
        } else {
            CallableParameterMode::Immutable
        };

        let ty = self.bind_type(&syntax.type_expression())?;

        Ok(CallableParameterData::new(name, position, mode, ty))
    }

    pub(super) fn intern_type(&self, data: TypeData) -> BinderFactResult<TypeId> {
        self.semantic_values
            .intern_type(data)
            .map_err(|_| BinderFactError::DependencyUnavailable)
    }

    fn empty_dependency_contract(
        &self,
    ) -> BinderFactResult<bray_symbols::DependencyContractTemplateId> {
        self.semantic_values
            .intern_dependency_contract_template(DependencyContractTemplateData::new([]))
            .map_err(|_| BinderFactError::DependencyUnavailable)
    }

    pub(super) fn error_type(&self) -> BinderFactResult<TypeId> {
        self.intern_type(TypeData::Error)
    }

    fn check_cancellation(&self) -> BinderFactResult<()> {
        if self.cancellation.is_cancelled() {
            return Err(BinderFactError::Cancelled);
        }

        Ok(())
    }
}

pub(super) fn token_text<'source>(
    source: &'source bray_source::SourceSnapshot,
    token: &SyntaxToken,
) -> Option<&'source str> {
    if token.is_missing() {
        return None;
    }

    token.text(source.text())
}
