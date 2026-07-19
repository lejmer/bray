use bray_diagnostics::DiagnosticNameKind;
use bray_symbols::{
    AnySymbolId, GenericConstParameterSymbolId, GenericOwnerId, GenericParameterSymbolId,
    GenericSubstitutionData, GenericTypeParameterSymbolId, MemberLookupResult, NamedTypeSymbolId,
    TraitApplicationData, TraitApplicationId, TraitSymbolId,
};
use bray_syntax::{PathSyntax, SourceSyntaxNode, TraitApplicationSyntax};

use super::core::{TypeExpressionBinder, token_text};
use crate::lookup::{
    NameReference, ResolvedName, classify_type, combine_name_lookups, lookup_diagnostic,
    lookup_surface_name,
};
use crate::{BinderFactError, BinderFactResult};

impl TypeExpressionBinder<'_> {
    pub(super) fn bind_trait(
        &mut self,
        syntax: &TraitApplicationSyntax,
    ) -> BinderFactResult<TraitApplicationId> {
        let definition = self.bind_trait_path(&syntax.path());

        let MemberLookupResult::Found(definition) = definition else {
            return Err(BinderFactError::DependencyUnavailable);
        };

        let parameters = self.trait_parameters(definition)?;

        let arguments =
            self.bind_generic_arguments(syntax.generic_argument_lists().next().as_ref())?;

        let Some(owner) = GenericOwnerId::try_new(definition.into()) else {
            return Err(BinderFactError::DependencyUnavailable);
        };

        let substitution = GenericSubstitutionData::try_new(owner, parameters, arguments)
            .map_err(|_| BinderFactError::DependencyUnavailable)?;

        let substitution = self
            .semantic_values
            .intern_generic_substitution(substitution)
            .map_err(|_| BinderFactError::DependencyUnavailable)?;

        self.semantic_values
            .intern_trait_application(TraitApplicationData::new(definition, substitution))
            .map_err(|_| BinderFactError::DependencyUnavailable)
    }

    pub(super) fn bind_type_path(
        &mut self,
        path: &PathSyntax,
    ) -> MemberLookupResult<crate::lookup::ResolvedTypeName, ResolvedName> {
        let lookup = self.bind_path(path).classify(classify_type);

        self.report_lookup(path, DiagnosticNameKind::Type, &lookup);

        lookup
    }

    fn bind_trait_path(
        &mut self,
        path: &PathSyntax,
    ) -> MemberLookupResult<TraitSymbolId, ResolvedName> {
        let lookup = self.bind_path(path).classify(|name| match name {
            ResolvedName::Surface(AnySymbolId::Trait(id)) => Some(id),
            ResolvedName::Local(_) | ResolvedName::Surface(_) => None,
        });

        self.report_lookup(path, DiagnosticNameKind::Trait, &lookup);

        lookup
    }

    fn bind_path(&self, path: &PathSyntax) -> MemberLookupResult<ResolvedName> {
        let references = path
            .identifier_tokens()
            .filter_map(|token| token_text(path.source(), &token))
            .collect::<Vec<_>>();

        let [first] = references.as_slice() else {
            return MemberLookupResult::Malformed(Box::new([]));
        };

        if let Some(parameter) = self.type_parameters.get(*first).copied() {
            return MemberLookupResult::Found(ResolvedName::Surface(parameter.into()));
        }

        if let Some(context) = self.self_type {
            let contextual = lookup_surface_name(
                self.symbols,
                context.symbol(),
                first,
                crate::lookup::NameAccess::Internal,
            );

            if contextual != MemberLookupResult::NotFound {
                return contextual;
            }
        }

        let ambient = lookup_surface_name(
            self.symbols,
            self.symbols.compiler_known_environment().id().into(),
            first,
            crate::lookup::NameAccess::Internal,
        );

        match self.module {
            Some(module) => combine_name_lookups(
                lookup_surface_name(
                    self.symbols,
                    module.into(),
                    first,
                    crate::lookup::NameAccess::Internal,
                ),
                ambient,
            ),
            None => ambient,
        }
    }

    fn report_lookup<T>(
        &mut self,
        path: &PathSyntax,
        expected: DiagnosticNameKind,
        result: &MemberLookupResult<T, ResolvedName>,
    ) {
        let Some(token) = path.identifier_tokens().last() else {
            return;
        };

        let Some(text) = token_text(path.source(), &token) else {
            return;
        };

        let reference = NameReference::new(text, path.source().source_id(), token.range());

        if let Some(diagnostic) = lookup_diagnostic(&reference, expected, result) {
            self.diagnostics.add(diagnostic);
        }
    }

    pub(super) fn named_type_parameters(
        &self,
        definition: NamedTypeSymbolId,
    ) -> BinderFactResult<Vec<GenericParameterSymbolId>> {
        let (type_parameters, const_parameters) = match definition {
            NamedTypeSymbolId::Struct(id) => self.symbols.structure(id).map(|symbol| {
                (
                    symbol.generic_type_parameters(),
                    symbol.generic_const_parameters(),
                )
            }),
            NamedTypeSymbolId::Union(id) => self.symbols.union(id).map(|symbol| {
                (
                    symbol.generic_type_parameters(),
                    symbol.generic_const_parameters(),
                )
            }),
        }
        .ok_or(BinderFactError::DependencyUnavailable)?;

        self.generic_parameters(type_parameters, const_parameters)
    }

    fn trait_parameters(
        &self,
        definition: TraitSymbolId,
    ) -> BinderFactResult<Vec<GenericParameterSymbolId>> {
        let symbol = self
            .symbols
            .trait_symbol(definition)
            .ok_or(BinderFactError::DependencyUnavailable)?;

        self.generic_parameters(
            symbol.generic_type_parameters(),
            symbol.generic_const_parameters(),
        )
    }

    fn generic_parameters(
        &self,
        type_parameters: &[GenericTypeParameterSymbolId],
        const_parameters: &[GenericConstParameterSymbolId],
    ) -> BinderFactResult<Vec<GenericParameterSymbolId>> {
        let mut parameters = Vec::with_capacity(type_parameters.len() + const_parameters.len());

        for parameter in type_parameters {
            let ordinal = self
                .symbols
                .generic_type_parameter(*parameter)
                .map(|record| record.ordinal())
                .ok_or(BinderFactError::DependencyUnavailable)?;

            parameters.push((ordinal, GenericParameterSymbolId::from(*parameter)));
        }

        for parameter in const_parameters {
            let ordinal = self
                .symbols
                .generic_const_parameter(*parameter)
                .map(|record| record.ordinal())
                .ok_or(BinderFactError::DependencyUnavailable)?;

            parameters.push((ordinal, GenericParameterSymbolId::from(*parameter)));
        }

        parameters.sort_by_key(|(ordinal, _)| *ordinal);

        Ok(parameters
            .into_iter()
            .map(|(_, parameter)| parameter)
            .collect())
    }
}
