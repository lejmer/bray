use bray_compiler_known::RepresentationRole;

use super::check::AbiField;

pub(super) const DATE_TIME_FIELDS: &[AbiField] = &[
    AbiField::Scalar(RepresentationRole::ScalarI32),
    AbiField::Scalar(RepresentationRole::ScalarU32),
    AbiField::Scalar(RepresentationRole::ScalarU32),
    AbiField::Scalar(RepresentationRole::ScalarU32),
    AbiField::Scalar(RepresentationRole::ScalarU32),
    AbiField::Scalar(RepresentationRole::ScalarU32),
    AbiField::Scalar(RepresentationRole::ScalarU32),
];

pub(super) const OBSERVATION_FIELDS: &[AbiField] = &[
    AbiField::Struct(DATE_TIME_FIELDS),
    AbiField::Scalar(RepresentationRole::ScalarI32),
    AbiField::Scalar(RepresentationRole::ScalarU32),
];

pub(super) const RESOLUTION_FIELDS: &[AbiField] = &[
    AbiField::Scalar(RepresentationRole::ScalarU32),
    AbiField::Scalar(RepresentationRole::ScalarU32),
    AbiField::Scalar(RepresentationRole::ScalarI64),
    AbiField::Scalar(RepresentationRole::ScalarU32),
    AbiField::Scalar(RepresentationRole::ScalarU32),
    AbiField::Scalar(RepresentationRole::ScalarI64),
    AbiField::Scalar(RepresentationRole::ScalarU32),
    AbiField::Scalar(RepresentationRole::ScalarU32),
];

pub(super) const VALUE_FIELDS: &[AbiField] = &[
    AbiField::Struct(DATE_TIME_FIELDS),
    AbiField::Scalar(RepresentationRole::ScalarI64),
    AbiField::Scalar(RepresentationRole::ScalarI32),
    AbiField::Scalar(RepresentationRole::ScalarU32),
];
