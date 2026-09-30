use super::encoding::OrderKey;
use bray_binder::BindingQueryContext;
use bray_symbols::{
    AnySymbolId, ExternalDeclarationIdentity, ExternalSymbolKey, ExternalSymbolKeyData, SymbolKey,
    SymbolKeyData,
};

use super::encoding::{sequence, term};
use super::values::OrderEncoder;
use crate::compilation::{CodegenPreparationError, binder::binding_query_error};

impl OrderEncoder<'_, '_> {
    pub(super) fn symbol(&self, symbol: AnySymbolId) -> Result<OrderKey, CodegenPreparationError> {
        let key = self
            .compilation
            .portable_codegen_symbol_key(self.context, symbol)?;

        if matches!(key.data(), SymbolKeyData::CompilerKnownDeclaration { .. }) {
            let graph = self.context.symbols();
            let module = graph.containing_module(symbol);
            let mut names = Vec::new();
            let mut current = symbol;

            while !matches!(
                current,
                AnySymbolId::Module(_) | AnySymbolId::CompilerKnownEnvironment(_)
            ) {
                let name = self
                    .context
                    .member_name(current)
                    .map_err(binding_query_error)?
                    .expect("compiler-known declaration must have a name");

                names.push(sequence([
                    name.as_str().as_bytes().to_vec().into(),
                    current.kind().as_str().as_bytes().to_vec().into(),
                ]));

                current = self
                    .context
                    .containing_symbol(current)
                    .map_err(binding_query_error)?
                    .expect("compiler-known declaration must have an owner");
            }

            names.reverse();

            return Ok(sequence([
                b"std".to_vec().into(),
                sequence(
                    module
                        .into_iter()
                        .flat_map(|module| module.path().segments())
                        .map(|part| part.as_bytes().to_vec().into()),
                ),
                sequence(names),
            ]));
        }

        self.symbol_key(&key)
    }

    pub(super) fn symbol_key(&self, key: &SymbolKey) -> Result<OrderKey, CodegenPreparationError> {
        match key.data() {
            SymbolKeyData::External(key) => Ok(external(key)),
            _ => {
                let symbol = self
                    .context
                    .symbols()
                    .symbol_for_key(key)
                    .expect("source structural identity must have a semantic declaration");

                self.symbol(symbol)
            }
        }
    }
}

fn external(key: &ExternalSymbolKey) -> OrderKey {
    let mut current = key;
    let mut declarations = Vec::new();
    let mut module = sequence([]);

    loop {
        match current.data() {
            ExternalSymbolKeyData::Package(package) => {
                declarations.reverse();

                return sequence([
                    package.as_str().as_bytes().to_vec().into(),
                    module,
                    sequence(declarations),
                ]);
            }
            ExternalSymbolKeyData::Module { package, path } => {
                module = sequence(path.segments().map(|part| part.as_bytes().to_vec().into()));
                current = package;
            }
            ExternalSymbolKeyData::Declaration {
                owner,
                kind,
                identity,
            } => {
                let name = match identity {
                    ExternalDeclarationIdentity::Name(name) => {
                        name.as_str().as_bytes().to_vec().into()
                    }
                    ExternalDeclarationIdentity::Ordinal(ordinal) => {
                        term("ordinal", [ordinal.raw().to_be_bytes().to_vec().into()])
                    }
                };

                declarations.push(sequence([name, kind.as_str().as_bytes().to_vec().into()]));
                current = owner;
            }
            ExternalSymbolKeyData::Synthesized {
                owner,
                role,
                ordinal,
            } => {
                declarations.push(sequence([
                    term(
                        "synthesized",
                        [role.kind().as_str().as_bytes().to_vec().into()],
                    ),
                    ordinal
                        .map(|value| value.raw().to_be_bytes().to_vec().into())
                        .unwrap_or_else(|| Vec::new().into()),
                ]));

                current = owner;
            }
        }
    }
}
