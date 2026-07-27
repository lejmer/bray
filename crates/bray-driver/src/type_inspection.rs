use std::collections::BTreeSet;

use bray_symbols::{
    AnySymbolId, BorrowKind, CallableParameterMode, GenericArgument, GenericArgumentTemplate,
    SemanticValueStore, SymbolGraph, TraitApplicationId, TraitApplicationTemplate, TypeData,
    TypeExpressionTemplate, TypeId,
};
use serde::Serialize;

use crate::inspection::InspectionSymbolIdentity;

const MAXIMUM_TYPE_DEPTH: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TypeInspectionError {
    Depth,
    SemanticValue,
}

#[derive(Serialize)]
pub(crate) struct InspectionType {
    type_kind: &'static str,
    text: String,
    symbol_references: Vec<InspectionSymbolIdentity>,
}

impl InspectionType {
    pub(crate) fn from_template(
        semantic_values: &SemanticValueStore,
        symbols: &SymbolGraph,
        template: &TypeExpressionTemplate,
    ) -> Result<Self, TypeInspectionError> {
        let mut formatter = TypeFormatter::new(semantic_values, symbols);
        let type_kind = template_kind(template);
        let text = formatter.template(template, 0)?;

        Ok(Self {
            type_kind,
            text,
            symbol_references: formatter.references(),
        })
    }

    pub(crate) fn from_type(
        semantic_values: &SemanticValueStore,
        symbols: &SymbolGraph,
        ty: TypeId,
    ) -> Result<Self, TypeInspectionError> {
        let mut formatter = TypeFormatter::new(semantic_values, symbols);

        let data = semantic_values
            .type_data(ty)
            .map_err(|_| TypeInspectionError::SemanticValue)?;

        let type_kind = type_data_kind(data.as_ref());
        let text = formatter.ty(ty, 0)?;

        Ok(Self {
            type_kind,
            text,
            symbol_references: formatter.references(),
        })
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }
}

struct TypeFormatter<'model> {
    semantic_values: &'model SemanticValueStore,
    symbols: &'model SymbolGraph,
    referenced_symbols: BTreeSet<AnySymbolId>,
}

impl<'model> TypeFormatter<'model> {
    fn new(semantic_values: &'model SemanticValueStore, symbols: &'model SymbolGraph) -> Self {
        Self {
            semantic_values,
            symbols,
            referenced_symbols: BTreeSet::new(),
        }
    }

    fn references(&self) -> Vec<InspectionSymbolIdentity> {
        self.referenced_symbols
            .iter()
            .copied()
            .map(|symbol| InspectionSymbolIdentity::from_symbol(self.symbols, symbol))
            .collect()
    }

    fn template(
        &mut self,
        template: &TypeExpressionTemplate,
        depth: usize,
    ) -> Result<String, TypeInspectionError> {
        check_depth(depth)?;

        match template {
            TypeExpressionTemplate::Resolved(ty) => self.ty(*ty, depth + 1),
            TypeExpressionTemplate::Named {
                definition,
                arguments,
                ..
            } => {
                let definition = definition.into_any();
                let name = self.symbol(definition);
                let arguments = self.template_arguments(arguments, depth + 1)?;

                Ok(format_application(name, arguments))
            }
            TypeExpressionTemplate::TypeValuedMemberProjection {
                subject,
                application,
                member,
            } => {
                let subject = self.template(subject, depth + 1)?;
                let application = self.trait_template(application, depth + 1)?;
                let member = self.symbol((*member).into());

                Ok(format!("{subject}.<{application}>::{member}"))
            }
            TypeExpressionTemplate::Tuple(elements) => {
                let elements = elements
                    .iter()
                    .map(|element| self.template(element, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(format!("({})", elements.join(", ")))
            }
            TypeExpressionTemplate::Array { element, .. } => {
                let element = self.template(element, depth + 1)?;

                Ok(format!("[{element}; <const>]"))
            }
            TypeExpressionTemplate::Slice(element) => {
                let element = self.template(element, depth + 1)?;

                Ok(format!("[{element}]"))
            }
            TypeExpressionTemplate::Nullable(target) => {
                let target = self.template(target, depth + 1)?;

                Ok(format!("{target}?"))
            }
            TypeExpressionTemplate::Borrow { kind, target } => {
                let target = self.template(target, depth + 1)?;

                Ok(format_borrow(*kind, target))
            }
            TypeExpressionTemplate::TraitView(application) => {
                let application = self.trait_template(application, depth + 1)?;

                Ok(format!("trait {application}"))
            }
            TypeExpressionTemplate::OwnedIndirection { storage, target } => {
                let storage = self.template(storage, depth + 1)?;
                let target = self.template(target, depth + 1)?;

                Ok(format!("owned[{storage}] {target}"))
            }
            TypeExpressionTemplate::Callable(callable) => {
                let parameters = callable
                    .parameters()
                    .iter()
                    .map(|parameter| {
                        let ty = self.template(parameter.ty(), depth + 1)?;

                        let mode = match parameter.mode() {
                            CallableParameterMode::Immutable => "",
                            CallableParameterMode::Mutable => "mut ",
                        };

                        Ok(format!("{mode}{}: {ty}", parameter.name().as_str()))
                    })
                    .collect::<Result<Vec<_>, TypeInspectionError>>()?;

                let result = self.template(callable.result(), depth + 1)?;

                Ok(format!("func({}) -> {result}", parameters.join(", ")))
            }
        }
    }

    fn ty(&mut self, ty: TypeId, depth: usize) -> Result<String, TypeInspectionError> {
        check_depth(depth)?;

        let data = self
            .semantic_values
            .type_data(ty)
            .map_err(|_| TypeInspectionError::SemanticValue)?;

        match data.as_ref() {
            TypeData::Error => Ok(String::from("<error>")),
            TypeData::Named {
                definition,
                substitution,
            } => {
                let name = self.symbol(definition.into_any());

                let substitution = self
                    .semantic_values
                    .generic_substitution_data(*substitution)
                    .map_err(|_| TypeInspectionError::SemanticValue)?;

                let arguments = substitution
                    .bindings()
                    .iter()
                    .map(|binding| self.argument(binding.argument(), depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(format_application(name, arguments))
            }
            TypeData::TypeParameter(parameter) => Ok(self.symbol((*parameter).into())),
            TypeData::ContextualSelf(context) => {
                self.referenced_symbols.insert(context.symbol());

                Ok(String::from("Self"))
            }
            TypeData::TypeValuedMemberProjection {
                subject,
                application,
                member,
            } => {
                let subject = self.ty(*subject, depth + 1)?;
                let application = self.trait_application(*application, depth + 1)?;
                let member = self.symbol((*member).into());

                Ok(format!("{subject}.<{application}>::{member}"))
            }
            TypeData::Tuple(elements) => {
                let elements = elements
                    .iter()
                    .map(|element| self.ty(*element, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(format!("({})", elements.join(", ")))
            }
            TypeData::Array { element, .. } => {
                let element = self.ty(*element, depth + 1)?;

                Ok(format!("[{element}; <const>]"))
            }
            TypeData::Slice(element) => {
                let element = self.ty(*element, depth + 1)?;

                Ok(format!("[{element}]"))
            }
            TypeData::Generator(element) => {
                let element = self.ty(*element, depth + 1)?;

                Ok(format!("generator {element}"))
            }
            TypeData::Nullable(target) => {
                let target = self.ty(*target, depth + 1)?;

                Ok(format!("{target}?"))
            }
            TypeData::Borrow { kind, target } => {
                let target = self.ty(*target, depth + 1)?;

                Ok(format_borrow(*kind, target))
            }
            TypeData::TraitView(application) => {
                let application = self.trait_application(*application, depth + 1)?;

                Ok(format!("trait {application}"))
            }
            TypeData::OwnedIndirection { storage, target } => {
                let storage = self.ty(*storage, depth + 1)?;
                let target = self.ty(*target, depth + 1)?;

                Ok(format!("owned[{storage}] {target}"))
            }
            TypeData::Callable(callable) => {
                let parameters = callable
                    .parameters()
                    .iter()
                    .map(|parameter| {
                        let ty = self.ty(parameter.ty(), depth + 1)?;

                        let mode = match parameter.mode() {
                            CallableParameterMode::Immutable => "",
                            CallableParameterMode::Mutable => "mut ",
                        };

                        Ok(format!("{mode}{}: {ty}", parameter.name().as_str()))
                    })
                    .collect::<Result<Vec<_>, TypeInspectionError>>()?;

                let result = self.ty(callable.result(), depth + 1)?;

                Ok(format!("func({}) -> {result}", parameters.join(", ")))
            }
        }
    }

    fn template_arguments(
        &mut self,
        arguments: &[GenericArgumentTemplate],
        depth: usize,
    ) -> Result<Vec<String>, TypeInspectionError> {
        arguments
            .iter()
            .map(|argument| match argument {
                GenericArgumentTemplate::Type(ty) => self.template(ty, depth + 1),
                GenericArgumentTemplate::Constant(_) => Ok(String::from("<const>")),
            })
            .collect()
    }

    fn argument(
        &mut self,
        argument: GenericArgument,
        depth: usize,
    ) -> Result<String, TypeInspectionError> {
        match argument {
            GenericArgument::Type(ty) => self.ty(ty, depth + 1),
            GenericArgument::Constant(_) => Ok(String::from("<const>")),
        }
    }

    fn trait_template(
        &mut self,
        application: &TraitApplicationTemplate,
        depth: usize,
    ) -> Result<String, TypeInspectionError> {
        let name = self.symbol(application.definition().into());
        let arguments = self.template_arguments(application.arguments(), depth + 1)?;

        Ok(format_application(name, arguments))
    }

    fn trait_application(
        &mut self,
        application: TraitApplicationId,
        depth: usize,
    ) -> Result<String, TypeInspectionError> {
        let application = self
            .semantic_values
            .trait_application_data(application)
            .map_err(|_| TypeInspectionError::SemanticValue)?;

        let name = self.symbol(application.definition().into());

        let substitution = self
            .semantic_values
            .generic_substitution_data(application.substitution())
            .map_err(|_| TypeInspectionError::SemanticValue)?;

        let arguments = substitution
            .bindings()
            .iter()
            .map(|binding| self.argument(binding.argument(), depth + 1))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(format_application(name, arguments))
    }

    fn symbol(&mut self, symbol: AnySymbolId) -> String {
        self.referenced_symbols.insert(symbol);

        let identity = InspectionSymbolIdentity::from_symbol(self.symbols, symbol);

        identity.display_name().into_owned()
    }
}

fn check_depth(depth: usize) -> Result<(), TypeInspectionError> {
    if depth > MAXIMUM_TYPE_DEPTH {
        return Err(TypeInspectionError::Depth);
    }

    Ok(())
}

fn format_application(name: String, arguments: Vec<String>) -> String {
    if arguments.is_empty() {
        return name;
    }

    format!("{name}<{}>", arguments.join(", "))
}

fn format_borrow(kind: BorrowKind, target: String) -> String {
    match kind {
        BorrowKind::Shared => format!("&{target}"),
        BorrowKind::Mutable => format!("&mut {target}"),
    }
}

fn template_kind(template: &TypeExpressionTemplate) -> &'static str {
    match template {
        TypeExpressionTemplate::Resolved(_) => "resolved",
        TypeExpressionTemplate::Named { .. } => "named",
        TypeExpressionTemplate::TypeValuedMemberProjection { .. } => {
            "type_valued_member_projection"
        }
        TypeExpressionTemplate::Tuple(_) => "tuple",
        TypeExpressionTemplate::Array { .. } => "array",
        TypeExpressionTemplate::Slice(_) => "slice",
        TypeExpressionTemplate::Nullable(_) => "nullable",
        TypeExpressionTemplate::Borrow { .. } => "borrow",
        TypeExpressionTemplate::TraitView(_) => "trait_view",
        TypeExpressionTemplate::OwnedIndirection { .. } => "owned_indirection",
        TypeExpressionTemplate::Callable(_) => "callable",
    }
}

fn type_data_kind(data: &TypeData) -> &'static str {
    match data {
        TypeData::Error => "error",
        TypeData::Named { .. } => "named",
        TypeData::TypeParameter(_) => "type_parameter",
        TypeData::ContextualSelf(_) => "contextual_self",
        TypeData::TypeValuedMemberProjection { .. } => "type_valued_member_projection",
        TypeData::Tuple(_) => "tuple",
        TypeData::Array { .. } => "array",
        TypeData::Slice(_) => "slice",
        TypeData::Generator(_) => "generator",
        TypeData::Nullable(_) => "nullable",
        TypeData::Borrow { .. } => "borrow",
        TypeData::TraitView(_) => "trait_view",
        TypeData::OwnedIndirection { .. } => "owned_indirection",
        TypeData::Callable(_) => "callable",
    }
}
