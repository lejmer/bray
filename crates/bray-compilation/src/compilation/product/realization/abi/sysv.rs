use std::collections::BTreeMap;
use std::num::NonZeroU16;

use bray_codegen::{
    CodegenAbiPiece, CodegenAbiScalar, CodegenAggregateCoercion, CodegenFieldLayout,
    CodegenParameterMapping, CodegenTarget, CodegenTypeKind, CodegenTypeMapping,
};
use bray_symbols::{CallableAbi, TypeId};
use bray_target::{ObjectFormat, TargetArchitecture};

use crate::compilation::CodegenPreparationError;

pub(super) fn applies(abi: CallableAbi, target: &CodegenTarget) -> bool {
    abi != CallableAbi::Bray
        && target.machine().architecture() == TargetArchitecture::X86_64
        && target.machine().object_format() != ObjectFormat::Coff
}

pub(super) fn is_aggregate(kind: &CodegenTypeKind) -> bool {
    matches!(
        kind,
        CodegenTypeKind::Aggregate(_)
            | CodegenTypeKind::Array { .. }
            | CodegenTypeKind::Union { .. }
    )
}

#[derive(Clone, Copy, Default)]
struct Eightbyte {
    integer: bool,
    occupied: u16,
    double: bool,
}

pub(super) fn coercion(
    ty: TypeId,
    mappings: &BTreeMap<TypeId, CodegenTypeMapping>,
) -> Result<Option<CodegenAggregateCoercion>, CodegenPreparationError> {
    let layout = mappings[&ty]
        .layout()
        .expect("ABI values have sized layouts");

    if layout.size() == 0 || layout.size() > 16 {
        return Ok(None);
    }

    let mut classes = [Eightbyte::default(); 2];

    if !classify(ty, 0, mappings, &mut classes)? {
        return Ok(None);
    }

    let pieces = classes
        .into_iter()
        .enumerate()
        .filter_map(|(index, class)| {
            let scalar = if class.occupied == 0 {
                return None;
            } else if class.integer {
                CodegenAbiScalar::Integer(
                    NonZeroU16::new(class.occupied * 8).expect("occupied integer class"),
                )
            } else if class.double {
                CodegenAbiScalar::Float64
            } else if class.occupied > 4 {
                CodegenAbiScalar::Float32Pair
            } else {
                CodegenAbiScalar::Float32
            };

            Some(CodegenAbiPiece {
                offset_bytes: index as u64 * 8,
                scalar,
            })
        })
        .collect::<Vec<_>>();

    Ok((!pieces.is_empty()).then(|| CodegenAggregateCoercion::new(pieces)))
}

fn classify(
    ty: TypeId,
    offset: u64,
    mappings: &BTreeMap<TypeId, CodegenTypeMapping>,
    classes: &mut [Eightbyte; 2],
) -> Result<bool, CodegenPreparationError> {
    let mapping = &mappings[&ty];
    let layout = mapping.layout().expect("ABI fields have sized layouts");

    if !offset.is_multiple_of(layout.alignment().get()) {
        return Ok(false);
    }

    match mapping.kind() {
        CodegenTypeKind::Aggregate(fields) => {
            if fields.is_empty() && layout.size() != 0 {
                // Size and alignment alone cannot distinguish INTEGER from SSE storage.
                return Err(CodegenPreparationError::UnsupportedType(ty));
            }

            classify_fields(fields, offset, mappings, classes)
        }
        CodegenTypeKind::Array { element, length } => {
            let stride = mappings[element]
                .layout()
                .expect("array element layout")
                .size();

            if stride != 0 {
                for index in 0..*length {
                    if !classify(*element, offset + index * stride, mappings, classes)? {
                        return Ok(false);
                    }
                }
            }

            Ok(true)
        }
        CodegenTypeKind::Union { tag, variants, .. } => {
            if let Some(tag) = tag
                && !classify(*tag, offset, mappings, classes)?
            {
                return Ok(false);
            }

            for variant in variants.iter() {
                if !classify_fields(variant.fields(), offset, mappings, classes)? {
                    return Ok(false);
                }
            }

            Ok(true)
        }
        CodegenTypeKind::Unit => Ok(true),
        CodegenTypeKind::Boolean
        | CodegenTypeKind::SignedInteger(_)
        | CodegenTypeKind::UnsignedInteger(_)
        | CodegenTypeKind::Pointer { .. }
        | CodegenTypeKind::Callable(_)
        | CodegenTypeKind::Float(_) => {
            let float = matches!(mapping.kind(), CodegenTypeKind::Float(_));

            if float && !matches!(layout.size(), 4 | 8) {
                return Err(CodegenPreparationError::UnsupportedType(ty));
            }

            for byte in offset..offset + layout.size() {
                let index = usize::try_from(byte / 8).expect("eightbyte index");

                let class = classes
                    .get_mut(index)
                    .expect("classified aggregate fits two eightbytes");

                class.integer |= !float;
                class.double |= float && layout.size() == 8;
                class.occupied = class.occupied.max((byte % 8 + 1) as u16);
            }

            Ok(true)
        }
        CodegenTypeKind::Opaque
        | CodegenTypeKind::UnsizedSlice { .. }
        | CodegenTypeKind::UnsizedTraitView => {
            panic!("foreign ABI classification requires a concrete C value: {ty:?}")
        }
    }
}

fn classify_fields(
    fields: &[CodegenFieldLayout],
    offset: u64,
    mappings: &BTreeMap<TypeId, CodegenTypeMapping>,
    classes: &mut [Eightbyte; 2],
) -> Result<bool, CodegenPreparationError> {
    for field in fields {
        if !classify(field.ty(), offset + field.offset_bytes(), mappings, classes)? {
            return Ok(false);
        }
    }

    Ok(true)
}

pub(super) struct Registers {
    integers: usize,
    vectors: usize,
}

impl Registers {
    pub(super) fn new(indirect_result: bool) -> Self {
        Self {
            integers: 6 - usize::from(indirect_result),
            vectors: 8,
        }
    }

    pub(super) fn consume(
        &mut self,
        parameter: &CodegenParameterMapping,
        mappings: &BTreeMap<TypeId, CodegenTypeMapping>,
    ) -> bool {
        let (integers, vectors) = match parameter {
            CodegenParameterMapping::Direct {
                coercion: Some(coercion),
                ..
            } => {
                let integers = coercion
                    .pieces()
                    .iter()
                    .filter(|piece| matches!(piece.scalar, CodegenAbiScalar::Integer(_)))
                    .count();

                (integers, coercion.pieces().len() - integers)
            }
            CodegenParameterMapping::Direct { ty, .. } => {
                let mapping = &mappings[ty];

                if matches!(mapping.kind(), CodegenTypeKind::Float(_)) {
                    (0, 1)
                } else {
                    (
                        mapping
                            .layout()
                            .expect("direct parameter layout")
                            .size()
                            .div_ceil(8) as usize,
                        0,
                    )
                }
            }
            CodegenParameterMapping::Ignore | CodegenParameterMapping::Indirect { .. } => {
                return true;
            }
        };

        if integers > self.integers || vectors > self.vectors {
            return false;
        }

        self.integers -= integers;
        self.vectors -= vectors;

        true
    }
}
