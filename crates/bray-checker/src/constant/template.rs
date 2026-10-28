use std::collections::BTreeMap;

use bray_symbols::{
    CallableParameterSignature, CallableSignature, CallableSignatureTemplate,
    ConstantExpressionOccurrenceKey, ConstantTermId, GenericArgument, GenericArgumentTemplate,
    GenericOwnerId, GenericSubstitutionData, GenericSubstitutionId, SemanticValueStore,
    TraitApplicationData, TraitApplicationTemplate, TypeData, TypeExpressionTemplate, TypeId,
};

/// Checked constant terms keyed by their stable source occurrence.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CheckedConstantTerms {
    terms: BTreeMap<ConstantExpressionOccurrenceKey, ConstantTermId>,
}

impl CheckedConstantTerms {
    /// Creates an empty checked-term set.
    pub const fn new() -> Self {
        Self {
            terms: BTreeMap::new(),
        }
    }

    /// Creates a checked-term set with one term per source occurrence.
    pub fn try_from_terms(
        terms: impl IntoIterator<Item = (ConstantExpressionOccurrenceKey, ConstantTermId)>,
    ) -> Result<Self, CheckedConstantTermsBuildError> {
        let mut checked = Self::new();

        for (occurrence, term) in terms {
            if checked.terms.insert(occurrence, term).is_some() {
                return Err(CheckedConstantTermsBuildError::DuplicateOccurrence(
                    occurrence,
                ));
            }
        }

        Ok(checked)
    }

    /// Returns the checked term for one exact source occurrence.
    pub fn term(&self, occurrence: ConstantExpressionOccurrenceKey) -> Option<ConstantTermId> {
        self.terms.get(&occurrence).copied()
    }
}

/// A malformed set of checked constant-expression occurrences.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckedConstantTermsBuildError {
    /// More than one term was supplied for one source occurrence.
    DuplicateOccurrence(ConstantExpressionOccurrenceKey),
}

pub(crate) fn checked_substituted_type<C: crate::CheckerRequestContext + ?Sized>(
    request: crate::CheckerUnitView<'_, C>,
    template: &TypeExpressionTemplate,
    substitution: GenericSubstitutionId,
) -> Result<
    bray_diagnostics::DiagnosticResult<Option<TypeId>>,
    crate::CheckerQueryError<C::UpstreamError>,
> {
    let constants = request.checked_constant_terms(template)?;

    let ty =
        resolve_type_expression_template(request.semantic_values(), template, constants.value());

    let ty = ty
        .map(|ty| request.semantic_values().substitute_type(ty, substitution))
        .transpose().unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in checked_substituted_type: {error:?}"));

    let mut diagnostics = constants.into_parts().1;

    let ty = ty
        .map(|ty| crate::normalize_type_valued_members(request.context(), ty, &mut diagnostics))
        .transpose()?;

    Ok(bray_diagnostics::DiagnosticResult::new(ty, diagnostics))
}

/// Resolves a type template whose embedded constant expressions have been checked.
///
/// Returns `Ok(None)` when a required constant occurrence has not been requested yet.
pub fn resolve_type_expression_template(
    values: &SemanticValueStore,
    template: &TypeExpressionTemplate,
    constants: &CheckedConstantTerms,
) -> Option<TypeId> {
    let data = match template {
        TypeExpressionTemplate::Resolved(ty) => return Some(*ty),
        TypeExpressionTemplate::Named {
            definition,
            parameters,
            arguments,
        } => {
            let Some(owner) = GenericOwnerId::try_new(definition.into_any()) else {
                panic!(
                    "Canonical semantic value construction or lookup failed without an available store cause. in resolve_type_expression_template"
                );
            };

            let arguments = resolve_arguments(values, arguments, constants)?;

            let substitution =
                GenericSubstitutionData::try_new(owner, parameters.iter().copied(), arguments).unwrap_or_else(|error| panic!("Generic substitution construction rejected an exact parameter-to-argument relationship. in resolve_type_expression_template: {error:?}"));

            let substitution = values
                .intern_generic_substitution(substitution).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in resolve_type_expression_template: {error:?}"));

            TypeData::Named {
                definition: *definition,
                substitution,
            }
        }
        TypeExpressionTemplate::CallableContract {
            definition,
            target,
            parameters,
            arguments,
        } => {
            let target = resolve_type_expression_template(values, target, constants)?;

            let arguments = resolve_arguments(values, arguments, constants)?;

            let Some(owner) = GenericOwnerId::try_new((*definition).into()) else {
                panic!(
                    "Canonical semantic value construction or lookup failed without an available store cause. in resolve_type_expression_template"
                );
            };

            let substitution =
                GenericSubstitutionData::try_new(owner, parameters.iter().copied(), arguments).unwrap_or_else(|error| panic!("Generic substitution construction rejected an exact parameter-to-argument relationship. in resolve_type_expression_template: {error:?}"));

            let substitution = values
                .intern_generic_substitution(substitution).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in resolve_type_expression_template: {error:?}"));

            return values
                .substitute_type(target, substitution)
                .map(Some).unwrap_or_else(|error| panic!("resolve_type_expression_template must satisfy its checked construction contract: {error:?}"));
        }
        TypeExpressionTemplate::TypeValuedMemberProjection {
            subject,
            application,
            member,
        } => {
            let subject = resolve_type_expression_template(values, subject, constants)?;

            let application = resolve_trait_application_template(values, application, constants)?;

            TypeData::TypeValuedMemberProjection {
                subject,
                application,
                member: *member,
            }
        }
        TypeExpressionTemplate::Tuple(elements) => {
            let elements = resolve_types(values, elements, constants)?;

            TypeData::tuple(elements)
        }
        TypeExpressionTemplate::Array { element, length } => {
            let element = resolve_type_expression_template(values, element, constants)?;

            let length = constants.term(length.key())?;

            TypeData::Array { element, length }
        }
        TypeExpressionTemplate::FlexibleArray(element) => {
            let element = resolve_type_expression_template(values, element, constants)?;

            TypeData::FlexibleArray(element)
        }
        TypeExpressionTemplate::Slice(target) => {
            let target = resolve_type_expression_template(values, target, constants)?;

            TypeData::Slice(target)
        }
        TypeExpressionTemplate::Nullable(target) => {
            let target = resolve_type_expression_template(values, target, constants)?;

            TypeData::Nullable(target)
        }
        TypeExpressionTemplate::Borrow { kind, target } => {
            let target = resolve_type_expression_template(values, target, constants)?;

            TypeData::Borrow {
                kind: *kind,
                target,
            }
        }
        TypeExpressionTemplate::TraitView(application) => {
            let application = resolve_trait_application_template(values, application, constants)?;

            TypeData::TraitView(application)
        }
        TypeExpressionTemplate::OwnedIndirection { storage, target } => {
            let storage = resolve_type_expression_template(values, storage, constants)?;

            let target = resolve_type_expression_template(values, target, constants)?;

            TypeData::OwnedIndirection { storage, target }
        }
        TypeExpressionTemplate::Callable(callable) => {
            let mut parameters = Vec::with_capacity(callable.parameters().len());

            for parameter in callable.parameters() {
                let ty = resolve_type_expression_template(values, parameter.ty(), constants)?;

                // Callable parameter names are immutable canonical payloads.
                parameters.push(bray_symbols::CallableParameterData::new(
                    parameter.name().clone(),
                    parameter.position(),
                    parameter.mode(),
                    ty,
                ));
            }

            let result = resolve_type_expression_template(values, callable.result(), constants)?;

            // The resolved callable type owns the Arc-backed phase behavior snapshot.
            TypeData::Callable(
                bray_symbols::CallableTypeData::new(
                    parameters,
                    result,
                    callable.constness(),
                    callable.trust(),
                    callable.abi(),
                    callable.dependencies(),
                )
                .with_variadic(callable.is_variadic())
                // The instantiated callable owns the compiler-known template's phase snapshot.
                .with_phase_behaviors(callable.phase_behaviors().clone()),
            )
        }
    };

    values
        .intern_type(data)
        .map(Some).unwrap_or_else(|error| panic!("resolve_type_expression_template must satisfy its checked construction contract: {error:?}"))
}

/// Resolves and substitutes one callable signature template.
///
/// Returns `Ok(None)` when a required constant occurrence has not been requested yet.
pub fn resolve_callable_signature_template(
    values: &SemanticValueStore,
    template: &CallableSignatureTemplate,
    substitution: GenericSubstitutionId,
    constants: &CheckedConstantTerms,
) -> Option<CallableSignature> {
    let callable_type =
        resolve_type_expression_template(values, template.callable_type(), constants)?;

    let result = resolve_type_expression_template(values, template.result(), constants)?;

    let parameter_templates =
        template
            .parameter_type_templates(values).unwrap_or_else(|error| panic!("resolve_callable_signature_template must satisfy its checked construction contract: {error:?}"));

    let mut parameters = Vec::with_capacity(parameter_templates.len());

    for (parameter, parameter_template) in template
        .parameters()
        .iter()
        .copied()
        .zip(parameter_templates)
    {
        let ty = resolve_type_expression_template(values, &parameter_template, constants)?;

        parameters.push(CallableParameterSignature::new(parameter, ty));
    }

    let signature = CallableSignature::new(callable_type, template.receiver(), parameters, result)
        .with_invocation(template.invocation());

    signature
        .try_map_types(|ty| {
            Ok::<_, std::convert::Infallible>(substitute_type(values, ty, substitution))
        })
        .unwrap_or_else(|error| match error {})
        .into()
}

fn substitute_type(
    values: &SemanticValueStore,
    ty: TypeId,
    substitution: GenericSubstitutionId,
) -> TypeId {
    values
        .substitute_type(ty, substitution)
        .unwrap_or_else(|error| {
            panic!("substitute_type must satisfy its checked construction contract: {error:?}")
        })
}

/// Resolves one trait-application template after checking its constant arguments.
pub fn resolve_trait_application_template(
    values: &SemanticValueStore,
    template: &TraitApplicationTemplate,
    constants: &CheckedConstantTerms,
) -> Option<bray_symbols::TraitApplicationId> {
    let Some(owner) = GenericOwnerId::try_new(template.definition().into()) else {
        panic!(
            "Canonical semantic value construction or lookup failed without an available store cause. in resolve_trait_application_template"
        );
    };

    let arguments = resolve_arguments(values, template.arguments(), constants)?;

    let substitution =
        GenericSubstitutionData::try_new(owner, template.parameters().iter().copied(), arguments).unwrap_or_else(|error| panic!("Generic substitution construction rejected an exact parameter-to-argument relationship. in resolve_trait_application_template: {error:?}"));

    let substitution = values
        .intern_generic_substitution(substitution).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in resolve_trait_application_template: {error:?}"));

    values
        .intern_trait_application(TraitApplicationData::new(
            template.definition(),
            substitution,
        ))
        .map(Some).unwrap_or_else(|error| panic!("resolve_trait_application_template must satisfy its checked construction contract: {error:?}"))
}

fn resolve_arguments(
    values: &SemanticValueStore,
    templates: &[GenericArgumentTemplate],
    constants: &CheckedConstantTerms,
) -> Option<Vec<GenericArgument>> {
    let mut arguments = Vec::with_capacity(templates.len());

    for template in templates {
        let argument = match template {
            GenericArgumentTemplate::Resolved(argument) => *argument,
            GenericArgumentTemplate::Type(template) => {
                let ty = resolve_type_expression_template(values, template, constants)?;

                GenericArgument::Type(ty)
            }
            GenericArgumentTemplate::Constant(occurrence) => {
                let term = constants.term(occurrence.key())?;

                GenericArgument::Constant(term)
            }
        };

        arguments.push(argument);
    }

    Some(arguments)
}

fn resolve_types(
    values: &SemanticValueStore,
    templates: &[TypeExpressionTemplate],
    constants: &CheckedConstantTerms,
) -> Option<Vec<TypeId>> {
    let mut types = Vec::with_capacity(templates.len());

    for template in templates {
        let ty = resolve_type_expression_template(values, template, constants)?;

        types.push(ty);
    }

    Some(types)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_declarations::SyntaxAnchor;
    use bray_parser::parse_source_unit;
    use bray_source::{SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion};
    use bray_symbols::{
        AnySymbolId, ConstantExpressionExpectedType, ConstantExpressionOccurrence,
        ConstantExpressionOccurrenceKey, ConstantSymbolId, ConstantTermData,
        GenericConstParameterSymbolId, SymbolId, TypeData, TypeExpressionTemplate,
    };
    use bray_syntax::LiteralExpressionSyntax;

    use super::{
        CheckedConstantTerms, CheckedConstantTermsBuildError, resolve_type_expression_template,
    };

    use crate::test_support::semantic_values;

    #[test]
    fn array_templates_wait_for_and_preserve_checked_open_length_terms() {
        let source = SourceSnapshot::new(
            SourceId::new(0),
            SourceIdentity::new(0),
            SourceOrigin::virtual_source("constant-template-test"),
            SourceVersion::new(1),
            "module example;\nconst value: u8 = 4;\n",
        );

        let Ok(source) = source else {
            panic!("test source must fit");
        };

        let parsed = parse_source_unit(&source);

        let literals =
            bray_testing::syntax_descendants::<LiteralExpressionSyntax>(parsed.source_unit());

        let [literal] = literals.as_slice() else {
            panic!("test source must contain one literal");
        };

        let values = semantic_values();
        let element = values.intern_type(TypeData::Error);

        let Ok(element) = element else {
            panic!("test element type must intern");
        };

        let parameter = GenericConstParameterSymbolId::from_symbol_id(SymbolId::new(30));
        let length = values.intern_constant_term(ConstantTermData::Parameter(parameter));

        let Ok(length) = length else {
            panic!("test length term must intern");
        };

        let occurrence = ConstantExpressionOccurrence::new(
            ConstantExpressionOccurrenceKey::new(
                AnySymbolId::from(ConstantSymbolId::from_symbol_id(SymbolId::new(31))),
                SyntaxAnchor::from_node(literal),
            ),
            ConstantExpressionExpectedType::Resolved(element),
        );

        let duplicate = CheckedConstantTerms::try_from_terms([
            (occurrence.key(), length),
            (occurrence.key(), length),
        ]);

        assert_eq!(
            duplicate,
            Err(CheckedConstantTermsBuildError::DuplicateOccurrence(
                occurrence.key()
            ))
        );

        let template = TypeExpressionTemplate::Array {
            element: Arc::new(TypeExpressionTemplate::Resolved(element)),
            length: occurrence,
        };

        assert_eq!(
            resolve_type_expression_template(values, &template, &CheckedConstantTerms::new()),
            None
        );

        let checked = CheckedConstantTerms::try_from_terms([(occurrence.key(), length)]);

        let Ok(checked) = checked else {
            panic!("one occurrence must build");
        };

        let resolved = resolve_type_expression_template(values, &template, &checked);

        let Some(resolved) = resolved else {
            panic!("checked array template must resolve");
        };

        let data = values.type_data(resolved);

        assert_eq!(data.as_ref(), &TypeData::Array { element, length });
    }
}
