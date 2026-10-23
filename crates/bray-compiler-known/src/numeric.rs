use crate::RepresentationRole;

/// Numeric widths and precision used by compiler-defined scalar conversions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScalarShape {
    /// Signed integer width.
    Signed(u16),
    /// Unsigned integer width.
    Unsigned(u16),
    /// Real significand precision.
    Real(u16),
    /// Complex component significand precision.
    Complex(u16),
}

impl ScalarShape {
    /// Whether every source value converts to the target without information loss.
    pub const fn converts_losslessly_to(self, target: Self) -> bool {
        match (self, target) {
            (Self::Signed(source), Self::Signed(target)) => source <= target,
            (Self::Unsigned(source), Self::Unsigned(target)) => source <= target,
            (Self::Unsigned(source), Self::Signed(target)) => source < target,
            (Self::Signed(source), Self::Real(mantissa)) => source - 1 <= mantissa,
            (Self::Unsigned(source), Self::Real(mantissa)) => source <= mantissa,
            (Self::Real(source), Self::Real(target)) => source <= target,
            (Self::Complex(source), Self::Complex(target)) => source <= target,
            _ => false,
        }
    }

    /// Resolves a scalar's shape using the selected target's pointer width.
    pub const fn for_role(role: RepresentationRole, target_width: u16) -> Option<Self> {
        if let Some(integer) = role.integer_representation() {
            return Some(match integer {
                crate::IntegerRepresentation::Signed(width) => Self::Signed(width),
                crate::IntegerRepresentation::Unsigned(width) => Self::Unsigned(width),
                crate::IntegerRepresentation::TargetSigned => Self::Signed(target_width),
                crate::IntegerRepresentation::TargetUnsigned => Self::Unsigned(target_width),
            });
        }

        if let Some((_, precision)) = role.real_format() {
            return Some(Self::Real(precision));
        }

        let Some(component) = role.complex_component() else {
            return None;
        };

        let Some((_, precision)) = component.real_format() else {
            panic!("complex components use real scalar representations");
        };

        Some(Self::Complex(precision))
    }
}
