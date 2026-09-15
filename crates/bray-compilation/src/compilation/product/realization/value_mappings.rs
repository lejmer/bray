use std::collections::BTreeMap;

use bray_codegen::{
    CodegenCallableMapping, CodegenConstantMapping, CodegenConstantTermMapping, CodegenTarget,
    CodegenTerminatorMapping, CodegenUnit, child_constants, demanded_callable_instances,
    demanded_constant_terms, demanded_constants,
};
use bray_symbols::ConstantTermData;

use super::super::super::{CodegenPreparationError, Compilation};
use super::super::specialization::{ConcreteCodegenCallee, ConcreteCodegenReachability};
use crate::compilation::{ProductDataKind, ProductQueryContext, ProductQueryFailure};
use crate::fact::CancellationToken;

impl Compilation {
    pub(super) fn codegen_callables(
        &self,
        unit: &CodegenUnit,
        target: &CodegenTarget,
        reachability: &ConcreteCodegenReachability,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenCallableMapping>, CodegenPreparationError> {
        let mut mappings = Vec::new();

        for (owner, demand) in demanded_callable_instances(unit) {
            let owner_realization = reachability.instance(&owner).ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::Instance(owner.clone()),
                    ProductDataKind::ConcreteInstance,
                )
            })?;

            let mapping = match self.concrete_codegen_callee(
                owner_realization,
                &demand,
                target,
                cancellation,
            )? {
                ConcreteCodegenCallee::Instance(instance) => CodegenCallableMapping::new(
                    owner.clone(),
                    demand.site(),
                    demand.reference(),
                    instance.key().clone(),
                ),
                ConcreteCodegenCallee::Intrinsic(intrinsic) => CodegenCallableMapping::intrinsic(
                    owner.clone(),
                    demand.site(),
                    demand.reference(),
                    intrinsic,
                ),
            };

            mappings.push(mapping);
        }

        Ok(mappings)
    }

    pub(super) fn codegen_constants(
        &self,
        unit: &CodegenUnit,
        additional: impl IntoIterator<Item = bray_symbols::ConstantValueId>,
    ) -> Result<Vec<CodegenConstantMapping>, CodegenPreparationError> {
        let values = self.semantic_value_store()?;
        let demands = demanded_constants(unit);
        let mut pending = Vec::new();
        let mut mapped = BTreeMap::new();

        for value in demands.values() {
            match demands.types().get(value) {
                Some(types) if !types.is_empty() => {
                    pending.extend(types.iter().map(|ty| (*value, Some(*ty))));
                }
                Some(_) | None => pending.push((*value, None)),
            }
        }

        pending.extend(additional.into_iter().map(|value| (value, None)));

        while let Some((value, representation)) = pending.pop() {
            let data = values.constant_value_data(value);

            let representation = representation.unwrap_or_else(|| data.ty());
            let key = (value, representation);

            if mapped.contains_key(&key) {
                continue;
            }

            pending.extend(child_constants(data.kind()).map(|child| (child, None)));

            // Code generation mappings outlive this shared semantic-store read and therefore
            // take independent ownership of the immutable payload at this boundary.
            mapped.insert(
                key,
                CodegenConstantMapping::with_representation(
                    value,
                    data.as_ref().clone(),
                    representation,
                ),
            );
        }

        Ok(mapped.into_values().collect())
    }

    pub(super) fn codegen_constant_terms(
        &self,
        unit: &CodegenUnit,
        reachability: &ConcreteCodegenReachability,
    ) -> Result<
        (
            Vec<CodegenConstantTermMapping>,
            Vec<CodegenTerminatorMapping>,
        ),
        CodegenPreparationError,
    > {
        let values = self.semantic_value_store()?;
        let mut terms = Vec::new();
        let mut resolved = BTreeMap::new();

        for instance in unit.instances() {
            let realization = reachability.instance(instance.key()).ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::Instance(instance.key().clone()),
                    ProductDataKind::ConcreteInstance,
                )
            })?;

            for template_term in demanded_constant_terms(instance.mir()) {
                let term = self
                    .substitute_codegen_constant_term(template_term, realization.substitution())?;

                let data = values.constant_term_data(term);

                let ConstantTermData::Value(value) = data.as_ref() else {
                    return Err(CodegenPreparationError::OpenConstantTerm(term));
                };

                resolved.insert((instance.key().clone(), template_term), *value);

                terms.push(CodegenConstantTermMapping::new(
                    instance.key().clone(),
                    template_term,
                    *value,
                ));
            }
        }

        let mut terminators = Vec::new();

        for instance in unit.instances() {
            for (block, data) in instance.mir().blocks_with_ids() {
                if let Some(value) = data.terminator().kind().pattern_literal_value() {
                    terminators.push(CodegenTerminatorMapping::new(
                        instance.key().clone(),
                        block,
                        [value],
                    ));

                    continue;
                }

                let Some(term) = data.terminator().kind().pattern_constant_term() else {
                    continue;
                };

                let Some(value) = resolved.get(&(instance.key().clone(), term)) else {
                    return Err(CodegenPreparationError::OpenConstantTerm(term));
                };

                terminators.push(CodegenTerminatorMapping::new(
                    instance.key().clone(),
                    block,
                    [*value],
                ));
            }
        }

        Ok((terms, terminators))
    }
}
