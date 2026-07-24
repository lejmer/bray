use std::collections::{BTreeMap, BTreeSet};

use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{
    Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticResult, SeverityKind,
};
use bray_symbols::{
    AvailableCompilerKnownSymbols, DeclaredCopyContract, DeclaredTypeRepresentation,
    GenericArgument, GenericArgumentTemplate, GenericParameterSymbolId,
    GenericTypeParameterSymbolId, NamedTypeSymbolId, TypeData, TypeExpressionTemplate, TypeId,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct MemberRepresentation {
    pub(super) finite: bool,
    pub(super) plain: bool,
    pub(super) copyable: Copyability,
    pub(super) copy_dependencies: BTreeSet<GenericTypeParameterSymbolId>,
    pub(super) recovered: bool,
}

impl MemberRepresentation {
    const SCALAR: Self = Self {
        finite: true,
        plain: true,
        copyable: Copyability::Always,
        copy_dependencies: BTreeSet::new(),
        recovered: false,
    };

    const fn invalid(recovered: bool) -> Self {
        Self {
            finite: false,
            plain: false,
            copyable: Copyability::Never,
            copy_dependencies: BTreeSet::new(),
            recovered,
        }
    }

    fn aggregate(values: impl IntoIterator<Item = Self>) -> Self {
        values.into_iter().fold(Self::SCALAR, |mut result, value| {
            result.finite &= value.finite;
            result.plain &= value.plain;
            result.copyable = combine_copyability(result.copyable, value.copyable);
            result.copy_dependencies.extend(value.copy_dependencies);
            result.recovered |= value.recovered;

            result
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

        let imported = self.context.imported_type_representation(subject)?;

        self.diagnostics
            .add_range(imported.diagnostics().iter().cloned());

        if let Some(result) = imported.value() {
            // Imported representation facts own Arc-backed tag storage.
            let result = result.clone();

            self.completed.insert(subject, result.clone());

            return Ok(result);
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

        let layout = self.check_layout(definition, &member_representation, &mut recovered)?;

        let (tags, tag_type) =
            self.check_union_tags(definition, layout.mode, layout.tag_type, &mut recovered)?;

        let copy = self.check_copy(definition, &member_representation, &mut recovered);

        let copy_dependencies = if copy == DeclaredCopyContract::Conditional {
            member_representation
                .copy_dependencies
                .iter()
                .copied()
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

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
            .with_properties(copy, plain_storage, finite_size, recovered)
            .with_copy_dependencies(copy_dependencies))
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
                parameters,
                arguments,
            } => self.check_named_template(*definition, parameters, arguments),
            TypeExpressionTemplate::TypeValuedMemberProjection { .. } => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Conditional,
                copy_dependencies: BTreeSet::new(),
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
                copy_dependencies: BTreeSet::new(),
                recovered: false,
            }),
            TypeExpressionTemplate::OwnedIndirection { .. } => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Never,
                copy_dependencies: BTreeSet::new(),
                recovered: false,
            }),
            TypeExpressionTemplate::Callable(_) => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Always,
                copy_dependencies: BTreeSet::new(),
                recovered: false,
            }),
        }
    }

    fn check_named_template(
        &mut self,
        subject: NamedTypeSymbolId,
        parameters: &[GenericParameterSymbolId],
        arguments: &[GenericArgumentTemplate],
    ) -> CheckerFactResult<MemberRepresentation> {
        let checked = self.check(subject)?;

        if parameters.is_empty()
            || !checked.has_finite_size()
            || checked.copy_dependencies().is_empty()
        {
            return Ok(member_representation(&checked));
        }

        let dependencies = checked
            .copy_dependencies()
            .iter()
            .filter_map(|dependency| {
                parameters
                    .iter()
                    .position(|parameter| *parameter == GenericParameterSymbolId::Type(*dependency))
                    .and_then(|index| arguments.get(index))
            })
            .filter_map(|argument| match argument {
                GenericArgumentTemplate::Type(ty) => Some(ty),
                GenericArgumentTemplate::Constant(_) => None,
            })
            .map(|argument| self.check_template(argument))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(apply_copy_dependencies(
            &checked,
            MemberRepresentation::aggregate(dependencies),
        ))
    }

    fn check_type(&mut self, ty: TypeId) -> CheckerFactResult<MemberRepresentation> {
        let data = self.context.semantic_values().type_data(ty).map_err(|_| {
            CheckerFactError::Infrastructure(CheckerInfrastructureError::SemanticValueUnavailable)
        })?;

        match data.as_ref() {
            TypeData::Error => Ok(MemberRepresentation::invalid(true)),
            TypeData::Named {
                definition,
                substitution,
            } => {
                if let Some(role) = named_representation_role(
                    self.context.available_compiler_known_symbols(),
                    *definition,
                ) {
                    return Ok(compiler_known_representation(role));
                }

                let representation = self.check(*definition)?;

                if representation.copy_dependencies().is_empty() {
                    return Ok(member_representation(&representation));
                }

                let substitution = self
                    .context
                    .semantic_values()
                    .generic_substitution_data(*substitution)
                    .map_err(|_| {
                        CheckerFactError::Infrastructure(
                            CheckerInfrastructureError::SemanticValueUnavailable,
                        )
                    })?;

                let dependencies = representation
                    .copy_dependencies()
                    .iter()
                    .filter_map(|parameter| {
                        substitution.argument_for(GenericParameterSymbolId::Type(*parameter))
                    })
                    .filter_map(|argument| match argument {
                        GenericArgument::Type(ty) => Some(ty),
                        GenericArgument::Constant(_) => None,
                    })
                    .map(|ty| self.check_type(ty))
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(apply_copy_dependencies(
                    &representation,
                    MemberRepresentation::aggregate(dependencies),
                ))
            }
            TypeData::TypeParameter(parameter) => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Conditional,
                copy_dependencies: BTreeSet::from([*parameter]),
                recovered: false,
            }),
            TypeData::ContextualSelf(_) => Ok(MemberRepresentation::invalid(false)),
            TypeData::TypeValuedMemberProjection { .. } => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Conditional,
                copy_dependencies: BTreeSet::new(),
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
                copy_dependencies: BTreeSet::new(),
                recovered: false,
            }),
            TypeData::OwnedIndirection { .. } => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Never,
                copy_dependencies: BTreeSet::new(),
                recovered: false,
            }),
            TypeData::Callable(_) => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Always,
                copy_dependencies: BTreeSet::new(),
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

fn member_representation(representation: &DeclaredTypeRepresentation) -> MemberRepresentation {
    MemberRepresentation {
        finite: representation.has_finite_size(),
        plain: representation.is_plain_storage(),
        copyable: match representation.copy_contract() {
            DeclaredCopyContract::Absent => Copyability::Never,
            DeclaredCopyContract::Unconditional => Copyability::Always,
            DeclaredCopyContract::Conditional => Copyability::Conditional,
        },
        copy_dependencies: representation.copy_dependencies().iter().copied().collect(),
        recovered: representation.is_recovered(),
    }
}

fn apply_copy_dependencies(
    representation: &DeclaredTypeRepresentation,
    dependencies: MemberRepresentation,
) -> MemberRepresentation {
    MemberRepresentation {
        finite: representation.has_finite_size(),
        plain: representation.is_plain_storage(),
        copyable: match representation.copy_contract() {
            DeclaredCopyContract::Absent => Copyability::Never,
            DeclaredCopyContract::Unconditional => Copyability::Always,
            DeclaredCopyContract::Conditional => dependencies.copyable,
        },
        copy_dependencies: dependencies.copy_dependencies,
        recovered: representation.is_recovered() || dependencies.recovered,
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
            copy_dependencies: BTreeSet::new(),
            recovered: false,
        },
        RepresentationRole::Future | RepresentationRole::Task | RepresentationRole::PanicReport => {
            MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Never,
                copy_dependencies: BTreeSet::new(),
                recovered: false,
            }
        }
        RepresentationRole::Result
        | RepresentationRole::RunResult
        | RepresentationRole::ConversionError => MemberRepresentation {
            finite: true,
            plain: false,
            copyable: Copyability::Conditional,
            copy_dependencies: BTreeSet::new(),
            recovered: false,
        },
        RepresentationRole::BooleanTrue
        | RepresentationRole::BooleanFalse
        | RepresentationRole::UnitValue
        | RepresentationRole::NoneValue => MemberRepresentation::invalid(true),
        _ => MemberRepresentation::invalid(true),
    }
}
