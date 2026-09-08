use super::{
    ConstantField, ConstantProjection, ConstantProjectionKind, ConstantTermData, ConstantTermId,
};

impl ConstantTermData {
    /// Rewrites the immediate term operands without changing their surrounding operation.
    ///
    /// Types, symbols, generic substitutions, witnesses, and closed values are unchanged. The
    /// caller decides whether to traverse operands recursively and how to bound that traversal.
    pub fn try_map_terms<E>(
        &self,
        mut map: impl FnMut(ConstantTermId) -> Result<ConstantTermId, E>,
    ) -> Result<Self, E> {
        Ok(match self {
            Self::Typed { term, ty } => Self::typed(map(*term)?, *ty),
            Self::Unary { operation, operand } => Self::Unary {
                operation: *operation,
                operand: map(*operand)?,
            },
            Self::Binary {
                operation,
                left,
                right,
            } => Self::Binary {
                operation: *operation,
                left: map(*left)?,
                right: map(*right)?,
            },
            Self::Conversion { operand, target } => Self::Conversion {
                operand: map(*operand)?,
                target: *target,
            },
            Self::NullablePresent(value) => Self::NullablePresent(map(*value)?),
            Self::Tuple(values) => Self::tuple(
                values
                    .iter()
                    .copied()
                    .map(&mut map)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            Self::Array(values) => Self::array(
                values
                    .iter()
                    .copied()
                    .map(&mut map)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            Self::Product(fields) => Self::product(map_fields(fields, &mut map)?),
            Self::Union { variant, fields } => Self::union(*variant, map_fields(fields, &mut map)?),
            Self::Call {
                callable,
                selected_implementation,
                arguments,
            } => Self::call(
                *callable,
                *selected_implementation,
                arguments
                    .iter()
                    .copied()
                    .map(&mut map)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            Self::PredicateCall {
                predicate,
                arguments,
            } => Self::predicate_call(
                *predicate,
                arguments
                    .iter()
                    .copied()
                    .map(&mut map)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            Self::Test { subject, kind } => Self::Test {
                subject: map(*subject)?,
                kind: *kind,
            },
            Self::Projection(projection) => {
                let subject = map(projection.subject())?;

                let kind = match projection.kind() {
                    ConstantProjectionKind::ArrayElement(index) => {
                        ConstantProjectionKind::ArrayElement(map(index)?)
                    }
                    kind => kind,
                };

                Self::Projection(ConstantProjection::new(subject, kind))
            }
            Self::Value(_)
            | Self::IntegerLiteral { .. }
            | Self::Parameter(_)
            | Self::CallableArgument(_)
            | Self::TargetProperty(_)
            | Self::DefinitionApplication { .. } => {
                // These leaves contain only copyable identities and shared immutable literal data.
                self.clone()
            }
        })
    }
}

fn map_fields<I: Copy, E>(
    fields: &[ConstantField<I, ConstantTermId>],
    map: &mut impl FnMut(ConstantTermId) -> Result<ConstantTermId, E>,
) -> Result<Vec<ConstantField<I, ConstantTermId>>, E> {
    fields
        .iter()
        .map(|field| Ok(ConstantField::new(*field.field(), map(*field.value())?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use super::{ConstantProjection, ConstantProjectionKind, ConstantTermData};
    use crate::{ConstantTest, SemanticValueStore, SymbolOrdinal};

    #[test]
    fn term_rewriting_includes_projection_indices_and_shape_subjects() {
        let values = SemanticValueStore::try_new().unwrap();

        let first = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let second = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(1)))
            .unwrap();

        let projection = ConstantTermData::Projection(ConstantProjection::new(
            first,
            ConstantProjectionKind::ArrayElement(second),
        ));

        let mut visited = Vec::new();

        let rewritten = projection
            .try_map_terms::<Infallible>(|term| {
                visited.push(term);

                Ok(first)
            })
            .unwrap();

        assert_eq!(visited, [first, second]);

        assert_eq!(
            rewritten,
            ConstantTermData::Projection(ConstantProjection::new(
                first,
                ConstantProjectionKind::ArrayElement(first)
            ))
        );

        assert_eq!(
            ConstantTermData::Test {
                subject: first,
                kind: ConstantTest::NullablePresent
            }
            .try_map_terms::<Infallible>(|_| Ok(second)),
            Ok(ConstantTermData::Test {
                subject: second,
                kind: ConstantTest::NullablePresent
            })
        );
    }
}
