use std::collections::BTreeMap;
use std::sync::Arc;

use bray_base::Cancellation;
use bray_compiler_known::RepresentationRole;
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, BorrowKind, CallableContractSymbolId, ConstantTermData, GenericArgument,
    GenericOwnerId, GenericParameterSymbolId, GenericSubstitutionData,
    GenericTypeParameterSymbolId, ImportedSymbolSkeleton, ModuleSymbolId, SelfTypeContext,
    SemanticValueStore, SymbolGraph, SymbolName, TraitApplicationData, TraitApplicationTemplate,
    TraitTypeMemberSymbolId, TypeData, TypeExpressionTemplate, TypeId,
};
use bray_syntax::{
    ImplementationSubjectSyntax, PathSyntax, SyntaxToken, TraitApplicationSyntax,
    TypeExpressionSyntax,
};

use super::contract::TypeExpressionScope;
use crate::{
    BindingError, BindingQueryContext, BindingQueryError, BindingQueryResult, ImportedPathRoot,
};

/// Supplies imported symbols only when type binding reaches an imported path or declaration.
pub trait TypeExpressionImports<Upstream = std::convert::Infallible> {
    /// Selects an imported package root for a qualified source path.
    fn imported_path_root(
        &self,
        module: ModuleSymbolId,
        components: &[&str],
    ) -> BindingQueryResult<Option<ImportedPathRoot<'_>>, Upstream>;

    /// Returns the imported identity skeleton when imported declaration details are required.
    fn imported_symbols(&self) -> BindingQueryResult<Option<&ImportedSymbolSkeleton>, Upstream>;

    /// Returns the callable type named by one callable-contract declaration.
    fn callable_contract_type(
        &self,
        definition: CallableContractSymbolId,
    ) -> BindingQueryResult<Arc<DiagnosticResult<TypeExpressionTemplate>>, Upstream>;
}

impl<T> TypeExpressionImports<T::UpstreamError> for T
where
    T: BindingQueryContext + ?Sized,
{
    fn imported_path_root(
        &self,
        module: ModuleSymbolId,
        components: &[&str],
    ) -> BindingQueryResult<Option<ImportedPathRoot<'_>>, T::UpstreamError> {
        crate::lookup::visible_imported_path_root(self, module, components)
    }

    fn imported_symbols(
        &self,
    ) -> BindingQueryResult<Option<&ImportedSymbolSkeleton>, T::UpstreamError> {
        BindingQueryContext::imported_symbols(self)
    }

    fn callable_contract_type(
        &self,
        definition: CallableContractSymbolId,
    ) -> BindingQueryResult<Arc<DiagnosticResult<TypeExpressionTemplate>>, T::UpstreamError> {
        BindingQueryContext::callable_contract_type(self, definition)
    }
}

/// Binds declaration type syntax while preserving unchecked constant-expression occurrences.
pub struct TypeExpressionBinder<'binding_context, Upstream = std::convert::Infallible> {
    pub(super) symbols: &'binding_context SymbolGraph,
    pub(super) imports: &'binding_context dyn TypeExpressionImports<Upstream>,
    pub(super) semantic_values: &'binding_context SemanticValueStore,
    pub(super) owner: AnySymbolId,
    pub(super) module: Option<ModuleSymbolId>,
    pub(super) type_parameters: BTreeMap<SymbolName, GenericTypeParameterSymbolId>,
    pub(super) self_type: Option<SelfTypeContext>,
    pub(super) cancellation: &'binding_context dyn Cancellation,
    pub(super) diagnostics: DiagnosticBag,
}

impl<'binding_context, Upstream> TypeExpressionBinder<'binding_context, Upstream> {
    /// Creates a binder for one declaration surface and its lexical generic scope.
    pub fn new(
        symbols: &'binding_context SymbolGraph,
        imports: &'binding_context dyn TypeExpressionImports<Upstream>,
        semantic_values: &'binding_context SemanticValueStore,
        scope: TypeExpressionScope,
        cancellation: &'binding_context dyn Cancellation,
    ) -> Self {
        let type_parameters = scope
            .type_parameters
            .into_iter()
            .map(|binding| (binding.name, binding.symbol))
            .collect();

        Self {
            symbols,
            imports,
            semantic_values,
            owner: scope.owner,
            module: scope.module,
            type_parameters,
            self_type: scope.self_type,
            cancellation,
            diagnostics: DiagnosticBag::new(),
        }
    }

    /// Binds one type expression without checking embedded constant expressions.
    pub fn bind_type_expression(
        mut self,
        syntax: &TypeExpressionSyntax,
    ) -> BindingQueryResult<DiagnosticResult<TypeExpressionTemplate>, Upstream> {
        self.check_cancellation()?;

        let ty = self.bind_type(syntax)?;

        self.check_cancellation()?;

        Ok(DiagnosticResult::new(ty, self.diagnostics))
    }

    /// Binds type expressions through one shared declaration lookup environment.
    pub fn bind_type_expressions<'syntax>(
        mut self,
        syntax: impl IntoIterator<Item = &'syntax TypeExpressionSyntax>,
    ) -> BindingQueryResult<DiagnosticResult<Vec<TypeExpressionTemplate>>, Upstream> {
        self.check_cancellation()?;

        let mut types = Vec::new();

        for expression in syntax {
            types.push(self.bind_type(expression)?);
        }

        self.check_cancellation()?;

        Ok(DiagnosticResult::new(types, self.diagnostics))
    }

    /// Binds the implicit unit result of a callable with no result clause.
    pub fn bind_omitted_callable_result(
        mut self,
    ) -> BindingQueryResult<DiagnosticResult<TypeExpressionTemplate>, Upstream> {
        self.check_cancellation()?;

        let result = self.bind_compiler_known_type(RepresentationRole::Unit)?;

        self.check_cancellation()?;

        Ok(DiagnosticResult::new(result, self.diagnostics))
    }

    /// Binds one trait application, or returns no application with source diagnostics when binding fails.
    pub fn bind_trait_application(
        mut self,
        syntax: &TraitApplicationSyntax,
    ) -> BindingQueryResult<DiagnosticResult<Option<TraitApplicationTemplate>>, Upstream> {
        self.check_cancellation()?;

        let application = self.bind_trait(syntax)?;

        self.check_cancellation()?;

        Ok(DiagnosticResult::new(application, self.diagnostics))
    }

    /// Binds one implementation subject through ordinary named-type rules.
    pub fn bind_implementation_subject(
        mut self,
        syntax: &ImplementationSubjectSyntax,
    ) -> BindingQueryResult<DiagnosticResult<TypeExpressionTemplate>, Upstream> {
        self.check_cancellation()?;

        let arguments = syntax.generic_argument_lists().next();
        let mut ty = self.bind_named_path(&syntax.path(), arguments.as_ref())?;

        if syntax.ampersand_token().is_some() {
            let kind = if syntax.mut_token().is_some() {
                BorrowKind::Mutable
            } else {
                BorrowKind::Shared
            };

            ty = self.bind_borrow_template(kind, ty)?;
        }

        self.check_cancellation()?;

        Ok(DiagnosticResult::new(ty, self.diagnostics))
    }

    pub(super) fn bind_type(
        &mut self,
        syntax: &TypeExpressionSyntax,
    ) -> BindingQueryResult<TypeExpressionTemplate, Upstream> {
        self.check_cancellation()?;

        if syntax.is_recovered() {
            return self.error_type_template();
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
                Some(context) => self
                    .intern_type(TypeData::ContextualSelf(context))
                    .map(TypeExpressionTemplate::Resolved),
                None => Err(BindingQueryError::Binding(
                    BindingError::ContextualSelfUnavailable(SyntaxAnchor::from_node(syntax)),
                )),
            };
        }

        if syntax.ampersand_token().is_some() {
            return self.bind_borrow_type(syntax);
        }

        if syntax.question_token().is_some() {
            return self.bind_unary_type(
                syntax,
                TypeData::Nullable,
                TypeExpressionTemplate::Nullable,
            );
        }

        if syntax.dot_token().is_some() {
            return self.bind_type_valued_member_projection(syntax);
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
                self.bind_unary_type(syntax, TypeData::Slice, TypeExpressionTemplate::Slice)
            };
        }

        Err(BindingQueryError::Binding(BindingError::SyntaxContract(
            SyntaxAnchor::from_node(syntax),
        )))
    }

    fn bind_borrow_type(
        &mut self,
        syntax: &TypeExpressionSyntax,
    ) -> BindingQueryResult<TypeExpressionTemplate, Upstream> {
        let target = self.bind_only_nested_type(syntax)?;

        let kind = if syntax.mut_token().is_some() {
            BorrowKind::Mutable
        } else {
            BorrowKind::Shared
        };

        self.bind_borrow_template(kind, target)
    }

    fn bind_unary_type(
        &mut self,
        syntax: &TypeExpressionSyntax,
        resolved: impl FnOnce(TypeId) -> TypeData,
        deferred: impl FnOnce(Arc<TypeExpressionTemplate>) -> TypeExpressionTemplate,
    ) -> BindingQueryResult<TypeExpressionTemplate, Upstream> {
        let target = self.bind_only_nested_type(syntax)?;

        match target.resolved_type() {
            Some(target) => self
                .intern_type(resolved(target))
                .map(TypeExpressionTemplate::Resolved),
            None => Ok(deferred(Arc::new(target))),
        }
    }

    pub(super) fn bind_only_nested_type(
        &mut self,
        syntax: &TypeExpressionSyntax,
    ) -> BindingQueryResult<TypeExpressionTemplate, Upstream> {
        let mut nested = syntax.type_expressions();

        let Some(target) = nested.next() else {
            return Err(BindingQueryError::Binding(BindingError::SyntaxContract(
                SyntaxAnchor::from_node(syntax),
            )));
        };

        if nested.next().is_some() {
            return Err(BindingQueryError::Binding(BindingError::SyntaxContract(
                SyntaxAnchor::from_node(syntax),
            )));
        }

        self.bind_type(&target)
    }

    fn bind_path_type(
        &mut self,
        path: &PathSyntax,
    ) -> BindingQueryResult<TypeExpressionTemplate, Upstream> {
        self.bind_named_path(path, None)
    }

    pub(super) fn bind_contextual_trait_type_member(
        &self,
        member: TraitTypeMemberSymbolId,
    ) -> BindingQueryResult<TypeExpressionTemplate, Upstream> {
        let Some(context @ SelfTypeContext::Trait(trait_definition)) = self.self_type else {
            return self.error_type_template();
        };

        let parameters = self.trait_parameters(trait_definition)?;

        let arguments = parameters
            .iter()
            .copied()
            .map(|parameter| self.contextual_generic_argument(parameter))
            .collect::<BindingQueryResult<Vec<_>, Upstream>>()?;

        let owner =
            GenericOwnerId::try_new(trait_definition.into()).ok_or(BindingQueryError::Binding(
                BindingError::GenericOwnerUnavailable(trait_definition.into()),
            ))?;

        let substitution =
            GenericSubstitutionData::try_new(owner, parameters, arguments).map_err(|error| {
                BindingQueryError::Binding(BindingError::GenericSubstitution(error))
            })?;

        let substitution = self
            .semantic_values
            .intern_generic_substitution(substitution)
            .map_err(BindingQueryError::SemanticValue)?;

        let application = self
            .semantic_values
            .intern_trait_application(TraitApplicationData::new(trait_definition, substitution))
            .map_err(BindingQueryError::SemanticValue)?;

        self.intern_type(TypeData::TypeValuedMemberProjection {
            subject: self.intern_type(TypeData::ContextualSelf(context))?,
            application,
            member,
        })
        .map(TypeExpressionTemplate::Resolved)
    }

    fn contextual_generic_argument(
        &self,
        parameter: GenericParameterSymbolId,
    ) -> BindingQueryResult<GenericArgument, Upstream> {
        match parameter {
            GenericParameterSymbolId::Type(parameter) => self
                .intern_type(TypeData::TypeParameter(parameter))
                .map(GenericArgument::Type),
            GenericParameterSymbolId::Const(parameter) => self
                .semantic_values
                .intern_constant_term(ConstantTermData::Parameter(parameter))
                .map(GenericArgument::Constant)
                .map_err(BindingQueryError::SemanticValue),
        }
    }

    pub(super) fn intern_type(&self, data: TypeData) -> BindingQueryResult<TypeId, Upstream> {
        self.semantic_values
            .intern_type(data)
            .map_err(BindingQueryError::SemanticValue)
    }

    pub(super) fn error_type(&self) -> BindingQueryResult<TypeId, Upstream> {
        self.intern_type(TypeData::Error)
    }

    pub(super) fn error_type_template(
        &self,
    ) -> BindingQueryResult<TypeExpressionTemplate, Upstream> {
        self.error_type().map(TypeExpressionTemplate::Resolved)
    }

    pub(super) fn require_resolved_type(
        &self,
        template: &TypeExpressionTemplate,
    ) -> BindingQueryResult<TypeId, Upstream> {
        template.resolved_type().ok_or(BindingQueryError::Binding(
            BindingError::UnresolvedTypeTemplate,
        ))
    }

    fn bind_borrow_template(
        &self,
        kind: BorrowKind,
        target: TypeExpressionTemplate,
    ) -> BindingQueryResult<TypeExpressionTemplate, Upstream> {
        match target.resolved_type() {
            Some(target) => self
                .intern_type(TypeData::Borrow { kind, target })
                .map(TypeExpressionTemplate::Resolved),
            None => Ok(TypeExpressionTemplate::Borrow {
                kind,
                target: Arc::new(target),
            }),
        }
    }

    pub(super) fn check_cancellation(&self) -> BindingQueryResult<(), Upstream> {
        if self.cancellation.is_cancelled() {
            return Err(BindingQueryError::Cancelled);
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
