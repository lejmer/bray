use crate::{
    AnySymbolId, CallableParameterDefaultProviderSymbolId, StructFieldDefaultProviderSymbolId,
    SymbolGraph, UnionPayloadDefaultProviderSymbolId,
};

/// One declaration-owned runtime default used by a call or construction.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DefaultValueProvider {
    /// A struct field default.
    StructField(StructFieldDefaultProviderSymbolId),
    /// A union payload field default.
    UnionPayload(UnionPayloadDefaultProviderSymbolId),
    /// A callable parameter default, including type-form parameters.
    CallableParameter(CallableParameterDefaultProviderSymbolId),
}

impl DefaultValueProvider {
    /// Resolves a declaration symbol to its exact default-provider category.
    pub const fn try_from_symbol(symbol: AnySymbolId) -> Option<Self> {
        match symbol {
            AnySymbolId::CallableParameterDefaultProvider(provider) => {
                Some(Self::CallableParameter(provider))
            }
            AnySymbolId::StructFieldDefaultProvider(provider) => Some(Self::StructField(provider)),
            AnySymbolId::UnionPayloadDefaultProvider(provider) => {
                Some(Self::UnionPayload(provider))
            }
            _ => None,
        }
    }

    /// Returns the declaration symbol that owns this runtime default.
    pub const fn symbol(self) -> AnySymbolId {
        match self {
            Self::StructField(symbol) => AnySymbolId::StructFieldDefaultProvider(symbol),
            Self::UnionPayload(symbol) => AnySymbolId::UnionPayloadDefaultProvider(symbol),
            Self::CallableParameter(symbol) => {
                AnySymbolId::CallableParameterDefaultProvider(symbol)
            }
        }
    }
}

impl SymbolGraph {
    /// Returns the parameter or field evaluated by one runtime-default provider.
    pub fn runtime_default_subject(&self, provider: AnySymbolId) -> Option<AnySymbolId> {
        match provider {
            AnySymbolId::CallableParameterDefaultProvider(id) => self
                .callable_parameter_default_provider(id)
                .map(|provider| provider.subject().into()),
            AnySymbolId::StructFieldDefaultProvider(id) => self
                .struct_field_default_provider(id)
                .map(|provider| provider.subject().into()),
            AnySymbolId::UnionPayloadDefaultProvider(id) => self
                .union_payload_default_provider(id)
                .map(|provider| provider.subject().into()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{PackageIdentity, SymbolGraph, SymbolOrigin};

    #[test]
    fn runtime_default_subject_recovers_the_exact_defaulted_declaration() {
        let (table, syntax) = crate::test_support::declarations_and_syntax(&[concat!(
            "module app;\n",
            "func main(value: i32 = 1)\n",
            "{\n",
            "}\n",
        )]);

        let Some(package) = PackageIdentity::try_new("test.package") else {
            panic!("test package identity must be valid");
        };

        let graph = match SymbolGraph::build_source(package, &table, &syntax) {
            Ok(graph) => graph,
            Err(error) => panic!("test symbol graph must build: {error:?}"),
        };

        let Some(parameter) = graph
            .callable_parameters()
            .iter()
            .find(|parameter| parameter.origin() == SymbolOrigin::Source)
        else {
            panic!("test source must declare one callable parameter");
        };

        let Some(provider) = graph.runtime_default_provider(parameter.id().into()) else {
            panic!("defaulted parameter must have a runtime-default provider");
        };

        assert_eq!(
            graph.runtime_default_subject(provider),
            Some(parameter.id().into())
        );
    }
}
