use std::collections::BTreeMap;

use bray_symbols::{
    CallableParameterSignature, CallableSignature, CallableSignatureTemplate,
    ConstantExpressionOccurrenceKey, ConstantTermId, GenericArgument, GenericArgumentTemplate,
    GenericOwnerId, GenericSubstitutionData, GenericSubstitutionId, SemanticValueStore,
    TraitApplicationData, TraitApplicationTemplate, TypeData, TypeExpressionTemplate, TypeId,
};

use crate::CheckerInfrastructureError;

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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckedConstantTermsBuildError {
    /// More than one term was supplied for one source occurrence.
    DuplicateOccurrence(ConstantExpressionOccurrenceKey),
}

/// Resolves a type template whose embedded constant expressions have been checked.
///
/// Returns `Ok(None)` when a required constant occurrence has not been requested yet.
pub fn resolve_type_expression_template(
    values: &SemanticValueStore,
    template: &TypeExpressionTemplate,
    constants: &CheckedConstantTerms,
) -> Result<Option<TypeId>, CheckerInfrastructureError> {
    let data = match template {
        TypeExpressionTemplate::Resolved(ty) => return Ok(Some(*ty)),
        TypeExpressionTemplate::Named {
            definition,
            parameters,
            arguments,
        } => {
            let Some(owner) = GenericOwnerId::try_new(definition.into_any()) else {
                return Err(CheckerInfrastructureError::SemanticValueUnavailable);
            };

            let Some(arguments) = resolve_arguments(values, arguments, constants)? else {
                return Ok(None);
            };

            let substitution =
                GenericSubstitutionData::try_new(owner, parameters.iter().copied(), arguments)
                    .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

            let substitution = values
                .intern_generic_substitution(substitution)
                .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

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
            let Some(target) = resolve_type_expression_template(values, target, constants)? else {
                return Ok(None);
            };

            let Some(arguments) = resolve_arguments(values, arguments, constants)? else {
                return Ok(None);
            };

            let Some(owner) = GenericOwnerId::try_new((*definition).into()) else {
                return Err(CheckerInfrastructureError::SemanticValueUnavailable);
            };

            let substitution =
                GenericSubstitutionData::try_new(owner, parameters.iter().copied(), arguments)
                    .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

            let substitution = values
                .intern_generic_substitution(substitution)
                .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

            return values
                .substitute_type(target, substitution)
                .map(Some)
                .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable);
        }
        TypeExpressionTemplate::TypeValuedMemberProjection {
            subject,
            application,
            member,
        } => {
            let Some(subject) = resolve_type_expression_template(values, subject, constants)?
            else {
                return Ok(None);
            };

            let Some(application) =
                resolve_trait_application_template(values, application, constants)?
            else {
                return Ok(None);
            };

            TypeData::TypeValuedMemberProjection {
                subject,
                application,
                member: *member,
            }
        }
        TypeExpressionTemplate::Tuple(elements) => {
            let Some(elements) = resolve_types(values, elements, constants)? else {
                return Ok(None);
            };

            TypeData::tuple(elements)
        }
        TypeExpressionTemplate::Array { element, length } => {
            let Some(element) = resolve_type_expression_template(values, element, constants)?
            else {
                return Ok(None);
            };

            let Some(length) = constants.term(length.key()) else {
                return Ok(None);
            };

            values
                .constant_term_data(length)
                .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

            TypeData::Array { element, length }
        }
        TypeExpressionTemplate::FlexibleArray(element) => {
            let Some(element) = resolve_type_expression_template(values, element, constants)?
            else {
                return Ok(None);
            };

            TypeData::FlexibleArray(element)
        }
        TypeExpressionTemplate::Slice(target) => {
            let Some(target) = resolve_type_expression_template(values, target, constants)? else {
                return Ok(None);
            };

            TypeData::Slice(target)
        }
        TypeExpressionTemplate::Nullable(target) => {
            let Some(target) = resolve_type_expression_template(values, target, constants)? else {
                return Ok(None);
            };

            TypeData::Nullable(target)
        }
        TypeExpressionTemplate::Borrow { kind, target } => {
            let Some(target) = resolve_type_expression_template(values, target, constants)? else {
                return Ok(None);
            };

            TypeData::Borrow {
                kind: *kind,
                target,
            }
        }
        TypeExpressionTemplate::TraitView(application) => {
            let Some(application) =
                resolve_trait_application_template(values, application, constants)?
            else {
                return Ok(None);
            };

            TypeData::TraitView(application)
        }
        TypeExpressionTemplate::OwnedIndirection { storage, target } => {
            let Some(storage) = resolve_type_expression_template(values, storage, constants)?
            else {
                return Ok(None);
            };

            let Some(target) = resolve_type_expression_template(values, target, constants)? else {
                return Ok(None);
            };

            TypeData::OwnedIndirection { storage, target }
        }
        TypeExpressionTemplate::Callable(callable) => {
            let mut parameters = Vec::with_capacity(callable.parameters().len());

            for parameter in callable.parameters() {
                let Some(ty) = resolve_type_expression_template(values, parameter.ty(), constants)?
                else {
                    return Ok(None);
                };

                // Callable parameter names are immutable canonical payloads.
                parameters.push(bray_symbols::CallableParameterData::new(
                    parameter.name().clone(),
                    parameter.position(),
                    parameter.mode(),
                    ty,
                ));
            }

            let Some(result) =
                resolve_type_expression_template(values, callable.result(), constants)?
            else {
                return Ok(None);
            };

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
        .map(Some)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
}

/// Resolves and substitutes one callable signature template.
///
/// Returns `Ok(None)` when a required constant occurrence has not been requested yet.
pub fn resolve_callable_signature_template(
    values: &SemanticValueStore,
    template: &CallableSignatureTemplate,
    substitution: GenericSubstitutionId,
    constants: &CheckedConstantTerms,
) -> Result<Option<CallableSignature>, CheckerInfrastructureError> {
    let Some(callable_type) =
        resolve_type_expression_template(values, template.callable_type(), constants)?
    else {
        return Ok(None);
    };

    let Some(result) = resolve_type_expression_template(values, template.result(), constants)?
    else {
        return Ok(None);
    };

    let parameter_templates = template
        .parameter_type_templates(values)
        .map_err(|_| CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

    let mut parameters = Vec::with_capacity(parameter_templates.len());

    for (parameter, parameter_template) in template
        .parameters()
        .iter()
        .copied()
        .zip(parameter_templates)
    {
        let Some(ty) = resolve_type_expression_template(values, &parameter_template, constants)?
        else {
            return Ok(None);
        };

        parameters.push(CallableParameterSignature::new(parameter, ty));
    }

    let signature = CallableSignature::new(callable_type, template.receiver(), parameters, result);

    signature
        .try_map_types(|ty| substitute_type(values, ty, substitution))
        .map(Some)
}

fn substitute_type(
    values: &SemanticValueStore,
    ty: TypeId,
    substitution: GenericSubstitutionId,
) -> Result<TypeId, CheckerInfrastructureError> {
    values
        .substitute_type(ty, substitution)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
}

/// Resolves one trait-application template after checking its constant arguments.
pub fn resolve_trait_application_template(
    values: &SemanticValueStore,
    template: &TraitApplicationTemplate,
    constants: &CheckedConstantTerms,
) -> Result<Option<bray_symbols::TraitApplicationId>, CheckerInfrastructureError> {
    let Some(owner) = GenericOwnerId::try_new(template.definition().into()) else {
        return Err(CheckerInfrastructureError::SemanticValueUnavailable);
    };

    let Some(arguments) = resolve_arguments(values, template.arguments(), constants)? else {
        return Ok(None);
    };

    let substitution =
        GenericSubstitutionData::try_new(owner, template.parameters().iter().copied(), arguments)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let substitution = values
        .intern_generic_substitution(substitution)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    values
        .intern_trait_application(TraitApplicationData::new(
            template.definition(),
            substitution,
        ))
        .map(Some)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
}

fn resolve_arguments(
    values: &SemanticValueStore,
    templates: &[GenericArgumentTemplate],
    constants: &CheckedConstantTerms,
) -> Result<Option<Vec<GenericArgument>>, CheckerInfrastructureError> {
    let mut arguments = Vec::with_capacity(templates.len());

    for template in templates {
        let argument = match template {
            GenericArgumentTemplate::Resolved(argument) => *argument,
            GenericArgumentTemplate::Type(template) => {
                let Some(ty) = resolve_type_expression_template(values, template, constants)?
                else {
                    return Ok(None);
                };

                GenericArgument::Type(ty)
            }
            GenericArgumentTemplate::Constant(occurrence) => {
                let Some(term) = constants.term(occurrence.key()) else {
                    return Ok(None);
                };

                values
                    .constant_term_data(term)
                    .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

                GenericArgument::Constant(term)
            }
        };

        arguments.push(argument);
    }

    Ok(Some(arguments))
}

fn resolve_types(
    values: &SemanticValueStore,
    templates: &[TypeExpressionTemplate],
    constants: &CheckedConstantTerms,
) -> Result<Option<Vec<TypeId>>, CheckerInfrastructureError> {
    let mut types = Vec::with_capacity(templates.len());

    for template in templates {
        let Some(ty) = resolve_type_expression_template(values, template, constants)? else {
            return Ok(None);
        };

        types.push(ty);
    }

    Ok(Some(types))
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

    use super::{CheckedConstantTerms, resolve_type_expression_template};
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

        let template = TypeExpressionTemplate::Array {
            element: Arc::new(TypeExpressionTemplate::Resolved(element)),
            length: occurrence,
        };

        assert_eq!(
            resolve_type_expression_template(values, &template, &CheckedConstantTerms::new()),
            Ok(None)
        );

        let checked = CheckedConstantTerms::try_from_terms([(occurrence.key(), length)]);

        let Ok(checked) = checked else {
            panic!("one occurrence must build");
        };

        let resolved = resolve_type_expression_template(values, &template, &checked);

        let Ok(Some(resolved)) = resolved else {
            panic!("checked array template must resolve");
        };

        let data = values.type_data(resolved);

        let Ok(data) = data else {
            panic!("resolved array type must exist");
        };

        assert_eq!(data.as_ref(), &TypeData::Array { element, length });
    }
}
