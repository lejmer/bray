use std::hash::Hash;

use bray_base::StableDigestHasher;
use bray_symbols::{
    AnySymbolId, ConstantTermId, ConstantValueId, DependencyContractTemplateId,
    GenericSubstitutionId, ImplementationInstanceId, SemanticValueStore,
    StaticReferenceSelection, TraitApplicationId, TypeId,
};

use super::encoding::StructuralValueEncoder;
use crate::compilation::binder::CompilationBindingContext;
use crate::fact::FactQueryError;

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::compilation::product) enum StructuralSemanticValue {
    Symbol(AnySymbolId),
    Type(TypeId),
    ConstantValue(ConstantValueId),
    ConstantTerm(ConstantTermId),
    Substitution(GenericSubstitutionId),
    TraitApplication(TraitApplicationId),
    Implementation(ImplementationInstanceId),
    DependencyContract(DependencyContractTemplateId),
}

pub(in crate::compilation::product) fn structural_semantic_identity(
    values: &SemanticValueStore,
    binding_context: &CompilationBindingContext<'_>,
    value: StructuralSemanticValue,
) -> Result<[u8; 32], FactQueryError> {
    let mut encoder = StructuralValueEncoder {
        values,
        binding_context,
        digest: StableDigestHasher::new(),
        allow_static_address: true,
    };

    encoder.bytes(b"bray.codegen-mir-semantic.v1");

    match value {
        StructuralSemanticValue::Symbol(id) => {
            encoder.tag(7);
            encoder.symbol(id)?;
        }
        StructuralSemanticValue::Type(id) => {
            encoder.tag(0);
            encoder.ty(id)?;
        }
        StructuralSemanticValue::ConstantValue(id) => {
            encoder.tag(1);
            encoder.constant_value(id)?;
        }
        StructuralSemanticValue::ConstantTerm(id) => {
            encoder.tag(2);
            encoder.constant_term(id)?;
        }
        StructuralSemanticValue::Substitution(id) => {
            encoder.tag(3);
            encoder.substitution(id)?;
        }
        StructuralSemanticValue::TraitApplication(id) => {
            encoder.tag(4);
            encoder.trait_application(id)?;
        }
        StructuralSemanticValue::Implementation(id) => {
            encoder.tag(5);
            encoder.implementation_instance(id)?;
        }
        StructuralSemanticValue::DependencyContract(id) => {
            encoder.tag(6);
            encoder.dependency_contract(id)?;
        }
    }

    Ok(encoder.digest.finalize())
}

pub(super) fn static_reference(encoder: &mut StructuralValueEncoder<'_, '_>, reference: &StaticReferenceSelection) -> Result<(), FactQueryError> {
    encoder.symbol(reference.template().declaration().into())?;
    encoder.substitution(reference.substitution())?;
    reference.target().hash(&mut encoder.digest);

    let witnesses = match reference {
        StaticReferenceSelection::Open {
            selected_witnesses,
            ..
        } => selected_witnesses.as_ref(),
        StaticReferenceSelection::Closed(instance) => instance.selected_witnesses(),
    };

    encoder.length(witnesses.len());

    for witness in witnesses {
        encoder.implementation_instance(*witness)?;
    }

    Ok(())
}
