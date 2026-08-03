use std::collections::BTreeMap;
use std::sync::Arc;

use bray_base::Cancellation;
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, BorrowKind, GenericTypeParameterSymbolId, ImportedSymbolSkeleton,
    MemberLookupResult, ModuleSymbolId, SelfTypeContext, SemanticValueStore, SymbolGraph,
    SymbolName, TraitApplicationTemplate, TraitTypeMemberSymbolId, TypeData,
    TypeExpressionTemplate, TypeId,
};
use bray_syntax::{
    ImplementationSubjectSyntax, PathSyntax, SyntaxToken, TraitApplicationSyntax,
    TypeExpressionSyntax,
};

use super::contract::TypeExpressionScope;
use crate::{BinderFactContext, BinderFactError, BinderFactResult, ImportedPathRoot};

/// Supplies imported symbols only when type binding reaches an imported path or declaration.
pub trait TypeExpressionImports {
    /// Selects an imported package root for a qualified source path.
    fn imported_path_root(
        &self,
        module: ModuleSymbolId,
        components: &[&str],
    ) -> BinderFactResult<Option<ImportedPathRoot<'_>>>;

    /// Returns the imported identity skeleton when imported declaration details are required.
    fn imported_symbols(&self) -> BinderFactResult<Option<&ImportedSymbolSkeleton>>;
}

impl<T> TypeExpressionImports for T
where
    T: BinderFactContext + ?Sized,
{
    fn imported_path_root(
        &self,
        module: ModuleSymbolId,
        components: &[&str],
    ) -> BinderFactResult<Option<ImportedPathRoot<'_>>> {
        crate::lookup::visible_imported_path_root(self, module, components)
    }

    fn imported_symbols(&self) -> BinderFactResult<Option<&ImportedSymbolSkeleton>> {
        BinderFactContext::imported_symbols(self)
    }
}

/// Binds declaration type syntax while preserving unchecked constant-expression occurrences.
pub struct TypeExpressionBinder<'facts> {
    pub(super) symbols: &'facts SymbolGraph,
    pub(super) imports: &'facts dyn TypeExpressionImports,
    pub(super) semantic_values: &'facts SemanticValueStore,
    pub(super) owner: AnySymbolId,
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
        imports: &'facts dyn TypeExpressionImports,
        semantic_values: &'facts SemanticValueStore,
        scope: TypeExpressionScope,
        cancellation: &'facts dyn Cancellation,
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
    ) -> BinderFactResult<DiagnosticResult<TypeExpressionTemplate>> {
        self.check_cancellation()?;

        let ty = self.bind_type(syntax)?;

        self.check_cancellation()?;

        Ok(DiagnosticResult::new(ty, self.diagnostics))
    }

    /// Binds type expressions through one shared declaration lookup environment.
    pub fn bind_type_expressions<'syntax>(
        mut self,
        syntax: impl IntoIterator<Item = &'syntax TypeExpressionSyntax>,
    ) -> BinderFactResult<DiagnosticResult<Vec<TypeExpressionTemplate>>> {
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
    ) -> BinderFactResult<DiagnosticResult<TypeExpressionTemplate>> {
        self.check_cancellation()?;

        let result = self.bind_compiler_known_type(RepresentationRole::Unit)?;

        self.check_cancellation()?;

        Ok(DiagnosticResult::new(result, self.diagnostics))
    }

    /// Binds one trait application and publishes its diagnostics atomically with the value.
    pub fn bind_trait_application(
        mut self,
        syntax: &TraitApplicationSyntax,
    ) -> BinderFactResult<DiagnosticResult<TraitApplicationTemplate>> {
        self.check_cancellation()?;

        let application = self.bind_trait(syntax)?;

        self.check_cancellation()?;

        Ok(DiagnosticResult::new(application, self.diagnostics))
    }

    /// Binds one implementation subject through ordinary named-type rules.
    pub fn bind_implementation_subject(
        mut self,
        syntax: &ImplementationSubjectSyntax,
    ) -> BinderFactResult<DiagnosticResult<TypeExpressionTemplate>> {
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
    ) -> BinderFactResult<TypeExpressionTemplate> {
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
                None => Err(BinderFactError::DependencyUnavailable),
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

        Err(BinderFactError::DependencyUnavailable)
    }

    fn bind_borrow_type(
        &mut self,
        syntax: &TypeExpressionSyntax,
    ) -> BinderFactResult<TypeExpressionTemplate> {
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
    ) -> BinderFactResult<TypeExpressionTemplate> {
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
    ) -> BinderFactResult<TypeExpressionTemplate> {
        let mut nested = syntax.type_expressions();

        let Some(target) = nested.next() else {
            return Err(BinderFactError::DependencyUnavailable);
        };

        if nested.next().is_some() {
            return Err(BinderFactError::DependencyUnavailable);
        }

        self.bind_type(&target)
    }

    fn bind_path_type(&mut self, path: &PathSyntax) -> BinderFactResult<TypeExpressionTemplate> {
        let resolved = self.bind_type_path(path)?;

        match resolved {
            MemberLookupResult::Found(crate::lookup::ResolvedTypeName::Named(definition)) => {
                self.bind_named_type(definition, None)
            }
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

    fn bind_contextual_trait_type_member(
        &self,
        member: TraitTypeMemberSymbolId,
    ) -> BinderFactResult<TypeExpressionTemplate> {
        let Some(context @ SelfTypeContext::Trait(trait_definition)) = self.self_type else {
            return self.error_type_template();
        };

        let parameters = self.trait_parameters(trait_definition)?;

        if !parameters.is_empty() {
            return self.error_type_template();
        }

        let subject = self
            .intern_type(TypeData::ContextualSelf(context))
            .map(TypeExpressionTemplate::Resolved)?;

        let application = TraitApplicationTemplate::new(trait_definition, [], []);

        Ok(TypeExpressionTemplate::TypeValuedMemberProjection {
            subject: Arc::new(subject),
            application,
            member,
        })
    }

    pub(super) fn intern_type(&self, data: TypeData) -> BinderFactResult<TypeId> {
        self.semantic_values
            .intern_type(data)
            .map_err(|_| BinderFactError::DependencyUnavailable)
    }

    pub(super) fn error_type(&self) -> BinderFactResult<TypeId> {
        self.intern_type(TypeData::Error)
    }

    pub(super) fn error_type_template(&self) -> BinderFactResult<TypeExpressionTemplate> {
        self.error_type().map(TypeExpressionTemplate::Resolved)
    }

    pub(super) fn require_resolved_type(
        &self,
        template: &TypeExpressionTemplate,
    ) -> BinderFactResult<TypeId> {
        template
            .resolved_type()
            .ok_or(BinderFactError::DependencyUnavailable)
    }

    fn bind_borrow_template(
        &self,
        kind: BorrowKind,
        target: TypeExpressionTemplate,
    ) -> BinderFactResult<TypeExpressionTemplate> {
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

    pub(super) fn check_cancellation(&self) -> BinderFactResult<()> {
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
