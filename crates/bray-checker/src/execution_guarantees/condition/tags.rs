use std::sync::Arc;

use super::{ExecutionCondition, ExecutionConditionKind as K};

impl ExecutionCondition {
    /// Retain the same physical tag snapshot independently of changed payload data.
    pub(crate) fn common_retained_discriminant(&self, other: &Self) -> Option<Self> {
        fn source(mut value: &ExecutionCondition) -> &ExecutionCondition {
            loop {
                let current = value.computed_value();

                match current.kind() {
                    K::Constructed(base, _) => value = base,
                    _ => return current,
                }
            }
        }

        let (left, right) = (source(self), source(other));

        let original = match (left.kind(), right.kind()) {
            (K::Discriminant(original), _) if original.as_ref() == right || left == right => original,
            (_, K::Discriminant(original)) if original.as_ref() == left || left == right => original,
            _ => return None,
        };

        Some(Self::new(K::Discriminant(Arc::clone(original))))
    }

    /// Select only the retained tag from the actual current snapshot.
    pub(crate) fn discriminant(&self) -> Self {
        match self.kind() {
            K::Observation { contents, .. } | K::Borrowed(contents)
            | K::Constructed(contents, _) => contents.discriminant(),
            K::Present(_) => Self::new(K::Present(Arc::new(Self::unknown()))),
            K::Tagged(variant, _) => Self::new(K::Tagged(*variant, Arc::new(Self::unknown()))),
            K::Literal(value) => match value.kind() {
                bray_symbols::ConstantValueKind::NullableAbsent => self.clone(),
                bray_symbols::ConstantValueKind::Union { variant, .. } =>
                    Self::new(K::Tagged(*variant, Arc::new(Self::unknown()))),
                _ => panic!("discriminant requires a nullable or union value: {self:?}"),
            },
            K::Discriminant(_) | K::Unknown => self.clone(),
            _ => Self::new(K::Discriminant(Arc::new(self.clone()))),
        }
    }

    pub(crate) fn known_presence(&self) -> Option<bool> {
        match self.tag_source().kind() {
            K::Present(_) => Some(true),
            K::Literal(value) if matches!(value.kind(), bray_symbols::ConstantValueKind::NullableAbsent) => Some(false),
            _ => None,
        }
    }

    pub(crate) fn known_absence(&self) -> Option<&Arc<bray_symbols::ConstantValueData>> {
        match self.tag_source().kind() {
            K::Literal(value) if matches!(value.kind(), bray_symbols::ConstantValueKind::NullableAbsent) => Some(value),
            _ => None,
        }
    }

    pub(crate) fn known_variant(&self) -> Option<bray_symbols::UnionVariantSymbolId> {
        match self.tag_source().kind() {
            K::Tagged(variant, _) => Some(*variant),
            K::Literal(literal) => match literal.kind() {
                bray_symbols::ConstantValueKind::Union { variant, .. } => Some(*variant),
                _ => None,
            },
            _ => None,
        }
    }

    fn tag_source(&self) -> &Self {
        let mut value = self;

        loop {
            match value.kind() {
                K::Constructed(base, _) | K::Discriminant(base) => value = base,
                K::Observation { contents, .. } | K::Borrowed(contents) => value = contents,
                _ => return value,
            }
        }
    }

    pub(crate) fn matches_variant(self, variant: bray_symbols::UnionVariantSymbolId) -> Self {
        match self.kind() {
            K::Unknown => self,
            K::Borrowed(value) | K::Observation { contents: value, .. } => value.as_ref().clone().matches_variant(variant),
            K::Literal(value)
                if let bray_symbols::ConstantValueKind::Union {
                    variant: active, ..
                } = value.kind() =>
            {
                Self::boolean(*active == variant)
            }
            K::Tagged(active, _) => Self::boolean(*active == variant),
            K::Constructed(base, _) | K::Discriminant(base) => base.as_ref().clone().matches_variant(variant),
            _ => {
                if let Some(snapshot) = self.nullable_payload_snapshot() {
                    snapshot.clone().matches_variant(variant)
                } else {
                    Self::new(K::VariantTest(variant, Arc::new(self)))
                }
            }
        }
    }

}
