use std::sync::Arc;

use bray_symbols::{MemberLookupResult, TypeData, TypeExpressionTemplate};
use bray_syntax::TypeExpressionSyntax;

use super::core::TypeExpressionBinder;
use crate::{BindingQueryError, BindingQueryResult};

impl TypeExpressionBinder<'_> {
    pub(super) fn bind_type_valued_member_projection(
        &mut self,
        syntax: &TypeExpressionSyntax,
    ) -> BindingQueryResult<TypeExpressionTemplate> {
        let mut subjects = syntax.type_expressions();

        let Some(subject) = subjects.next() else {
            return self.error_type_template();
        };

        if subjects.next().is_some() {
            return self.error_type_template();
        }

        let mut applications = syntax.trait_applications();

        let Some(application) = applications.next() else {
            return self.error_type_template();
        };

        if applications.next().is_some() {
            return self.error_type_template();
        }

        let subject = self.bind_type(&subject)?;
        let application = self.bind_trait(&application)?;
        let member = self.bind_trait_type_member(application.definition(), syntax)?;

        let MemberLookupResult::Found(member) = member else {
            return self.error_type_template();
        };

        if let (Some(subject), Some(application)) = (
            subject.resolved_type(),
            self.resolve_trait_application_template(&application)?,
        ) {
            return self
                .intern_type(TypeData::TypeValuedMemberProjection {
                    subject,
                    application,
                    member,
                })
                .map(TypeExpressionTemplate::Resolved);
        }

        Ok(TypeExpressionTemplate::TypeValuedMemberProjection {
            subject: Arc::new(subject),
            application,
            member,
        })
    }

    pub(super) fn bind_box_type(
        &mut self,
        syntax: &TypeExpressionSyntax,
    ) -> BindingQueryResult<TypeExpressionTemplate> {
        let target = self.bind_only_nested_type(syntax)?;

        let storage = match syntax.type_form_argument_lists().next() {
            Some(arguments) => {
                let mut arguments = arguments.type_form_arguments();

                let Some(argument) = arguments.next() else {
                    return self.error_type_template();
                };

                if arguments.next().is_some() || argument.expressions().next().is_some() {
                    return self.error_type_template();
                }

                let mut types = argument.type_expressions();

                let Some(storage) = types.next() else {
                    return self.error_type_template();
                };

                if types.next().is_some() {
                    return self.error_type_template();
                }

                self.bind_type(&storage)?
            }
            None => self.bind_heap_storage_type()?,
        };

        if let (Some(storage), Some(target)) = (storage.resolved_type(), target.resolved_type()) {
            return self
                .intern_type(TypeData::OwnedIndirection { storage, target })
                .map(TypeExpressionTemplate::Resolved);
        }

        Ok(TypeExpressionTemplate::OwnedIndirection {
            storage: Arc::new(storage),
            target: Arc::new(target),
        })
    }

    pub(super) fn bind_view_type(
        &mut self,
        syntax: &TypeExpressionSyntax,
    ) -> BindingQueryResult<TypeExpressionTemplate> {
        let mut applications = syntax.trait_applications();

        let Some(application) = applications.next() else {
            return self.error_type_template();
        };

        if applications.next().is_some() {
            return self.error_type_template();
        }

        let application = self.bind_trait(&application)?;

        match self.resolve_trait_application_template(&application)? {
            Some(application) => self
                .intern_type(TypeData::TraitView(application))
                .map(TypeExpressionTemplate::Resolved),
            None => Ok(TypeExpressionTemplate::TraitView(application)),
        }
    }

    pub(super) fn bind_grouped_or_tuple_type(
        &mut self,
        syntax: &TypeExpressionSyntax,
    ) -> BindingQueryResult<TypeExpressionTemplate> {
        let mut elements = syntax
            .type_expressions()
            .map(|element| self.bind_type(&element))
            .collect::<Result<Vec<_>, _>>()?;

        if elements.len() == 1 && syntax.comma_token().is_none() {
            let Some(element) = elements.pop() else {
                return self.error_type_template();
            };

            return Ok(element);
        }

        let resolved = elements
            .iter()
            .map(TypeExpressionTemplate::resolved_type)
            .collect::<Option<Vec<_>>>();

        match resolved {
            Some(elements) => self
                .intern_type(TypeData::tuple(elements))
                .map(TypeExpressionTemplate::Resolved),
            None => Ok(TypeExpressionTemplate::Tuple(Arc::from(elements))),
        }
    }

    pub(super) fn bind_array_type(
        &mut self,
        syntax: &TypeExpressionSyntax,
    ) -> BindingQueryResult<TypeExpressionTemplate> {
        let mut elements = syntax.type_expressions();

        let Some(element) = elements.next() else {
            return self.error_type_template();
        };

        if elements.next().is_some() {
            return self.error_type_template();
        }

        let element = self.bind_type(&element)?;

        if syntax.dot_dot_token().is_some() {
            return Ok(TypeExpressionTemplate::FlexibleArray(Arc::new(element)));
        }

        let mut lengths = syntax.expressions();

        let Some(length) = lengths.next() else {
            return self.error_type_template();
        };

        if lengths.next().is_some() {
            return self.error_type_template();
        }

        let length = self.bind_array_length(&length)?;

        Ok(TypeExpressionTemplate::Array {
            element: Arc::new(element),
            length,
        })
    }

    fn bind_heap_storage_type(&mut self) -> BindingQueryResult<TypeExpressionTemplate> {
        let definition = self
            .symbols
            .compiler_known_provider()
            .heap_storage_policy()
            .ok_or(BindingQueryError::DependencyUnavailable)?;

        self.bind_named_type(definition.into(), None)
    }
}
