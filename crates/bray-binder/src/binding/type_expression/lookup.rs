use bray_diagnostics::DiagnosticNameKind;
use bray_symbols::{
    AnySymbolId, GenericArgumentTemplate, GenericConstParameterSymbolId, GenericOwnerId,
    GenericParameterSymbolId, GenericSubstitutionData, GenericTypeParameterSymbolId,
    MemberLookupResult, NamedTypeSymbolId, TraitApplicationData, TraitApplicationId,
    TraitApplicationTemplate, TraitSymbolId, TraitTypeMemberSymbolId,
};
use bray_syntax::{PathSyntax, SourceSyntaxNode, TraitApplicationSyntax, TypeExpressionSyntax};

use super::core::{TypeExpressionBinder, token_text};
use crate::lookup::{
    NameReference, ResolvedName, bind_source_path, classify_type, combine_name_lookups,
    lookup_diagnostic, lookup_surface_name, lookup_surface_name_with_imports,
};
use crate::{BinderFactError, BinderFactResult};

impl TypeExpressionBinder<'_> {
    pub(super) fn bind_trait(
        &mut self,
        syntax: &TraitApplicationSyntax,
    ) -> BinderFactResult<TraitApplicationTemplate> {
        let definition = self.bind_trait_path(&syntax.path())?;

        let MemberLookupResult::Found(definition) = definition else {
            return Err(BinderFactError::DependencyUnavailable);
        };

        let parameters = self.trait_parameters(definition)?;

        let arguments = self
            .bind_generic_arguments(syntax.generic_argument_lists().next().as_ref(), &parameters)?;

        Ok(TraitApplicationTemplate::new(
            definition, parameters, arguments,
        ))
    }

    /// Produces a canonical trait application when every argument is already resolved.
    pub fn resolve_trait_application_template(
        &self,
        template: &TraitApplicationTemplate,
    ) -> BinderFactResult<Option<TraitApplicationId>> {
        let arguments = template
            .arguments()
            .iter()
            .map(GenericArgumentTemplate::resolved_argument)
            .collect::<Option<Vec<_>>>();

        let Some(arguments) = arguments else {
            return Ok(None);
        };

        let Some(owner) = GenericOwnerId::try_new(template.definition().into()) else {
            return Err(BinderFactError::DependencyUnavailable);
        };

        let substitution = GenericSubstitutionData::try_new(
            owner,
            template.parameters().iter().copied(),
            arguments,
        )
        .map_err(|_| BinderFactError::DependencyUnavailable)?;

        let substitution = self
            .semantic_values
            .intern_generic_substitution(substitution)
            .map_err(|_| BinderFactError::DependencyUnavailable)?;

        let application = self
            .semantic_values
            .intern_trait_application(TraitApplicationData::new(
                template.definition(),
                substitution,
            ))
            .map_err(|_| BinderFactError::DependencyUnavailable)?;

        Ok(Some(application))
    }

    pub(super) fn bind_type_path(
        &mut self,
        path: &PathSyntax,
    ) -> BinderFactResult<MemberLookupResult<crate::lookup::ResolvedTypeName, ResolvedName>> {
        let lookup = self.bind_path(path)?.classify(classify_type);

        self.report_lookup(path, DiagnosticNameKind::Type, &lookup);

        Ok(lookup)
    }

    pub(super) fn bind_trait_path(
        &mut self,
        path: &PathSyntax,
    ) -> BinderFactResult<MemberLookupResult<TraitSymbolId, ResolvedName>> {
        let lookup = self.bind_path(path)?.classify(|name| match name {
            ResolvedName::Surface(AnySymbolId::Trait(id)) => Some(id),
            ResolvedName::Local(_) | ResolvedName::Surface(_) => None,
        });

        self.report_lookup(path, DiagnosticNameKind::Trait, &lookup);

        Ok(lookup)
    }

    pub(super) fn bind_trait_type_member(
        &mut self,
        definition: TraitSymbolId,
        syntax: &TypeExpressionSyntax,
    ) -> BinderFactResult<MemberLookupResult<TraitTypeMemberSymbolId, ResolvedName>> {
        let Some(token) = syntax.identifier_token() else {
            return Ok(MemberLookupResult::Malformed(Box::new([])));
        };

        let Some(text) = token_text(syntax.source(), &token) else {
            return Ok(MemberLookupResult::Malformed(Box::new([])));
        };

        let imported_symbols = self.imports.imported_symbols()?;

        let lookup = lookup_surface_name_with_imports(
            self.symbols,
            imported_symbols,
            definition.into(),
            text,
            crate::lookup::NameAccess::Internal,
        )
        .classify(|name| match name {
            ResolvedName::Surface(AnySymbolId::TraitTypeMember(member)) => Some(member),
            ResolvedName::Local(_) | ResolvedName::Surface(_) => None,
        });

        let reference = NameReference::new(text, syntax.source().source_id(), token.range());

        if let Some(diagnostic) = lookup_diagnostic(&reference, DiagnosticNameKind::Type, &lookup) {
            self.diagnostics.add(diagnostic);
        }

        Ok(lookup)
    }

    fn bind_path(&self, path: &PathSyntax) -> BinderFactResult<MemberLookupResult<ResolvedName>> {
        let references = path
            .identifier_tokens()
            .filter_map(|token| token_text(path.source(), &token))
            .collect::<Vec<_>>();

        let Some(first) = references.first() else {
            return Ok(MemberLookupResult::Malformed(Box::new([])));
        };

        if references.len() == 1
            && let Some(parameter) = self.type_parameters.get(*first).copied()
        {
            return Ok(MemberLookupResult::Found(ResolvedName::Surface(
                parameter.into(),
            )));
        }

        if references.len() == 1
            && let Some(context) = self.self_type
        {
            let contextual = lookup_surface_name(
                self.symbols,
                context.symbol(),
                first,
                crate::lookup::NameAccess::Internal,
            );

            if contextual != MemberLookupResult::NotFound {
                return Ok(contextual);
            }
        }

        let ambient = lookup_surface_name(
            self.symbols,
            self.symbols.compiler_known_environment().id().into(),
            first,
            crate::lookup::NameAccess::Internal,
        );

        let Some(module) = self.module else {
            return Ok(ambient);
        };

        let ordinary = combine_name_lookups(
            lookup_surface_name(
                self.symbols,
                module.into(),
                first,
                crate::lookup::NameAccess::Internal,
            ),
            ambient,
        );

        let imported_root = self.imports.imported_path_root(&references)?;

        Ok(bind_source_path(
            self.symbols,
            imported_root,
            module,
            path,
            crate::lookup::NameAccess::Internal,
            ordinary,
        ))
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
            NamedTypeSymbolId::Struct(id) => match self.symbols.structure(id) {
                Some(symbol) => Some((
                    symbol.generic_type_parameters(),
                    symbol.generic_const_parameters(),
                )),
                None => self.imports.imported_symbols()?.and_then(|symbols| {
                    symbols.structure(id).map(|symbol| {
                        (
                            symbol.generic_type_parameters(),
                            symbol.generic_const_parameters(),
                        )
                    })
                }),
            },
            NamedTypeSymbolId::Union(id) => match self.symbols.union(id) {
                Some(symbol) => Some((
                    symbol.generic_type_parameters(),
                    symbol.generic_const_parameters(),
                )),
                None => self.imports.imported_symbols()?.and_then(|symbols| {
                    symbols.union(id).map(|symbol| {
                        (
                            symbol.generic_type_parameters(),
                            symbol.generic_const_parameters(),
                        )
                    })
                }),
            },
        }
        .ok_or(BinderFactError::DependencyUnavailable)?;

        self.generic_parameters(type_parameters, const_parameters)
    }

    pub(super) fn trait_parameters(
        &self,
        definition: TraitSymbolId,
    ) -> BinderFactResult<Vec<GenericParameterSymbolId>> {
        let parameters = match self.symbols.trait_symbol(definition) {
            Some(symbol) => Some((
                symbol.generic_type_parameters(),
                symbol.generic_const_parameters(),
            )),
            None => self.imports.imported_symbols()?.and_then(|symbols| {
                symbols.trait_symbol(definition).map(|symbol| {
                    (
                        symbol.generic_type_parameters(),
                        symbol.generic_const_parameters(),
                    )
                })
            }),
        }
        .ok_or(BinderFactError::DependencyUnavailable)?;

        self.generic_parameters(parameters.0, parameters.1)
    }

    fn generic_parameters(
        &self,
        type_parameters: &[GenericTypeParameterSymbolId],
        const_parameters: &[GenericConstParameterSymbolId],
    ) -> BinderFactResult<Vec<GenericParameterSymbolId>> {
        let mut parameters = Vec::with_capacity(type_parameters.len() + const_parameters.len());

        for parameter in type_parameters {
            let ordinal = match self.symbols.generic_type_parameter(*parameter) {
                Some(record) => Some(record.ordinal()),
                None => self
                    .imports
                    .imported_symbols()?
                    .and_then(|symbols| symbols.generic_type_parameter(*parameter))
                    .map(|record| record.ordinal()),
            }
            .ok_or(BinderFactError::DependencyUnavailable)?;

            parameters.push((ordinal, GenericParameterSymbolId::from(*parameter)));
        }

        for parameter in const_parameters {
            let ordinal = match self.symbols.generic_const_parameter(*parameter) {
                Some(record) => Some(record.ordinal()),
                None => self
                    .imports
                    .imported_symbols()?
                    .and_then(|symbols| symbols.generic_const_parameter(*parameter))
                    .map(|record| record.ordinal()),
            }
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
