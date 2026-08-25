use bray_package_interface::{
    DependencyInterfaceId, InterfaceConstantProjection, InterfaceConstantTerm,
    InterfaceConstantTermId, InterfaceConstantValue, InterfaceConstantValueId,
    InterfaceConstantValueKind, InterfaceGenericSubstitutionId, InterfaceStorageMember,
    InterfaceStorageShape, InterfaceSymbolReference, InterfaceTypeRepresentation,
    InterfaceUnionStorageVariant, InterfaceUnionTag,
};
use bray_symbols::{
    AnySymbolId, ConstantField, ConstantProjectionKind, ConstantTermData, ConstantTermId,
    ConstantValueData, ConstantValueId, ConstantValueKind, DeclaredStorageShape,
    GenericSubstitutionId, ImplementationInstanceId, SymbolKeyData, TypeId,
};

use super::super::PackageInterfaceExportError;

use super::context::{SemanticExporter, export_acyclic_semantic_value};
use super::implementation::incomplete_type;
use super::templates::{incomplete, index};

impl<'a> SemanticExporter<'a> {
    pub(in crate::compilation::export) fn constant_term_id(
        &mut self,
        id: ConstantTermId,
    ) -> Result<InterfaceConstantTermId, PackageInterfaceExportError> {
        if let Some(id) = self.constant_term_ids.get(&id) {
            return Ok(*id);
        }

        export_acyclic_semantic_value!(self, active_constant_terms, id, {
            let data = self
                .values
                .constant_term_data(id)
                .map_err(|_| incomplete_type())?;

            let term = match data.as_ref() {
            ConstantTermData::Value(value) => {
                InterfaceConstantTerm::Value(self.constant_value_id(*value)?)
            }
            ConstantTermData::IntegerLiteral { ty, value } => {
                InterfaceConstantTerm::IntegerLiteral {
                    ty: *ty,
                    value: value.clone(),
                }
            }
            ConstantTermData::Parameter(parameter) => {
                InterfaceConstantTerm::Parameter(self.symbol_reference((*parameter).into())?)
            }
            ConstantTermData::TargetProperty(semantics) => {
                InterfaceConstantTerm::TargetProperty(self.symbol_reference((*semantics).into())?)
            }
            ConstantTermData::Unary { operation, operand } => InterfaceConstantTerm::Unary {
                operation: *operation,
                operand: self.constant_term_id(*operand)?,
            },
            ConstantTermData::Binary {
                operation,
                left,
                right,
            } => InterfaceConstantTerm::Binary {
                operation: *operation,
                left: self.constant_term_id(*left)?,
                right: self.constant_term_id(*right)?,
            },
            ConstantTermData::Conversion { operand, target } => InterfaceConstantTerm::Conversion {
                operand: self.constant_term_id(*operand)?,
                target: self.type_id(*target)?,
            },
            ConstantTermData::NullablePresent(value) => {
                InterfaceConstantTerm::NullablePresent(self.constant_term_id(*value)?)
            }
            ConstantTermData::Tuple(values) => InterfaceConstantTerm::Tuple(
                values
                    .iter()
                    .copied()
                    .map(|value| self.constant_term_id(value))
                    .collect::<Result<Vec<_>, _>>()?
                    .into(),
            ),
            ConstantTermData::Array(values) => InterfaceConstantTerm::Array(
                values
                    .iter()
                    .copied()
                    .map(|value| self.constant_term_id(value))
                    .collect::<Result<Vec<_>, _>>()?
                    .into(),
            ),
            ConstantTermData::Product(fields) => InterfaceConstantTerm::Product(
                fields
                    .iter()
                    .map(|field| {
                        Ok(ConstantField::new(
                            self.symbol_reference((*field.field()).into())?,
                            self.constant_term_id(*field.value())?,
                        ))
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .into(),
            ),
            ConstantTermData::Union { variant, fields } => InterfaceConstantTerm::Union {
                variant: self.symbol_reference((*variant).into())?,
                fields: fields
                    .iter()
                    .map(|field| {
                        Ok(ConstantField::new(
                            self.symbol_reference((*field.field()).into())?,
                            self.constant_term_id(*field.value())?,
                        ))
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .into(),
            },
            ConstantTermData::DefinitionApplication {
                definition,
                substitution,
                selected_implementation,
            } => InterfaceConstantTerm::DefinitionApplication {
                definition: self.symbol_reference(definition.into_any())?,
                substitution: self.substitution_id(*substitution)?,
                selected_implementation: selected_implementation
                    .map(|instance| self.implementation_instance_id(instance))
                    .transpose()?,
            },
            ConstantTermData::Call {
                callable,
                selected_implementation,
                arguments,
            } => InterfaceConstantTerm::Call {
                callable: self.callable_instance_id(*callable)?,
                selected_implementation: selected_implementation
                    .map(|instance| self.implementation_instance_id(instance))
                    .transpose()?,
                arguments: arguments
                    .iter()
                    .copied()
                    .map(|argument| self.constant_term_id(argument))
                    .collect::<Result<Vec<_>, _>>()?
                    .into(),
            },
            ConstantTermData::PredicateCall {
                predicate,
                arguments,
            } => InterfaceConstantTerm::PredicateCall {
                predicate: self.symbol_reference(predicate.definition().into_any())?,
                substitution: self.substitution_id(predicate.substitution())?,
                arguments: arguments
                    .iter()
                    .copied()
                    .map(|argument| self.constant_term_id(argument))
                    .collect::<Result<Vec<_>, _>>()?
                    .into(),
            },
            ConstantTermData::Projection(projection) => InterfaceConstantTerm::Projection {
                subject: self.constant_term_id(projection.subject())?,
                kind: match projection.kind() {
                    ConstantProjectionKind::TupleElement(ordinal) => {
                        InterfaceConstantProjection::TupleElement(ordinal)
                    }
                    ConstantProjectionKind::ArrayElement(index) => {
                        InterfaceConstantProjection::ArrayElement(self.constant_term_id(index)?)
                    }
                    ConstantProjectionKind::ProductField(field) => {
                        InterfaceConstantProjection::ProductField(
                            self.symbol_reference(field.into())?,
                        )
                    }
                    ConstantProjectionKind::UnionPayloadField(field) => {
                        InterfaceConstantProjection::UnionPayloadField(
                            self.symbol_reference(field.into())?,
                        )
                    }
                    ConstantProjectionKind::NullableValue => {
                        InterfaceConstantProjection::NullableValue
                    }
                },
            },
            };

            let exported = InterfaceConstantTermId::new(index(self.constant_terms.len())?);

            self.constant_terms.push(term);
            self.constant_term_ids.insert(id, exported);

            Ok(exported)
        })
    }

    pub(in crate::compilation::export) fn constant_value_term_id(
        &mut self,
        value: ConstantValueId,
    ) -> Result<InterfaceConstantTermId, PackageInterfaceExportError> {
        let term = self
            .values
            .intern_constant_term(ConstantTermData::Value(value))
            .map_err(|_| incomplete_type())?;

        self.constant_term_id(term)
    }

    pub(in crate::compilation::export) fn nullable_absence_term_id(
        &mut self,
        ty: TypeId,
    ) -> Result<InterfaceConstantTermId, PackageInterfaceExportError> {
        let value = self
            .values
            .intern_constant_value(ConstantValueData::new(
                ty,
                ConstantValueKind::NullableAbsent,
            ))
            .map_err(|_| incomplete_type())?;

        self.constant_value_term_id(value)
    }

    pub(in crate::compilation::export) fn declaration_template_reference(
        &mut self,
        declaration: AnySymbolId,
        substitution: GenericSubstitutionId,
    ) -> Result<
        (
            bray_package_interface::InterfaceTemplateReference,
            InterfaceGenericSubstitutionId,
        ),
        PackageInterfaceExportError,
    > {
        Ok((
            bray_package_interface::InterfaceTemplateReference::Symbol(
                self.symbol_reference(declaration)?,
            ),
            self.substitution_id(substitution)?,
        ))
    }

    pub(in crate::compilation::export) fn implementation_template_reference(
        &mut self,
        instance: ImplementationInstanceId,
    ) -> Result<
        (
            bray_package_interface::InterfaceImplementationReference,
            InterfaceGenericSubstitutionId,
        ),
        PackageInterfaceExportError,
    > {
        let data = self
            .values
            .implementation_instance_data(instance)
            .map_err(|_| incomplete_type())?;

        Ok((
            bray_package_interface::InterfaceImplementationReference::Symbol(
                self.symbol_reference(data.definition().into_any())?,
            ),
            self.substitution_id(data.substitution())?,
        ))
    }

    pub(in crate::compilation::export) fn constant_value_id(
        &mut self,
        id: ConstantValueId,
    ) -> Result<InterfaceConstantValueId, PackageInterfaceExportError> {
        if let Some(id) = self.constant_value_ids.get(&id) {
            return Ok(*id);
        }

        export_acyclic_semantic_value!(self, active_constant_values, id, {
            let data = self
                .values
                .constant_value_data(id)
                .map_err(|_| incomplete_type())?;

            let kind = match data.kind() {
            ConstantValueKind::Error | ConstantValueKind::StaticAddress(_) => {
                return Err(incomplete_type());
            }
            ConstantValueKind::Boolean(value) => InterfaceConstantValueKind::Boolean(*value),
            ConstantValueKind::Character(value) => InterfaceConstantValueKind::Character(*value),
            ConstantValueKind::Integer(value) => InterfaceConstantValueKind::Integer(value.clone()),
            ConstantValueKind::Real(value) => InterfaceConstantValueKind::Real(*value),
            ConstantValueKind::Complex { real, imaginary } => InterfaceConstantValueKind::Complex {
                real: *real,
                imaginary: *imaginary,
            },
            ConstantValueKind::String(value) => InterfaceConstantValueKind::String(value.clone()),
            ConstantValueKind::Unit => InterfaceConstantValueKind::Unit,
            ConstantValueKind::NullableAbsent => InterfaceConstantValueKind::NullableAbsent,
            ConstantValueKind::NullablePresent(value) => {
                InterfaceConstantValueKind::NullablePresent(self.constant_value_id(*value)?)
            }
            ConstantValueKind::Tuple(values) => {
                InterfaceConstantValueKind::Tuple(self.constant_value_ids(values)?.into())
            }
            ConstantValueKind::Array(values) => {
                InterfaceConstantValueKind::Array(self.constant_value_ids(values)?.into())
            }
            ConstantValueKind::Product(fields) => InterfaceConstantValueKind::Product(
                fields
                    .iter()
                    .map(|field| {
                        Ok(ConstantField::new(
                            self.symbol_reference((*field.field()).into())?,
                            self.constant_value_id(*field.value())?,
                        ))
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .into(),
            ),
            ConstantValueKind::Union { variant, fields } => InterfaceConstantValueKind::Union {
                variant: self.symbol_reference((*variant).into())?,
                fields: fields
                    .iter()
                    .map(|field| {
                        Ok(ConstantField::new(
                            self.symbol_reference((*field.field()).into())?,
                            self.constant_value_id(*field.value())?,
                        ))
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .into(),
            },
            };

            let exported = InterfaceConstantValueId::new(index(self.constant_values.len())?);
            let ty = self.type_id(data.ty())?;

            self.constant_values
                .push(InterfaceConstantValue::new(ty, kind));

            self.constant_value_ids.insert(id, exported);

            Ok(exported)
        })
    }

    pub(super) fn constant_value_ids(
        &mut self,
        values: &[ConstantValueId],
    ) -> Result<Vec<InterfaceConstantValueId>, PackageInterfaceExportError> {
        values
            .iter()
            .copied()
            .map(|value| self.constant_value_id(value))
            .collect()
    }

    pub(super) fn type_representation(
        &mut self,
        owner: AnySymbolId,
        representation: &bray_symbols::DeclaredTypeRepresentation,
    ) -> Result<InterfaceTypeRepresentation, PackageInterfaceExportError> {
        let union_tag_type = representation
            .union_tag_type()
            .map(|ty| self.type_id(ty))
            .transpose()?;

        let union_tags = representation
            .union_tags()
            .iter()
            .map(|tag| {
                Ok(InterfaceUnionTag::new(
                    self.symbol_reference(tag.variant().into())?,
                    tag.value().clone(),
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let copy_dependencies = representation
            .copy_dependencies()
            .iter()
            .copied()
            .map(|parameter| self.symbol_reference(parameter.into()))
            .collect::<Result<Vec<_>, _>>()?;

        let storage = match representation.storage() {
            DeclaredStorageShape::Structure(members) => InterfaceStorageShape::Structure(
                self.storage_members(
                    owner,
                    members
                        .iter()
                        .map(|member| (member.field().map(AnySymbolId::from), member.ty())),
                )?
                .into(),
            ),
            DeclaredStorageShape::Union(variants) => InterfaceStorageShape::Union(
                variants
                    .iter()
                    .map(|variant| {
                        let members = self.storage_members(
                            owner,
                            variant
                                .members()
                                .iter()
                                .map(|member| (member.field().map(AnySymbolId::from), member.ty())),
                        )?;

                        Ok(InterfaceUnionStorageVariant::new(
                            self.symbol_reference(variant.variant().into())?,
                            members,
                        ))
                    })
                    .collect::<Result<Vec<_>, PackageInterfaceExportError>>()?
                    .into(),
            ),
        };

        Ok(
            InterfaceTypeRepresentation::new(self.symbol_reference(owner)?)
                .with_layout(
                    representation.layout(),
                    representation.alignment(),
                    representation.packing(),
                    union_tag_type,
                )
                .with_union_tags(union_tags)
                .with_opaque_storage(representation.opaque_size(), representation.is_incomplete())
                .with_tagless_union(representation.is_tagless_union())
                .with_storage(storage)
                .with_copy(representation.copy_contract(), copy_dependencies)
                .with_properties(
                    representation.is_plain_storage(),
                    representation.has_finite_size(),
                ),
        )
    }

    fn storage_members<'b>(
        &mut self,
        owner: AnySymbolId,
        members: impl IntoIterator<
            Item = (
                Option<AnySymbolId>,
                &'b bray_symbols::TypeExpressionTemplate,
            ),
        >,
    ) -> Result<Vec<InterfaceStorageMember>, PackageInterfaceExportError> {
        members
            .into_iter()
            .map(|(field, ty)| {
                let field = field
                    .filter(|field| self.keys.contains_key(field))
                    .map(|field| self.symbol_reference(field))
                    .transpose()?;

                let ty = self.resolve_type_template(owner, ty)?;

                Ok(InterfaceStorageMember::new(field, self.type_id(ty)?))
            })
            .collect()
    }

    pub(in crate::compilation::export) fn symbol_reference(
        &self,
        symbol: AnySymbolId,
    ) -> Result<InterfaceSymbolReference, PackageInterfaceExportError> {
        if let Some(key) = self.keys.get(&symbol) {
            let id = self
                .surface
                .symbol_by_external_key(key)
                .ok_or_else(|| incomplete(symbol))?;

            return Ok(InterfaceSymbolReference::Local(id));
        }

        match self.graph.symbol_key(symbol).map(|key| key.data()) {
            Some(SymbolKeyData::CompilerKnownDeclaration { .. })
            | Some(SymbolKeyData::Synthesized(_)) => {
                let key = self
                    .graph
                    .symbol_key(symbol)
                    .cloned()
                    .ok_or_else(|| incomplete(symbol))?;

                let reference = bray_package_interface::CompilerKnownSymbolReference::try_new(key)
                    .ok_or_else(|| incomplete(symbol))?;

                Ok(InterfaceSymbolReference::CompilerKnown(reference))
            }
            Some(SymbolKeyData::External(key)) => {
                let dependency = self
                    .surface
                    .dependencies()
                    .iter()
                    .position(|dependency| dependency.package() == key.package_identity())
                    .and_then(|index| u32::try_from(index).ok())
                    .map(DependencyInterfaceId::new)
                    .ok_or_else(|| incomplete(symbol))?;

                Ok(InterfaceSymbolReference::Dependency {
                    dependency,
                    key: key.clone(),
                })
            }
            _ => Err(incomplete(symbol)),
        }
    }
}
