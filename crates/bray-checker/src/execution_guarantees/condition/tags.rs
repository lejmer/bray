use std::sync::Arc;

use super::ExecutionCondition;

impl ExecutionCondition {
    /// Select only the retained tag from the actual current snapshot.
    pub(crate) fn discriminant(&self) -> Self {
        match self {
            Self::Observation { contents, .. } | Self::Borrowed(contents)
            | Self::Constructed(contents, _) => contents.discriminant(),
            Self::Present(_) => Self::Present(Arc::new(Self::Unknown)),
            Self::Tagged(variant, _) => Self::Tagged(*variant, Arc::new(Self::Unknown)),
            Self::Literal(value) => match value.kind() {
                bray_symbols::ConstantValueKind::NullableAbsent => self.clone(),
                bray_symbols::ConstantValueKind::Union { variant, .. } =>
                    Self::Tagged(*variant, Arc::new(Self::Unknown)),
                _ => panic!("discriminant requires a nullable or union value: {self:?}"),
            },
            Self::Discriminant(_) | Self::Unknown => self.clone(),
            value => Self::Discriminant(Arc::new(value.clone())),
        }
    }

    pub(crate) fn known_presence(&self) -> Option<bool> {
        match self {
            Self::Observation { contents, .. } | Self::Borrowed(contents)
            | Self::Constructed(contents, _) | Self::Discriminant(contents) => contents.known_presence(),
            Self::Present(_) => Some(true),
            Self::Literal(value) if matches!(value.kind(), bray_symbols::ConstantValueKind::NullableAbsent) => Some(false),
            _ => None,
        }
    }

    pub(crate) fn known_variant(&self) -> Option<bray_symbols::UnionVariantSymbolId> {
        let mut value = self;

        loop {
            match value {
                Self::Tagged(variant, _) => return Some(*variant),
                Self::Constructed(base, _) | Self::Discriminant(base) => value = base,
                Self::Observation { contents, .. } | Self::Borrowed(contents) => value = contents,
                Self::Literal(literal) => return match literal.kind() {
                    bray_symbols::ConstantValueKind::Union { variant, .. } => Some(*variant),
                    _ => None,
                },
                _ => return None,
            }
        }
    }

    pub(crate) fn matches_variant(self, variant: bray_symbols::UnionVariantSymbolId) -> Self {
        match self {
            Self::Unknown => Self::Unknown,
            Self::Borrowed(value) | Self::Observation { contents: value, .. } => value.as_ref().clone().matches_variant(variant),
            Self::Literal(ref value)
                if let bray_symbols::ConstantValueKind::Union {
                    variant: active, ..
                } = value.kind() =>
            {
                Self::Boolean(*active == variant)
            }
            Self::Tagged(active, _) => Self::Boolean(active == variant),
            Self::Constructed(base, _) | Self::Discriminant(base) => base.as_ref().clone().matches_variant(variant),
            value => {
                if let Some(snapshot) = value.nullable_payload_snapshot() {
                    snapshot.clone().matches_variant(variant)
                } else {
                    Self::VariantTest(variant, Arc::new(value))
                }
            }
        }
    }

}
