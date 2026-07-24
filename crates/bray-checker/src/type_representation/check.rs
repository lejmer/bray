use std::collections::{BTreeMap, BTreeSet};

use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{
    Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticResult, SeverityKind,
};
use bray_symbols::{
    AvailableCompilerKnownSymbols, DeclaredCopyContract, DeclaredTypeRepresentation,
    NamedTypeSymbolId, TypeData, TypeExpressionTemplate, TypeId,
};

use crate::{CheckerFactError, CheckerFactResult, CheckerInfrastructureError, CheckerOutcome};

use super::model::{
    DeclaredStorageMember, DeclaredTypeDefinition, DeclaredUnionVariant, TypeRepresentationContext,
};

/// Derives one source-level named type representation contract.
pub fn check_declared_type_representation<C>(
    context: &C,
    subject: NamedTypeSymbolId,
) -> CheckerOutcome<DeclaredTypeRepresentation>
where
    C: TypeRepresentationContext + ?Sized,
{
    let mut checker = RepresentationChecker::new(context);

    match checker.check(subject) {
        Ok(value) => CheckerOutcome::Complete(DiagnosticResult::new(value, checker.diagnostics)),
        Err(CheckerFactError::Cancelled) => CheckerOutcome::Cancelled,
        Err(CheckerFactError::Infrastructure(error)) => {
            CheckerOutcome::InfrastructureFailure(error)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Copyability {
    Always,
    Conditional,
    Never,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct MemberRepresentation {
    pub(super) finite: bool,
    pub(super) plain: bool,
    pub(super) copyable: Copyability,
    pub(super) recovered: bool,
}

impl MemberRepresentation {
    const SCALAR: Self = Self {
        finite: true,
        plain: true,
        copyable: Copyability::Always,
        recovered: false,
    };

    const fn invalid(recovered: bool) -> Self {
        Self {
            finite: false,
            plain: false,
            copyable: Copyability::Never,
            recovered,
        }
    }

    fn aggregate(values: impl IntoIterator<Item = Self>) -> Self {
        values.into_iter().fold(Self::SCALAR, |result, value| Self {
            finite: result.finite && value.finite,
            plain: result.plain && value.plain,
            copyable: combine_copyability(result.copyable, value.copyable),
            recovered: result.recovered || value.recovered,
        })
    }
}

fn combine_copyability(left: Copyability, right: Copyability) -> Copyability {
    match (left, right) {
        (Copyability::Never, _) | (_, Copyability::Never) => Copyability::Never,
        (Copyability::Conditional, _) | (_, Copyability::Conditional) => Copyability::Conditional,
        (Copyability::Always, Copyability::Always) => Copyability::Always,
    }
}

pub(super) struct RepresentationChecker<'context, C>
where
    C: TypeRepresentationContext + ?Sized,
{
    pub(super) context: &'context C,
    pub(super) diagnostics: DiagnosticBag,
    active: BTreeSet<NamedTypeSymbolId>,
    completed: BTreeMap<NamedTypeSymbolId, DeclaredTypeRepresentation>,
}

impl<'context, C> RepresentationChecker<'context, C>
where
    C: TypeRepresentationContext + ?Sized,
{
    fn new(context: &'context C) -> Self {
        Self {
            context,
            diagnostics: DiagnosticBag::new(),
            active: BTreeSet::new(),
            completed: BTreeMap::new(),
        }
    }

    fn check(
        &mut self,
        subject: NamedTypeSymbolId,
    ) -> CheckerFactResult<DeclaredTypeRepresentation> {
        if let Some(result) = self.completed.get(&subject) {
            // Completed contracts own Arc-backed tag storage and are cheap to share.
            return Ok(result.clone());
        }

        if self.context.cancellation().is_cancelled() {
            return Err(CheckerFactError::Cancelled);
        }

        // TODO(BRA-272): Replace this local guard with the compilation-wide semantic
        // recursion policy once that policy is defined.
        if self.active.len() >= 256 {
            return Ok(DeclaredTypeRepresentation::new(subject).with_properties(
                DeclaredCopyContract::Absent,
                false,
                false,
                true,
            ));
        }

        if !self.active.insert(subject) {
            return Ok(DeclaredTypeRepresentation::new(subject));
        }

        let definition = self.context.type_definition(subject)?;

        self.diagnostics
            .add_range(definition.diagnostics().iter().cloned());

        let result = self.check_definition(definition.value())?;

        self.active.remove(&subject);

        // The memoized and returned contracts share immutable tag storage.
        self.completed.insert(subject, result.clone());

        Ok(result)
    }

    fn check_definition(
        &mut self,
        definition: &DeclaredTypeDefinition,
    ) -> CheckerFactResult<DeclaredTypeRepresentation> {
        let members = definition
            .fields()
            .iter()
            .chain(
                definition
                    .variants()
                    .iter()
                    .flat_map(DeclaredUnionVariant::payload),
            )
            .map(|member| self.check_member(member))
            .collect::<Result<Vec<_>, _>>()?;

        let member_representation = MemberRepresentation::aggregate(members);
        let mut recovered = definition.is_recovered() || member_representation.recovered;

        let layout = self.check_layout(definition, member_representation, &mut recovered)?;
        let (tags, tag_type) =
            self.check_union_tags(definition, layout.mode, layout.tag_type, &mut recovered)?;
        let copy = self.check_copy(definition, member_representation, &mut recovered);

        let plain_storage = !definition.has_lifecycle() && member_representation.plain;
        let finite_size = member_representation.finite;

        if !finite_size {
            self.add_diagnostic(
                DiagnosticKind::CheckingRecursiveTypeRepresentation,
                definition.span(),
            );

            recovered = true;
        }

        Ok(DeclaredTypeRepresentation::new(definition.subject())
            .with_layout(
                layout.mode,
                layout.alignment,
                layout.packing,
                tag_type.map(|tag_type| tag_type.ty()),
            )
            .with_union_tags(tags)
            .with_properties(copy, plain_storage, finite_size, recovered))
    }

    fn check_member(
        &mut self,
        member: &DeclaredStorageMember,
    ) -> CheckerFactResult<MemberRepresentation> {
        let mut result = self.check_template(member.ty())?;

        result.recovered |= member.is_recovered();

        if !result.finite {
            self.add_diagnostic(DiagnosticKind::CheckingInvalidStoredType, member.span());
        }

        Ok(result)
    }

    fn check_template(
        &mut self,
        template: &TypeExpressionTemplate,
    ) -> CheckerFactResult<MemberRepresentation> {
        match template {
            TypeExpressionTemplate::Resolved(ty) => self.check_type(*ty),
            TypeExpressionTemplate::Named {
                definition,
                arguments,
                ..
            } => {
                let definition = self.check(*definition)?;
                let argument_copy = arguments
                    .iter()
                    .filter_map(|argument| match argument {
                        bray_symbols::GenericArgumentTemplate::Type(ty) => {
                            Some(self.check_template(ty))
                        }
                        bray_symbols::GenericArgumentTemplate::Constant(_) => None,
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                let arguments = MemberRepresentation::aggregate(argument_copy);

                Ok(MemberRepresentation {
                    finite: definition.has_finite_size() && arguments.finite,
                    plain: definition.is_plain_storage() && arguments.plain,
                    copyable: match definition.copy_contract() {
                        DeclaredCopyContract::Absent => Copyability::Never,
                        DeclaredCopyContract::Unconditional => Copyability::Always,
                        DeclaredCopyContract::Conditional => arguments.copyable,
                    },
                    recovered: definition.is_recovered() || arguments.recovered,
                })
            }
            TypeExpressionTemplate::TypeValuedMemberProjection { .. } => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Conditional,
                recovered: false,
            }),
            TypeExpressionTemplate::Tuple(elements) => elements
                .iter()
                .map(|element| self.check_template(element))
                .collect::<Result<Vec<_>, _>>()
                .map(MemberRepresentation::aggregate),
            TypeExpressionTemplate::Array { element, .. }
            | TypeExpressionTemplate::Nullable(element) => self.check_template(element),
            TypeExpressionTemplate::Slice(_) | TypeExpressionTemplate::TraitView(_) => {
                Ok(MemberRepresentation::invalid(false))
            }
            TypeExpressionTemplate::Borrow { kind, .. } => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: match kind {
                    bray_symbols::BorrowKind::Shared => Copyability::Always,
                    bray_symbols::BorrowKind::Mutable => Copyability::Never,
                },
                recovered: false,
            }),
            TypeExpressionTemplate::OwnedIndirection { .. } => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Never,
                recovered: false,
            }),
            TypeExpressionTemplate::Callable(_) => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Always,
                recovered: false,
            }),
        }
    }

    fn check_type(&mut self, ty: TypeId) -> CheckerFactResult<MemberRepresentation> {
        let data = self.context.semantic_values().type_data(ty).map_err(|_| {
            CheckerFactError::Infrastructure(CheckerInfrastructureError::SemanticValueUnavailable)
        })?;

        match data.as_ref() {
            TypeData::Error => Ok(MemberRepresentation::invalid(true)),
            TypeData::Named { definition, .. } => {
                if let Some(role) = named_representation_role(
                    self.context.available_compiler_known_symbols(),
                    *definition,
                ) {
                    return Ok(compiler_known_representation(role));
                }

                let representation = self.check(*definition)?;

                Ok(MemberRepresentation {
                    finite: representation.has_finite_size(),
                    plain: representation.is_plain_storage(),
                    copyable: match representation.copy_contract() {
                        DeclaredCopyContract::Absent => Copyability::Never,
                        DeclaredCopyContract::Unconditional => Copyability::Always,
                        DeclaredCopyContract::Conditional => Copyability::Conditional,
                    },
                    recovered: representation.is_recovered(),
                })
            }
            TypeData::TypeParameter(_) => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Conditional,
                recovered: false,
            }),
            TypeData::ContextualSelf(_) => Ok(MemberRepresentation::invalid(false)),
            TypeData::TypeValuedMemberProjection { .. } => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Conditional,
                recovered: false,
            }),
            TypeData::Tuple(elements) => elements
                .iter()
                .map(|element| self.check_type(*element))
                .collect::<Result<Vec<_>, _>>()
                .map(MemberRepresentation::aggregate),
            TypeData::Array { element, .. } | TypeData::Nullable(element) => {
                self.check_type(*element)
            }
            TypeData::Slice(_) | TypeData::TraitView(_) | TypeData::Generator(_) => {
                Ok(MemberRepresentation::invalid(false))
            }
            TypeData::Borrow { kind, .. } => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: match kind {
                    bray_symbols::BorrowKind::Shared => Copyability::Always,
                    bray_symbols::BorrowKind::Mutable => Copyability::Never,
                },
                recovered: false,
            }),
            TypeData::OwnedIndirection { .. } => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Never,
                recovered: false,
            }),
            TypeData::Callable(_) => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Always,
                recovered: false,
            }),
        }
    }

    pub(super) fn add_diagnostic(&mut self, kind: DiagnosticKind, span: bray_source::SourceSpan) {
        let id = u32::try_from(self.diagnostics.len()).unwrap_or(u32::MAX);

        self.diagnostics.add(
            Diagnostic::new(DiagnosticId::new(id), kind, SeverityKind::Error)
                .with_primary_span(span),
        );
    }
}

fn named_representation_role(
    symbols: &AvailableCompilerKnownSymbols,
    definition: NamedTypeSymbolId,
) -> Option<RepresentationRole> {
    match definition {
        NamedTypeSymbolId::Struct(definition) => symbols.symbol_representation(definition),
        NamedTypeSymbolId::Union(definition) => symbols.symbol_representation(definition),
    }
}

fn compiler_known_representation(role: RepresentationRole) -> MemberRepresentation {
    if role.numeric_kind().is_some()
        || matches!(
            role,
            RepresentationRole::ScalarBool
                | RepresentationRole::ScalarChar
                | RepresentationRole::Unit
                | RepresentationRole::Never
        )
    {
        return MemberRepresentation::SCALAR;
    }

    match role {
        RepresentationRole::String | RepresentationRole::RawPointer => MemberRepresentation {
            finite: true,
            plain: false,
            copyable: Copyability::Always,
            recovered: false,
        },
        RepresentationRole::Future | RepresentationRole::Task | RepresentationRole::PanicReport => {
            MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Never,
                recovered: false,
            }
        }
        RepresentationRole::Result
        | RepresentationRole::RunResult
        | RepresentationRole::ConversionError => MemberRepresentation {
            finite: true,
            plain: false,
            copyable: Copyability::Conditional,
            recovered: false,
        },
        RepresentationRole::BooleanTrue
        | RepresentationRole::BooleanFalse
        | RepresentationRole::UnitValue
        | RepresentationRole::NoneValue => MemberRepresentation::invalid(true),
        _ => MemberRepresentation::invalid(true),
    }
}
