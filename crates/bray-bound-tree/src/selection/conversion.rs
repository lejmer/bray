use std::sync::Arc;

use bray_symbols::{CallableInstanceData, ImplementationSelectionKey, TypeId};

/// A selected built-in scalar conversion operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ScalarConversionKind {
    /// A value-preserving widening within the signed integer domain.
    SignedIntegerWidening,
    /// A value-preserving widening within the unsigned integer domain.
    UnsignedIntegerWidening,
    /// A value-preserving unsigned-to-signed integer conversion.
    UnsignedToSigned,
    /// An exact integer-to-real conversion.
    IntegerToReal,
    /// A value-preserving real-format widening.
    RealWidening,
    /// A value-preserving complex-format widening.
    ComplexWidening,
}

/// The exact rule used by one level of an explicit conversion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConversionTarget {
    /// Source and target are the same semantic type.
    Identity,
    /// A compiler-defined total value-preserving scalar conversion.
    BuiltInScalar(ScalarConversionKind),
    /// Element-wise tuple conversion with unchanged arity.
    Tuple(Arc<[SelectedConversion]>),
    /// Element-wise fixed-array conversion with unchanged length.
    Array(Box<SelectedConversion>),
    /// Present-value conversion preserving nullable absence.
    Nullable(Box<SelectedConversion>),
    /// Two-element tuple conversion into a built-in complex value.
    TupleToComplex {
        /// Conversion of the first tuple element into the real component.
        real: Box<SelectedConversion>,
        /// Conversion of the second tuple element into the imaginary component.
        imaginary: Box<SelectedConversion>,
    },
    /// A selected `ConvertTo<Target>` member and implementation witness.
    Trait {
        /// The exact substituted conversion member.
        callable: CallableInstanceData,
        /// The exact conversion trait requirement.
        requirement: ImplementationSelectionKey,
        /// The exact implementation witness.
        witness: bray_symbols::ImplementationInstanceId,
    },
}

/// One exact source-to-target conversion plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedConversion {
    source_type: TypeId,
    target_type: TypeId,
    target: ConversionTarget,
}

impl SelectedConversion {
    /// Creates one explicit conversion plan.
    pub const fn new(source_type: TypeId, target_type: TypeId, target: ConversionTarget) -> Self {
        Self {
            source_type,
            target_type,
            target,
        }
    }

    /// Returns the converted source type.
    pub const fn source_type(&self) -> TypeId {
        self.source_type
    }

    /// Returns the explicit target type.
    pub const fn target_type(&self) -> TypeId {
        self.target_type
    }

    /// Returns the exact selected conversion rule.
    pub const fn target(&self) -> &ConversionTarget {
        &self.target
    }

    /// Walks this plan and every nested conversion in deterministic depth-first order.
    pub fn walk(&self) -> impl Iterator<Item = &SelectedConversion> {
        SelectedConversionWalk {
            pending: vec![self],
        }
    }

    pub(crate) fn is_structurally_valid(&self) -> bool {
        self.walk().all(|conversion| match conversion.target() {
            ConversionTarget::Identity => conversion.source_type() == conversion.target_type(),
            ConversionTarget::BuiltInScalar(_)
            | ConversionTarget::Tuple(_)
            | ConversionTarget::Array(_)
            | ConversionTarget::Nullable(_)
            | ConversionTarget::TupleToComplex { .. }
            | ConversionTarget::Trait { .. } => true,
        })
    }
}

/// Deterministic depth-first traversal over one selected conversion plan.
struct SelectedConversionWalk<'plan> {
    pending: Vec<&'plan SelectedConversion>,
}

impl<'plan> Iterator for SelectedConversionWalk<'plan> {
    type Item = &'plan SelectedConversion;

    fn next(&mut self) -> Option<Self::Item> {
        let conversion = self.pending.pop()?;

        match conversion.target() {
            ConversionTarget::Tuple(children) => self.pending.extend(children.iter().rev()),
            ConversionTarget::Array(child) | ConversionTarget::Nullable(child) => {
                self.pending.push(child)
            }
            ConversionTarget::TupleToComplex { real, imaginary } => {
                self.pending.push(imaginary);
                self.pending.push(real);
            }
            ConversionTarget::Identity
            | ConversionTarget::BuiltInScalar(_)
            | ConversionTarget::Trait { .. } => {}
        }

        Some(conversion)
    }
}
