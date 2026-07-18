use bray_symbols::{
    ConstantTermData, ConstantTermId, ConstantValueData, ConstantValueKind, GenericArgument,
    TypeData, TypeId,
};
use bray_syntax::TypeExpressionSyntax;

use super::core::TypeExpressionBinder;
use crate::{BinderFactError, BinderFactResult};

impl TypeExpressionBinder<'_> {
    pub(super) fn bind_box_type(
        &mut self,
        syntax: &TypeExpressionSyntax,
    ) -> BinderFactResult<TypeId> {
        let target = self.bind_only_nested_type(syntax)?;

        let storage = match syntax.type_form_argument_lists().next() {
            Some(arguments) => {
                let mut arguments = arguments.type_form_arguments();

                let Some(argument) = arguments.next() else {
                    return self.error_type();
                };

                if arguments.next().is_some() || argument.expressions().next().is_some() {
                    return self.error_type();
                }

                let mut types = argument.type_expressions();

                let Some(storage) = types.next() else {
                    return self.error_type();
                };

                if types.next().is_some() {
                    return self.error_type();
                }

                self.bind_type(&storage)?
            }
            None => self.bind_heap_storage_type()?,
        };

        self.intern_type(TypeData::OwnedIndirection { storage, target })
    }

    pub(super) fn bind_view_type(
        &mut self,
        syntax: &TypeExpressionSyntax,
    ) -> BinderFactResult<TypeId> {
        let mut applications = syntax.trait_applications();

        let Some(application) = applications.next() else {
            return self.error_type();
        };

        if applications.next().is_some() {
            return self.error_type();
        }

        let application = self.bind_trait(&application)?;

        self.intern_type(TypeData::TraitView(application))
    }

    pub(super) fn bind_grouped_or_tuple_type(
        &mut self,
        syntax: &TypeExpressionSyntax,
    ) -> BinderFactResult<TypeId> {
        let mut elements = syntax
            .type_expressions()
            .map(|element| self.bind_type(&element))
            .collect::<Result<Vec<_>, _>>()?;

        if elements.len() == 1 && syntax.comma_token().is_none() {
            let Some(element) = elements.pop() else {
                return self.error_type();
            };

            return Ok(element);
        }

        self.intern_type(TypeData::tuple(elements))
    }

    pub(super) fn bind_array_type(
        &mut self,
        syntax: &TypeExpressionSyntax,
    ) -> BinderFactResult<TypeId> {
        let mut elements = syntax.type_expressions();

        let Some(element) = elements.next() else {
            return self.error_type();
        };

        if elements.next().is_some() {
            return self.error_type();
        }

        let element = self.bind_type(&element)?;

        let mut lengths = syntax.expressions();

        let Some(_length) = lengths.next() else {
            return self.error_type();
        };

        if lengths.next().is_some() {
            return self.error_type();
        }

        // TODO(BRA-202): Request the constant checker and retain its checked open term.
        let length = self.recovery_constant_term()?;

        self.intern_type(TypeData::Array { element, length })
    }

    pub(super) fn bind_recovery_generic_argument(&self) -> BinderFactResult<GenericArgument> {
        self.recovery_constant_term().map(GenericArgument::Constant)
    }

    fn bind_heap_storage_type(&mut self) -> BinderFactResult<TypeId> {
        let definition = self
            .symbols
            .compiler_known_provider()
            .heap_storage_policy()
            .ok_or(BinderFactError::DependencyUnavailable)?;

        self.bind_named_type(definition.into(), None)
    }

    fn recovery_constant_term(&self) -> BinderFactResult<ConstantTermId> {
        let error_type = self.error_type()?;

        let value = self
            .semantic_values
            .intern_constant_value(ConstantValueData::new(error_type, ConstantValueKind::Error))
            .map_err(|_| BinderFactError::DependencyUnavailable)?;

        self.semantic_values
            .intern_constant_term(ConstantTermData::Value(value))
            .map_err(|_| BinderFactError::DependencyUnavailable)
    }
}
