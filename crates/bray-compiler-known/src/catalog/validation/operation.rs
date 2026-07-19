use crate::operation::CompilerKnownOperationContractShape;
use crate::{CompilerKnownOperationRole, catalog::CatalogDeclarationKind};

use super::super::{CatalogDiagnostic, CatalogDiagnosticKind};
use super::model::{RawDeclaration, ValidatedDeclarationOwner};

pub(super) fn validate_operation_contracts(
    declarations: &[RawDeclaration],
    diagnostics: &mut Vec<CatalogDiagnostic>,
) {
    for role in CompilerKnownOperationRole::ALL.iter().copied() {
        let components = declarations
            .iter()
            .enumerate()
            .filter(|(_, declaration)| declaration.operation_role == Some(role))
            .collect::<Vec<_>>();

        let Some((_, first)) = components.first().copied() else {
            continue;
        };

        let mut trait_definition = None;
        let mut result_type_member = None;
        let mut fixed_result_type = None;
        let mut callable = None;
        let contract_shape = role.contract_shape();

        for (index, declaration) in components {
            let slot = match declaration.kind {
                Some(CatalogDeclarationKind::Trait) => &mut trait_definition,
                Some(CatalogDeclarationKind::TraitTypeMember) => &mut result_type_member,
                Some(CatalogDeclarationKind::Struct | CatalogDeclarationKind::Union)
                    if contract_shape
                        == CompilerKnownOperationContractShape::FixedResultCallable =>
                {
                    &mut fixed_result_type
                }
                Some(CatalogDeclarationKind::TraitCallableMember) => &mut callable,
                Some(kind) => {
                    diagnostics.push(CatalogDiagnostic::new(
                        declaration.anchor,
                        CatalogDiagnosticKind::IncompatibleOperationRole {
                            role,
                            declaration: kind,
                        },
                    ));

                    continue;
                }
                None => continue,
            };

            if slot.replace(index).is_some() {
                diagnostics.push(CatalogDiagnostic::new(
                    declaration.anchor,
                    CatalogDiagnosticKind::DuplicateOperationComponent { role },
                ));
            }
        }

        let complete = match contract_shape {
            CompilerKnownOperationContractShape::Trait => {
                trait_definition.is_some()
                    && result_type_member.is_none()
                    && fixed_result_type.is_none()
                    && callable.is_none()
            }
            CompilerKnownOperationContractShape::Callable => {
                trait_definition.is_some()
                    && result_type_member.is_none()
                    && fixed_result_type.is_none()
                    && callable.is_some()
            }
            CompilerKnownOperationContractShape::FixedResultCallable => {
                trait_definition.is_some()
                    && result_type_member.is_none()
                    && fixed_result_type.is_some()
                    && callable.is_some()
            }
            CompilerKnownOperationContractShape::AssociatedResultCallable => {
                trait_definition.is_some()
                    && result_type_member.is_some()
                    && fixed_result_type.is_none()
                    && callable.is_some()
            }
        };

        if !complete {
            diagnostics.push(CatalogDiagnostic::new(
                first.anchor,
                CatalogDiagnosticKind::IncompleteOperationContract { role },
            ));

            continue;
        }

        let Some(trait_definition) = trait_definition else {
            continue;
        };

        for member in [result_type_member, callable].into_iter().flatten() {
            if declarations[member].owner
                != Some(ValidatedDeclarationOwner::Declaration(trait_definition))
            {
                diagnostics.push(CatalogDiagnostic::new(
                    declarations[member].anchor,
                    CatalogDiagnosticKind::InvalidOperationComponentOwner { role },
                ));
            }
        }
    }
}
