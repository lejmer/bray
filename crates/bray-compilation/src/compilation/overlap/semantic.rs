use std::collections::{BTreeMap, BTreeSet};

use bray_symbols::{
    CallableParameterData, ConstantTermData, ConstantTermId, GenericArgument,
    GenericArgumentTemplate, GenericConstParameterSymbolId, GenericParameterSymbolId,
    GenericTypeParameterSymbolId, SemanticValueStore, SemanticValueStoreError, TraitApplicationId,
    TypeData, TypeExpressionTemplate, TypeId,
};

pub(super) struct SemanticUnifier<'values> {
    parameters: BTreeSet<GenericParameterSymbolId>,
    type_bindings: BTreeMap<GenericTypeParameterSymbolId, TypeId>,
    constant_bindings: BTreeMap<GenericConstParameterSymbolId, ConstantTermId>,
    values: &'values SemanticValueStore,
}

impl<'values> SemanticUnifier<'values> {
    pub(super) fn new(
        left: &[GenericParameterSymbolId],
        right: &[GenericParameterSymbolId],
        values: &'values SemanticValueStore,
    ) -> Self {
        Self {
            parameters: left.iter().chain(right).copied().collect(),
            type_bindings: BTreeMap::new(),
            constant_bindings: BTreeMap::new(),
            values,
        }
    }

    pub(super) fn types_may_overlap(
        &mut self,
        left: TypeId,
        right: TypeId,
    ) -> Result<bool, SemanticValueStoreError> {
        if left == right {
            return Ok(true);
        }

        if let Some(parameter) = self.header_type_parameter(left)? {
            if let Some(bound) = self.type_bindings.get(&parameter).copied() {
                return self.types_may_overlap(bound, right);
            }

            return self.bind_type_parameter(parameter, right);
        }

        if let Some(parameter) = self.header_type_parameter(right)? {
            if let Some(bound) = self.type_bindings.get(&parameter).copied() {
                return self.types_may_overlap(left, bound);
            }

            return self.bind_type_parameter(parameter, left);
        }

        let left_data = self.values.type_data(left)?;
        let right_data = self.values.type_data(right)?;

        match (left_data.as_ref(), right_data.as_ref()) {
            (TypeData::Error, _) | (_, TypeData::Error) => Ok(false),
            (
                TypeData::Named {
                    definition: left_definition,
                    substitution: left_substitution,
                },
                TypeData::Named {
                    definition: right_definition,
                    substitution: right_substitution,
                },
            ) => {
                if left_definition != right_definition {
                    return Ok(false);
                }

                self.substitutions_may_overlap(*left_substitution, *right_substitution)
            }
            (
                TypeData::TypeValuedMemberProjection {
                    subject: left_subject,
                    application: left_application,
                    member: left_member,
                },
                TypeData::TypeValuedMemberProjection {
                    subject: right_subject,
                    application: right_application,
                    member: right_member,
                },
            ) => {
                if left_member != right_member
                    || !self.types_may_overlap(*left_subject, *right_subject)?
                {
                    return Ok(false);
                }

                self.trait_applications_may_overlap(*left_application, *right_application)
            }
            (TypeData::Tuple(left), TypeData::Tuple(right)) => {
                self.type_lists_may_overlap(left, right)
            }
            (
                TypeData::Array {
                    element: left_element,
                    length: left_length,
                },
                TypeData::Array {
                    element: right_element,
                    length: right_length,
                },
            ) => {
                if !self.types_may_overlap(*left_element, *right_element)? {
                    return Ok(false);
                }

                self.constants_may_overlap(*left_length, *right_length)
            }
            (TypeData::Slice(left), TypeData::Slice(right))
            | (TypeData::Generator(left), TypeData::Generator(right))
            | (TypeData::Nullable(left), TypeData::Nullable(right)) => {
                self.types_may_overlap(*left, *right)
            }
            (
                TypeData::Borrow {
                    kind: left_kind,
                    target: left_target,
                },
                TypeData::Borrow {
                    kind: right_kind,
                    target: right_target,
                },
            ) => {
                if left_kind != right_kind {
                    return Ok(false);
                }

                self.types_may_overlap(*left_target, *right_target)
            }
            (TypeData::TraitView(left), TypeData::TraitView(right)) => {
                self.trait_applications_may_overlap(*left, *right)
            }
            (
                TypeData::OwnedIndirection {
                    storage: left_storage,
                    target: left_target,
                },
                TypeData::OwnedIndirection {
                    storage: right_storage,
                    target: right_target,
                },
            ) => {
                if !self.types_may_overlap(*left_storage, *right_storage)? {
                    return Ok(false);
                }

                self.types_may_overlap(*left_target, *right_target)
            }
            (TypeData::Callable(left), TypeData::Callable(right)) => {
                self.callables_may_overlap(left, right)
            }
            _ => Ok(false),
        }
    }

    pub(super) fn type_templates_may_overlap(
        &mut self,
        left: &TypeExpressionTemplate,
        right: &TypeExpressionTemplate,
    ) -> Result<bool, SemanticValueStoreError> {
        match (left, right) {
            (TypeExpressionTemplate::Resolved(left), TypeExpressionTemplate::Resolved(right)) => {
                self.types_may_overlap(*left, *right)
            }
            (
                TypeExpressionTemplate::Named {
                    definition: left_definition,
                    arguments: left_arguments,
                    ..
                },
                TypeExpressionTemplate::Named {
                    definition: right_definition,
                    arguments: right_arguments,
                    ..
                },
            ) => {
                if left_definition != right_definition
                    || left_arguments.len() != right_arguments.len()
                {
                    return Ok(false);
                }

                for (left, right) in left_arguments.iter().zip(right_arguments.iter()) {
                    match (left, right) {
                        (
                            GenericArgumentTemplate::Resolved(left),
                            GenericArgumentTemplate::Resolved(right),
                        ) if left != right => return Ok(false),
                        (
                            GenericArgumentTemplate::Resolved(bray_symbols::GenericArgument::Type(left)),
                            GenericArgumentTemplate::Type(right),
                        ) if !self.type_templates_may_overlap(
                            &TypeExpressionTemplate::Resolved(*left),
                            right,
                        )? => return Ok(false),
                        (
                            GenericArgumentTemplate::Type(left),
                            GenericArgumentTemplate::Resolved(bray_symbols::GenericArgument::Type(right)),
                        ) if !self.type_templates_may_overlap(
                            left,
                            &TypeExpressionTemplate::Resolved(*right),
                        )? => return Ok(false),
                        (
                            GenericArgumentTemplate::Type(left),
                            GenericArgumentTemplate::Type(right),
                        ) if !self.type_templates_may_overlap(left, right)? => return Ok(false),
                        (
                            GenericArgumentTemplate::Type(_),
                            GenericArgumentTemplate::Constant(_),
                        )
                        | (
                            GenericArgumentTemplate::Constant(_),
                            GenericArgumentTemplate::Type(_),
                        )
                        | (
                            GenericArgumentTemplate::Resolved(bray_symbols::GenericArgument::Type(_)),
                            GenericArgumentTemplate::Constant(_),
                        )
                        | (
                            GenericArgumentTemplate::Constant(_),
                            GenericArgumentTemplate::Resolved(bray_symbols::GenericArgument::Type(_)),
                        )
                        | (
                            GenericArgumentTemplate::Resolved(bray_symbols::GenericArgument::Constant(_)),
                            GenericArgumentTemplate::Type(_),
                        )
                        | (
                            GenericArgumentTemplate::Type(_),
                            GenericArgumentTemplate::Resolved(bray_symbols::GenericArgument::Constant(_)),
                        ) => return Ok(false),
                        _ => {}
                    }
                }

                Ok(true)
            }
            (
                TypeExpressionTemplate::TypeValuedMemberProjection {
                    subject: left_subject,
                    application: left_application,
                    member: left_member,
                },
                TypeExpressionTemplate::TypeValuedMemberProjection {
                    subject: right_subject,
                    application: right_application,
                    member: right_member,
                },
            ) => {
                if left_member != right_member
                    || left_application.definition() != right_application.definition()
                {
                    return Ok(false);
                }

                self.type_templates_may_overlap(left_subject, right_subject)
            }
            (TypeExpressionTemplate::Tuple(left), TypeExpressionTemplate::Tuple(right)) => {
                if left.len() != right.len() {
                    return Ok(false);
                }

                for (left, right) in left.iter().zip(right.iter()) {
                    if !self.type_templates_may_overlap(left, right)? {
                        return Ok(false);
                    }
                }

                Ok(true)
            }
            (
                TypeExpressionTemplate::Array { element: left, .. },
                TypeExpressionTemplate::Array { element: right, .. },
            )
            | (TypeExpressionTemplate::Slice(left), TypeExpressionTemplate::Slice(right))
            | (TypeExpressionTemplate::Nullable(left), TypeExpressionTemplate::Nullable(right)) => {
                self.type_templates_may_overlap(left, right)
            }
            (
                TypeExpressionTemplate::Borrow {
                    kind: left_kind,
                    target: left_target,
                },
                TypeExpressionTemplate::Borrow {
                    kind: right_kind,
                    target: right_target,
                },
            ) => {
                if left_kind != right_kind {
                    return Ok(false);
                }

                self.type_templates_may_overlap(left_target, right_target)
            }
            (TypeExpressionTemplate::TraitView(left), TypeExpressionTemplate::TraitView(right)) => {
                Ok(left.definition() == right.definition())
            }
            (
                TypeExpressionTemplate::OwnedIndirection {
                    storage: left_storage,
                    target: left_target,
                },
                TypeExpressionTemplate::OwnedIndirection {
                    storage: right_storage,
                    target: right_target,
                },
            ) => {
                if !self.type_templates_may_overlap(left_storage, right_storage)? {
                    return Ok(false);
                }

                self.type_templates_may_overlap(left_target, right_target)
            }
            (TypeExpressionTemplate::Callable(left), TypeExpressionTemplate::Callable(right)) => {
                if left.parameters().len() != right.parameters().len() {
                    return Ok(false);
                }

                for (left, right) in left.parameters().iter().zip(right.parameters()) {
                    if left.name() != right.name()
                        || left.position() != right.position()
                        || left.mode() != right.mode()
                        || !self.type_templates_may_overlap(left.ty(), right.ty())?
                    {
                        return Ok(false);
                    }
                }

                self.type_templates_may_overlap(left.result(), right.result())
            }
            (TypeExpressionTemplate::Resolved(_), _) | (_, TypeExpressionTemplate::Resolved(_)) => {
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn bind_type_parameter(
        &mut self,
        parameter: GenericTypeParameterSymbolId,
        value: TypeId,
    ) -> Result<bool, SemanticValueStoreError> {
        if self.type_contains_parameter(value, parameter, &mut BTreeSet::new())? {
            return Ok(false);
        }

        self.type_bindings.insert(parameter, value);

        Ok(true)
    }

    fn header_type_parameter(
        &self,
        ty: TypeId,
    ) -> Result<Option<GenericTypeParameterSymbolId>, SemanticValueStoreError> {
        let ty = self.values.type_data(ty)?;

        Ok(match ty.as_ref() {
            TypeData::TypeParameter(parameter)
                if self
                    .parameters
                    .contains(&GenericParameterSymbolId::Type(*parameter)) =>
            {
                Some(*parameter)
            }
            _ => None,
        })
    }

    pub(super) fn trait_applications_may_overlap(
        &mut self,
        left: TraitApplicationId,
        right: TraitApplicationId,
    ) -> Result<bool, SemanticValueStoreError> {
        let left = self.values.trait_application_data(left)?;
        let right = self.values.trait_application_data(right)?;

        if left.definition() != right.definition() {
            return Ok(false);
        }

        self.substitutions_may_overlap(left.substitution(), right.substitution())
    }

    fn substitutions_may_overlap(
        &mut self,
        left: bray_symbols::GenericSubstitutionId,
        right: bray_symbols::GenericSubstitutionId,
    ) -> Result<bool, SemanticValueStoreError> {
        let left = self.values.generic_substitution_data(left)?;
        let right = self.values.generic_substitution_data(right)?;

        if left.owner() != right.owner() || left.bindings().len() != right.bindings().len() {
            return Ok(false);
        }

        for (left, right) in left.bindings().iter().zip(right.bindings()) {
            if left.parameter() != right.parameter()
                || !self.arguments_may_overlap(left.argument(), right.argument())?
            {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn arguments_may_overlap(
        &mut self,
        left: GenericArgument,
        right: GenericArgument,
    ) -> Result<bool, SemanticValueStoreError> {
        match (left, right) {
            (GenericArgument::Type(left), GenericArgument::Type(right)) => {
                self.types_may_overlap(left, right)
            }
            (GenericArgument::Constant(left), GenericArgument::Constant(right)) => {
                self.constants_may_overlap(left, right)
            }
            _ => Ok(false),
        }
    }

    fn type_lists_may_overlap(
        &mut self,
        left: &[TypeId],
        right: &[TypeId],
    ) -> Result<bool, SemanticValueStoreError> {
        if left.len() != right.len() {
            return Ok(false);
        }

        for (left, right) in left.iter().zip(right) {
            if !self.types_may_overlap(*left, *right)? {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn constants_may_overlap(
        &mut self,
        left: ConstantTermId,
        right: ConstantTermId,
    ) -> Result<bool, SemanticValueStoreError> {
        if left == right {
            return Ok(true);
        }

        if let Some(parameter) = self.header_const_parameter(left)? {
            if let Some(bound) = self.constant_bindings.get(&parameter).copied() {
                return self.constants_may_overlap(bound, right);
            }

            return self.bind_const_parameter(parameter, right);
        }

        if let Some(parameter) = self.header_const_parameter(right)? {
            if let Some(bound) = self.constant_bindings.get(&parameter).copied() {
                return self.constants_may_overlap(left, bound);
            }

            return self.bind_const_parameter(parameter, left);
        }

        let left = self.values.constant_term_data(left)?;
        let right = self.values.constant_term_data(right)?;

        match (left.as_ref(), right.as_ref()) {
            (ConstantTermData::Value(left), ConstantTermData::Value(right)) => Ok(left == right),
            (
                ConstantTermData::IntegerLiteral {
                    ty: left_type,
                    value: left_value,
                },
                ConstantTermData::IntegerLiteral {
                    ty: right_type,
                    value: right_value,
                },
            ) => Ok(left_type == right_type && left_value == right_value),
            (ConstantTermData::Parameter(left), ConstantTermData::Parameter(right)) => {
                Ok(left == right)
            }
            _ => Ok(true),
        }
    }

    fn bind_const_parameter(
        &mut self,
        parameter: GenericConstParameterSymbolId,
        value: ConstantTermId,
    ) -> Result<bool, SemanticValueStoreError> {
        if self.constant_contains_parameter(value, parameter, &mut BTreeSet::new())? {
            return Ok(false);
        }

        self.constant_bindings.insert(parameter, value);

        Ok(true)
    }

    fn header_const_parameter(
        &self,
        term: ConstantTermId,
    ) -> Result<Option<GenericConstParameterSymbolId>, SemanticValueStoreError> {
        let term = self.values.constant_term_data(term)?;

        Ok(match term.as_ref() {
            ConstantTermData::Parameter(parameter)
                if self
                    .parameters
                    .contains(&GenericParameterSymbolId::Const(*parameter)) =>
            {
                Some(*parameter)
            }
            _ => None,
        })
    }

    fn callables_may_overlap(
        &mut self,
        left: &bray_symbols::CallableTypeData,
        right: &bray_symbols::CallableTypeData,
    ) -> Result<bool, SemanticValueStoreError> {
        if left.constness() != right.constness()
            || left.execution() != right.execution()
            || left.trust() != right.trust()
            || left.abi() != right.abi()
            || left.dependency_contracts() != right.dependency_contracts()
            || left.parameters().len() != right.parameters().len()
        {
            return Ok(false);
        }

        for (left, right) in left.parameters().iter().zip(right.parameters()) {
            if !self.callable_parameters_may_overlap(left, right)? {
                return Ok(false);
            }
        }

        self.types_may_overlap(left.result(), right.result())
    }

    fn callable_parameters_may_overlap(
        &mut self,
        left: &CallableParameterData,
        right: &CallableParameterData,
    ) -> Result<bool, SemanticValueStoreError> {
        if left.name() != right.name()
            || left.position() != right.position()
            || left.mode() != right.mode()
        {
            return Ok(false);
        }

        self.types_may_overlap(left.ty(), right.ty())
    }

    fn type_contains_parameter(
        &self,
        ty: TypeId,
        parameter: GenericTypeParameterSymbolId,
        visited: &mut BTreeSet<TypeId>,
    ) -> Result<bool, SemanticValueStoreError> {
        if !visited.insert(ty) {
            return Ok(false);
        }

        let ty = self.values.type_data(ty)?;

        match ty.as_ref() {
            TypeData::TypeParameter(candidate) => Ok(*candidate == parameter),
            TypeData::Named { substitution, .. } => {
                self.substitution_contains_type_parameter(*substitution, parameter, visited)
            }
            TypeData::TypeValuedMemberProjection {
                subject,
                application,
                ..
            } => {
                if self.type_contains_parameter(*subject, parameter, visited)? {
                    return Ok(true);
                }

                let application = self.values.trait_application_data(*application)?;

                self.substitution_contains_type_parameter(
                    application.substitution(),
                    parameter,
                    visited,
                )
            }
            TypeData::Tuple(elements) => {
                for element in &**elements {
                    if self.type_contains_parameter(*element, parameter, visited)? {
                        return Ok(true);
                    }
                }

                Ok(false)
            }
            TypeData::Array { element, .. }
            | TypeData::Slice(element)
            | TypeData::Generator(element)
            | TypeData::Nullable(element) => {
                self.type_contains_parameter(*element, parameter, visited)
            }
            TypeData::Borrow { target, .. } => {
                self.type_contains_parameter(*target, parameter, visited)
            }
            TypeData::TraitView(application) => {
                let application = self.values.trait_application_data(*application)?;

                self.substitution_contains_type_parameter(
                    application.substitution(),
                    parameter,
                    visited,
                )
            }
            TypeData::OwnedIndirection { storage, target } => {
                if self.type_contains_parameter(*storage, parameter, visited)? {
                    return Ok(true);
                }

                self.type_contains_parameter(*target, parameter, visited)
            }
            TypeData::Callable(callable) => {
                for callable_parameter in callable.parameters() {
                    if self.type_contains_parameter(callable_parameter.ty(), parameter, visited)? {
                        return Ok(true);
                    }
                }

                self.type_contains_parameter(callable.result(), parameter, visited)
            }
            TypeData::Error | TypeData::ContextualSelf(_) => Ok(false),
        }
    }

    fn substitution_contains_type_parameter(
        &self,
        substitution: bray_symbols::GenericSubstitutionId,
        parameter: GenericTypeParameterSymbolId,
        visited: &mut BTreeSet<TypeId>,
    ) -> Result<bool, SemanticValueStoreError> {
        let substitution = self.values.generic_substitution_data(substitution)?;

        for binding in substitution.bindings() {
            if let GenericArgument::Type(ty) = binding.argument()
                && self.type_contains_parameter(ty, parameter, visited)?
            {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn constant_contains_parameter(
        &self,
        term: ConstantTermId,
        parameter: GenericConstParameterSymbolId,
        visited: &mut BTreeSet<ConstantTermId>,
    ) -> Result<bool, SemanticValueStoreError> {
        if !visited.insert(term) {
            return Ok(false);
        }

        let term = self.values.constant_term_data(term)?;

        match term.as_ref() {
            ConstantTermData::Parameter(candidate) => Ok(*candidate == parameter),
            ConstantTermData::Unary { operand, .. }
            | ConstantTermData::Conversion { operand, .. }
            | ConstantTermData::NullablePresent(operand) => {
                self.constant_contains_parameter(*operand, parameter, visited)
            }
            ConstantTermData::Binary { left, right, .. } => {
                if self.constant_contains_parameter(*left, parameter, visited)? {
                    return Ok(true);
                }

                self.constant_contains_parameter(*right, parameter, visited)
            }
            ConstantTermData::Tuple(elements) | ConstantTermData::Array(elements) => {
                for element in &**elements {
                    if self.constant_contains_parameter(*element, parameter, visited)? {
                        return Ok(true);
                    }
                }

                Ok(false)
            }
            ConstantTermData::Product(fields) => {
                for field in &**fields {
                    if self.constant_contains_parameter(*field.value(), parameter, visited)? {
                        return Ok(true);
                    }
                }

                Ok(false)
            }
            ConstantTermData::Union { fields, .. } => {
                for field in &**fields {
                    if self.constant_contains_parameter(*field.value(), parameter, visited)? {
                        return Ok(true);
                    }
                }

                Ok(false)
            }
            ConstantTermData::DefinitionApplication { substitution, .. } => {
                self.substitution_contains_const_parameter(*substitution, parameter, visited)
            }
            ConstantTermData::Call {
                callable,
                arguments,
                ..
            } => {
                let callable = self.values.callable_instance_data(*callable)?;

                if self.substitution_contains_const_parameter(
                    callable.substitution(),
                    parameter,
                    visited,
                )? {
                    return Ok(true);
                }

                for argument in &**arguments {
                    if self.constant_contains_parameter(*argument, parameter, visited)? {
                        return Ok(true);
                    }
                }

                Ok(false)
            }
            ConstantTermData::PredicateCall {
                predicate,
                arguments,
            } => {
                if self.substitution_contains_const_parameter(
                    predicate.substitution(),
                    parameter,
                    visited,
                )? {
                    return Ok(true);
                }

                for argument in &**arguments {
                    if self.constant_contains_parameter(*argument, parameter, visited)? {
                        return Ok(true);
                    }
                }

                Ok(false)
            }
            ConstantTermData::Projection(projection) => {
                self.constant_contains_parameter(projection.subject(), parameter, visited)
            }
            ConstantTermData::Value(_)
            | ConstantTermData::IntegerLiteral { .. }
            | ConstantTermData::TargetFact(_) => Ok(false),
        }
    }

    fn substitution_contains_const_parameter(
        &self,
        substitution: bray_symbols::GenericSubstitutionId,
        parameter: GenericConstParameterSymbolId,
        visited: &mut BTreeSet<ConstantTermId>,
    ) -> Result<bool, SemanticValueStoreError> {
        let substitution = self.values.generic_substitution_data(substitution)?;

        for binding in substitution.bindings() {
            if let GenericArgument::Constant(term) = binding.argument()
                && self.constant_contains_parameter(term, parameter, visited)?
            {
                return Ok(true);
            }
        }

        Ok(false)
    }
}
