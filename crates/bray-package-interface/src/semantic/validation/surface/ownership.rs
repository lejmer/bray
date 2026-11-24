use std::collections::BTreeMap;

use crate::{
    InterfaceConstantProjection, InterfaceConstantTerm, InterfaceConstantTermId,
    InterfaceConstantValueKind, InterfaceGenericArgument, InterfaceSemantics,
    InterfaceStorageShape, InterfaceSymbolReference, InterfaceType, InterfaceTypeId,
    InterfaceValidationError, InterfaceValidationField, PackageInterfaceSurface,
};

type Bindings = BTreeMap<InterfaceSymbolReference, InterfaceGenericArgument>;

fn invalid() -> InterfaceValidationError {
    crate::semantic::codec::invalid_value(InterfaceValidationField::Dependency)
}

#[cfg(test)]
mod tests {
    use crate::{
        InterfaceConstantProjection, InterfaceConstantTerm, InterfaceConstantTermId,
        InterfaceSemantics, InterfaceStorageShape, InterfaceSymbolReference, InterfaceType,
        InterfaceTypeId,
    };
    use bray_symbols::{DependencyOwnedTransfer, DependencyValuePreservation as Mask, SymbolOrdinal};

    #[test]
    fn masks_select_the_owned_input_projection_and_cannot_follow_borrows() {
        let bundle = crate::test_support::package_interface_export_bundle();
        let empty = InterfaceTypeId::new(0);
        let nullable = InterfaceTypeId::new(1);
        let tuple = InterfaceTypeId::new(2);
        let borrowed = InterfaceTypeId::new(3);
        let input = InterfaceConstantTermId::new(0);
        let selected = InterfaceConstantTermId::new(1);
        let field = InterfaceConstantProjection::TupleElement(SymbolOrdinal::new(0));

        let semantics = InterfaceSemantics::new().with_values([], [
            InterfaceType::Tuple([].into()),
            InterfaceType::Nullable(empty),
            InterfaceType::Tuple([nullable].into()),
            InterfaceType::Borrow { kind: bray_symbols::BorrowKind::Mutable, target: tuple },
        ], [], [
            InterfaceConstantTerm::CallableArgument(SymbolOrdinal::new(0)),
            InterfaceConstantTerm::Projection { subject: input, kind: field.clone() },
        ]);

        let transfer = DependencyOwnedTransfer::new(selected, []).with_preservation(Mask::new(true, [(
            InterfaceConstantProjection::NullableValue, Mask::none(),
        )].into()));

        assert!(semantics.validate_owned_preservation(&transfer, &[borrowed], bundle.surface()).is_ok());

        let malformed = transfer.clone().with_preservation(Mask::new(true, [(field.clone(), Mask::none())].into()));

        assert!(semantics.validate_owned_preservation(&malformed, &[borrowed], bundle.surface()).is_err());

        let reached = DependencyOwnedTransfer::new(input, []).with_preservation(Mask::new(true, [(field, Mask::none())].into()));

        assert!(semantics.validate_owned_preservation(&reached, &[borrowed], bundle.surface()).is_err());
    }

    #[test]
    fn masks_resolve_named_and_contextual_self_owners() {
        use crate::{InterfaceGenericSubstitution, InterfaceGenericSubstitutionId, InterfaceImplementationRecord, InterfaceProductIdentity, InterfaceStorageMember, InterfaceTypeRepresentation};
        use bray_symbols::{PackageIdentity, SymbolKind};

        let package = PackageIdentity::try_new("example.preservation").unwrap();
        let module = crate::test_support::module_key(package.clone(), "api");
        let first = crate::test_support::named_key(module.clone(), SymbolKind::Struct, "First");
        let second = crate::test_support::named_key(module.clone(), SymbolKind::Struct, "Second");
        let implementation = crate::test_support::named_key(module.clone(), SymbolKind::InherentImplementation, "FirstImpl");
        let trait_key = crate::test_support::named_key(module, SymbolKind::Trait, "Unrelated");
        let first_field = crate::test_support::named_key(first.clone(), SymbolKind::StructField, "value");
        let second_field = crate::test_support::named_key(second.clone(), SymbolKind::StructField, "value");

        let surface = crate::test_support::identity_surface(package, InterfaceProductIdentity::try_new("library").unwrap(),
            [first.clone(), second.clone(), implementation.clone(), trait_key.clone(), first_field.clone(), second_field.clone()], []);

        let reference = |key| InterfaceSymbolReference::Local(surface.symbol_by_external_key(key).unwrap());
        let first = reference(&first);
        let first_field = reference(&first_field);
        let second_field = reference(&second_field);
        let implementation = reference(&implementation);
        let trait_key = reference(&trait_key);
        let scalar = InterfaceTypeId::new(0);
        let owner = InterfaceTypeId::new(1);
        let input = InterfaceConstantTermId::new(0);

        let semantics = InterfaceSemantics {
            substitutions: [InterfaceGenericSubstitution::new(first.clone(), [])].into(),
            types: [InterfaceType::Tuple([].into()), InterfaceType::Named {
                definition: first.clone(), substitution: InterfaceGenericSubstitutionId::new(0),
            }, InterfaceType::ContextualSelf(first.clone()), InterfaceType::ContextualSelf(implementation.clone()),
                InterfaceType::ContextualSelf(trait_key)].into(),
            constant_terms: [InterfaceConstantTerm::CallableArgument(SymbolOrdinal::new(0))].into(),
            type_representations: [InterfaceTypeRepresentation::new(first).with_storage(InterfaceStorageShape::Structure([
                InterfaceStorageMember::new(Some(first_field.clone()), scalar),
            ].into()))].into(),
            implementations: [InterfaceImplementationRecord::new(implementation, owner, None)].into(),
            ..InterfaceSemantics::default()
        };

        for (field, valid) in [(first_field, true), (second_field, false)] {
            let transfer = DependencyOwnedTransfer::new(input, []).with_preservation(Mask::new(true, [(
                InterfaceConstantProjection::ProductField(field), Mask::none(),
            )].into()));

            for ty in [owner, InterfaceTypeId::new(2), InterfaceTypeId::new(3)] {
                assert_eq!(semantics.validate_owned_preservation(&transfer, &[ty], &surface).is_ok(), valid);
            }

            assert!(semantics.validate_owned_preservation(&transfer, &[InterfaceTypeId::new(4)], &surface).is_err());
        }
    }

    #[test]
    fn array_masks_allow_symbolic_lengths_and_reject_known_out_of_bounds() {
        use crate::InterfaceProductIdentity;
        use bray_symbols::{IntegerConstant, PackageIdentity, SymbolKind, TargetSizedIntegerType};

        let package = PackageIdentity::try_new("example.arraypreservation").unwrap();
        let module = crate::test_support::module_key(package.clone(), "api");
        let owner = crate::test_support::named_key(module, SymbolKind::Struct, "Owner");
        let parameter = crate::test_support::named_key(owner.clone(), SymbolKind::GenericConstParameter, "N");

        let surface = crate::test_support::identity_surface(
            package, InterfaceProductIdentity::try_new("library").unwrap(),
            [owner, parameter.clone()], [],
        );

        let parameter = InterfaceSymbolReference::Local(surface.symbol_by_external_key(&parameter).unwrap());
        let input = InterfaceConstantTermId::new(0);
        let symbolic = InterfaceTypeId::new(1);
        let fixed = InterfaceTypeId::new(2);

        let semantics = InterfaceSemantics::new().with_values([], [
            InterfaceType::Tuple([].into()),
            InterfaceType::Array { element: InterfaceTypeId::new(0), length: InterfaceConstantTermId::new(1) },
            InterfaceType::Array { element: InterfaceTypeId::new(0), length: InterfaceConstantTermId::new(2) },
        ], [], [
            InterfaceConstantTerm::CallableArgument(SymbolOrdinal::new(0)),
            InterfaceConstantTerm::Parameter(parameter),
            InterfaceConstantTerm::IntegerLiteral { ty: TargetSizedIntegerType::Usize, value: IntegerConstant::from_u64(1) },
        ]);

        for (ty, index, valid) in [(symbolic, 0, true), (fixed, 0, true), (fixed, 1, false)] {
            let transfer = DependencyOwnedTransfer::new(input, []).with_preservation(Mask::new(true, [(
                InterfaceConstantProjection::ArrayElementOrdinal(SymbolOrdinal::new(index)), Mask::none(),
            )].into()));

            assert_eq!(semantics.validate_owned_preservation(&transfer, &[ty], &surface).is_ok(), valid);
        }
    }
}

impl InterfaceSemantics {
    pub(in crate::semantic::validation) fn validate_owned_preservation(
        &self,
        transfer: &bray_symbols::DependencyOwnedTransfer<InterfaceConstantProjection, InterfaceConstantTermId>,
        inputs: &[InterfaceTypeId],
        surface: &PackageInterfaceSurface,
    ) -> Result<(), InterfaceValidationError> {
        if transfer.preservation().children().is_empty() {
            return Ok(());
        }

        let mut term = transfer.input();
        let mut projections = Vec::new();
        let mut remaining = bray_symbols::EXECUTION_CONDITION_WORK_LIMIT;

        let mut ty = loop {
            remaining = remaining.checked_sub(1).ok_or_else(invalid)?;

            match self.constant_terms.get(term.to_index().ok_or_else(invalid)?).ok_or_else(invalid)? {
                InterfaceConstantTerm::CallableArgument(ordinal) => {
                    let index = usize::try_from(ordinal.raw()).map_err(|_| invalid())?;

                    break *inputs.get(index).ok_or_else(invalid)?;
                }
                InterfaceConstantTerm::Projection { subject, kind } => {
                    projections.push(kind);
                    term = *subject;
                }
                _ => return Err(invalid()),
            }
        };

        let mut bindings = Bindings::new();

        for projection in projections.into_iter().rev() {
            ty = self.preservation_projection_type(ty, projection, &mut bindings, surface, true)?;
        }

        let mut pending = vec![(transfer.preservation(), ty, bindings)];

        while let Some((mask, ty, bindings)) = pending.pop() {
            remaining = remaining.checked_sub(1).ok_or_else(invalid)?;

            for (projection, child) in mask.children() {
                let mut bindings = bindings.clone();
                let ty = self.preservation_projection_type(ty, projection, &mut bindings, surface, false)?;

                pending.push((child, ty, bindings));
            }
        }

        Ok(())
    }

    fn preservation_projection_type(
        &self,
        mut ty: InterfaceTypeId,
        projection: &InterfaceConstantProjection,
        bindings: &mut Bindings,
        surface: &PackageInterfaceSurface,
        input_projection: bool,
    ) -> Result<InterfaceTypeId, InterfaceValidationError> {
        let mut remaining = bray_symbols::EXECUTION_CONDITION_WORK_LIMIT;

        let shape = loop {
            remaining = remaining.checked_sub(1).ok_or_else(invalid)?;

            let shape = self.types.get(ty.to_index().ok_or_else(invalid)?).ok_or_else(invalid)?;

            match shape {
                InterfaceType::Borrow { target, .. } if input_projection => ty = *target,
                InterfaceType::TypeParameter(parameter) => {
                    let Some(InterfaceGenericArgument::Type(target)) = bindings.get(parameter) else {
                        return Err(invalid());
                    };

                    ty = *target;
                }
                InterfaceType::ContextualSelf(context)
                    if super::reference::validate_symbol_kind(context, surface)?.is_implementation() =>
                {
                    ty = self
                        .implementations
                        .iter()
                        .find(|implementation| &implementation.implementation == context)
                        .map(|implementation| implementation.subject)
                        .ok_or_else(invalid)?;
                }
                _ => break shape,
            }
        };

        match (shape, projection) {
            (InterfaceType::Tuple(elements), InterfaceConstantProjection::TupleElement(ordinal)) => {
                let index = usize::try_from(ordinal.raw()).map_err(|_| invalid())?;

                elements.get(index).copied().ok_or_else(invalid)
            }
            (InterfaceType::Nullable(target), InterfaceConstantProjection::NullableValue) => Ok(*target),
            (InterfaceType::Array { element, length }, InterfaceConstantProjection::ArrayElementOrdinal(ordinal)) => {
                // Generic bounds can remain symbolic in a valid exported body.
                // A mask selects existing inline data and grants no access authority.
                if self.preservation_array_length(*length, bindings)?
                    .is_some_and(|length| u64::from(ordinal.raw()) >= length)
                {
                    return Err(invalid());
                }

                Ok(*element)
            }
            (InterfaceType::Named { definition, substitution }, InterfaceConstantProjection::ProductField(field))
            | (InterfaceType::Named { definition, substitution }, InterfaceConstantProjection::UnionPayloadField(field)) => {
                self.preservation_named_projection_type(definition, Some(*substitution), field, projection, bindings, surface)
            }
            (InterfaceType::ContextualSelf(definition), InterfaceConstantProjection::ProductField(field))
            | (InterfaceType::ContextualSelf(definition), InterfaceConstantProjection::UnionPayloadField(field))
                if matches!(super::reference::validate_symbol_kind(definition, surface)?, bray_symbols::SymbolKind::Struct | bray_symbols::SymbolKind::Union) =>
            {
                self.preservation_named_projection_type(definition, None, field, projection, bindings, surface)
            }
            _ => Err(invalid()),
        }
    }

    fn preservation_named_projection_type(
        &self,
        definition: &InterfaceSymbolReference,
        substitution: Option<crate::InterfaceGenericSubstitutionId>,
        field: &InterfaceSymbolReference,
        projection: &InterfaceConstantProjection,
        bindings: &mut Bindings,
        surface: &PackageInterfaceSurface,
    ) -> Result<InterfaceTypeId, InterfaceValidationError> {
                let kind = super::reference::validate_symbol_kind(field, surface)?;

                let expected = if matches!(projection, InterfaceConstantProjection::ProductField(_)) {
                    bray_symbols::SymbolKind::StructField
                } else {
                    bray_symbols::SymbolKind::UnionPayloadField
                };

                if kind != expected {
                    return Err(invalid());
                }

                let representation = self.type_representations.iter()
                    .find(|representation| representation.owner() == definition).ok_or_else(invalid)?;

                let member = match (representation.storage(), projection) {
                    (InterfaceStorageShape::Structure(members), InterfaceConstantProjection::ProductField(_)) => {
                        members.iter().find(|member| member.field() == Some(field))
                    }
                    (InterfaceStorageShape::Union(variants), InterfaceConstantProjection::UnionPayloadField(_)) => {
                        variants.iter().flat_map(|variant| variant.members()).find(|member| member.field() == Some(field))
                    }
                    _ => None,
                }.ok_or_else(invalid)?;

                if let Some(substitution) = substitution {
                    let substitution = self.substitutions.get(substitution.to_index().ok_or_else(invalid)?).ok_or_else(invalid)?;

                    for binding in &*substitution.bindings {
                        let argument = match &binding.argument {
                            InterfaceGenericArgument::Type(target) => match self.types.get(target.to_index().ok_or_else(invalid)?) {
                                Some(InterfaceType::TypeParameter(parameter)) => bindings.get(parameter).cloned().unwrap_or(binding.argument.clone()),
                                _ => binding.argument.clone(),
                            },
                            InterfaceGenericArgument::Constant(term) => match self.constant_terms.get(term.to_index().ok_or_else(invalid)?) {
                                Some(InterfaceConstantTerm::Parameter(parameter)) => bindings.get(parameter).cloned().unwrap_or(binding.argument.clone()),
                                _ => binding.argument.clone(),
                            },
                        };

                        bindings.insert(binding.parameter.clone(), argument);
                    }
                }

                Ok(member.ty())
    }

    fn preservation_array_length(&self, mut term: InterfaceConstantTermId, bindings: &Bindings) -> Result<Option<u64>, InterfaceValidationError> {
        for _ in 0..bray_symbols::EXECUTION_CONDITION_WORK_LIMIT {
            match self.constant_terms.get(term.to_index().ok_or_else(invalid)?).ok_or_else(invalid)? {
                InterfaceConstantTerm::Typed { term: inner, .. } => term = *inner,
                InterfaceConstantTerm::Parameter(parameter) => {
                    match bindings.get(parameter) {
                        Some(InterfaceGenericArgument::Constant(value)) => term = *value,
                        None => return Ok(None),
                        Some(_) => return Err(invalid()),
                    }
                }
                InterfaceConstantTerm::IntegerLiteral { value, .. } => return value.to_u64().map(Some).ok_or_else(invalid),
                InterfaceConstantTerm::Value(value) => {
                    let value = self.constant_values.get(value.to_index().ok_or_else(invalid)?).ok_or_else(invalid)?;

                    return match value.kind() {
                        InterfaceConstantValueKind::Integer(value) => value.to_u64().map(Some).ok_or_else(invalid),
                        _ => Err(invalid()),
                    };
                }
                _ => return Ok(None),
            }
        }

        Err(invalid())
    }
}
