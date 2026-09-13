use std::hash::Hasher;

use bray_symbols::{
    CallableAbi, CallableConstness, CallableParameterMode, CallablePosition, CallableTrust,
    ConstantBinaryOperation, ConstantUnaryOperation, CurrentRunCancellation,
    DependencyRequirementKind, IntegerConstant, IntegerSign, LifecycleObligationKind,
    RealConstantBits, SymbolOrdinal, TargetSizedIntegerType,
};

use super::encoding::StructuralValueEncoder;

impl StructuralValueEncoder<'_, '_> {
    pub(super) fn integer(&mut self, value: &IntegerConstant) {
        self.integer_sign(value.sign());
        self.bytes(value.magnitude());
    }

    pub(super) fn real(&mut self, value: RealConstantBits) {
        match value {
            RealConstantBits::Binary16(bits) => {
                self.tag(0);
                self.digest.write_u16(bits);
            }
            RealConstantBits::Binary32(bits) => {
                self.tag(1);
                self.digest.write_u32(bits);
            }
            RealConstantBits::Binary64(bits) => {
                self.tag(2);
                self.digest.write_u64(bits);
            }
            RealConstantBits::Binary128(bits) => {
                self.tag(3);
                self.digest.write(&bits);
            }
        }
    }

    pub(super) fn dependency_requirement_kind(&mut self, kind: DependencyRequirementKind) {
        match kind {
            DependencyRequirementKind::StorageAlive => self.tag(0),
            DependencyRequirementKind::ValueDependencies => self.tag(6),
            DependencyRequirementKind::StorageInitialized => self.tag(1),
            DependencyRequirementKind::BorrowCapabilityActive(kind) => {
                self.tag(2);

                self.tag(match kind {
                    bray_symbols::BorrowKind::Shared => 0,
                    bray_symbols::BorrowKind::Mutable => 1,
                });
            }
            DependencyRequirementKind::ExclusiveMutationAuthority => {
                self.tag(3);
            }
            DependencyRequirementKind::ScopedCapabilityLive => self.tag(4),
            DependencyRequirementKind::LifecycleObligation(obligation) => {
                self.tag(5);
                self.lifecycle_obligation(obligation);
            }
        }
    }

    pub(super) fn constant_binary_operation(&mut self, operation: ConstantBinaryOperation) {
        self.tag(match operation {
            ConstantBinaryOperation::Add => 0,
            ConstantBinaryOperation::Subtract => 1,
            ConstantBinaryOperation::Multiply => 2,
            ConstantBinaryOperation::Divide => 3,
            ConstantBinaryOperation::Remainder => 4,
            ConstantBinaryOperation::Exponentiate => 5,
            ConstantBinaryOperation::LogicalAnd => 6,
            ConstantBinaryOperation::LogicalOr => 7,
            ConstantBinaryOperation::BitwiseAnd => 8,
            ConstantBinaryOperation::BitwiseOr => 9,
            ConstantBinaryOperation::BitwiseXor => 10,
            ConstantBinaryOperation::ShiftLeft => 11,
            ConstantBinaryOperation::ShiftRight => 12,
            ConstantBinaryOperation::Equal => 13,
            ConstantBinaryOperation::NotEqual => 14,
            ConstantBinaryOperation::Less => 15,
            ConstantBinaryOperation::LessOrEqual => 16,
            ConstantBinaryOperation::Greater => 17,
            ConstantBinaryOperation::GreaterOrEqual => 18,
        });
    }

    pub(super) fn constant_unary_operation(&mut self, operation: ConstantUnaryOperation) {
        self.tag(match operation {
            ConstantUnaryOperation::Identity => 0,
            ConstantUnaryOperation::Negate => 1,
            ConstantUnaryOperation::LogicalNot => 2,
            ConstantUnaryOperation::BitwiseNot => 3,
        });
    }

    pub(super) fn lifecycle_obligation(&mut self, obligation: LifecycleObligationKind) {
        self.tag(match obligation {
            LifecycleObligationKind::Destruction => 0,
            LifecycleObligationKind::Finalization => 1,
            LifecycleObligationKind::Cancellation => 2,
            LifecycleObligationKind::Joining => 3,
        });
    }

    pub(super) fn target_sized_integer_type(&mut self, ty: TargetSizedIntegerType) {
        self.tag(match ty {
            TargetSizedIntegerType::Isize => 0,
            TargetSizedIntegerType::Usize => 1,
        });
    }

    fn integer_sign(&mut self, sign: IntegerSign) {
        self.tag(match sign {
            IntegerSign::NonNegative => 0,
            IntegerSign::Negative => 1,
        });
    }

    pub(super) fn callable_position(&mut self, position: CallablePosition) {
        self.tag(match position {
            CallablePosition::NamedOnly => 0,
            CallablePosition::PositionalOrNamed => 1,
        });
    }

    pub(super) fn callable_parameter_mode(&mut self, mode: CallableParameterMode) {
        self.tag(match mode {
            CallableParameterMode::Immutable => 0,
            CallableParameterMode::Mutable => 1,
        });
    }

    pub(super) fn callable_constness(&mut self, constness: CallableConstness) {
        self.tag(match constness {
            CallableConstness::Runtime => 0,
            CallableConstness::Constant => 1,
        });
    }

    pub(super) fn callable_trust(&mut self, trust: CallableTrust) {
        self.tag(match trust {
            CallableTrust::Safe => 0,
            CallableTrust::Trusted => 1,
        });
    }

    pub(super) fn callable_abi(&mut self, abi: CallableAbi) {
        self.tag(match abi {
            CallableAbi::Bray => 0,
            CallableAbi::C => 1,
            CallableAbi::System => 2,
        });
    }

    pub(super) fn current_run_cancellation(&mut self, value: CurrentRunCancellation) {
        self.tag(match value {
            CurrentRunCancellation::NotEntered => 0,
            CurrentRunCancellation::MayEnter => 1,
        });
    }

    pub(super) fn ordinal(&mut self, ordinal: SymbolOrdinal) {
        self.digest.write_u32(ordinal.raw());
    }

    pub(super) fn boolean(&mut self, value: bool) {
        self.tag(u8::from(value));
    }

    pub(super) fn length(&mut self, value: usize) {
        self.digest.write_usize(value);
    }

    pub(super) fn bytes(&mut self, value: &[u8]) {
        self.length(value.len());
        self.digest.write(value);
    }

    pub(super) fn tag(&mut self, value: u8) {
        self.digest.write_u8(value);
    }
}
