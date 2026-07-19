use crate::CompilerKnownOperationRole;
use crate::catalog::operation::{
    CompilerKnownOperationComponentError, CompilerKnownOperationComponents,
};

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

        let mut collected = CompilerKnownOperationComponents::new();

        for (index, declaration) in components {
            let Some(kind) = declaration.kind else {
                continue;
            };

            match collected.insert(role, kind, index) {
                Ok(()) => {}
                Err(CompilerKnownOperationComponentError::Incompatible) => {
                    diagnostics.push(CatalogDiagnostic::new(
                        declaration.anchor,
                        CatalogDiagnosticKind::IncompatibleOperationRole {
                            role,
                            declaration: kind,
                        },
                    ));
                }
                Err(CompilerKnownOperationComponentError::Duplicate) => {
                    diagnostics.push(CatalogDiagnostic::new(
                        declaration.anchor,
                        CatalogDiagnosticKind::DuplicateOperationComponent { role },
                    ));
                }
            }
        }

        if !collected.is_complete(role) {
            diagnostics.push(CatalogDiagnostic::new(
                first.anchor,
                CatalogDiagnosticKind::IncompleteOperationContract { role },
            ));

            continue;
        }

        let Some(trait_definition) = collected.trait_definition() else {
            continue;
        };

        for member in [collected.associated_result_type(), collected.callable()]
            .into_iter()
            .flatten()
        {
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
