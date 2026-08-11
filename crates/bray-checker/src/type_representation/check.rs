// rust-style: allow(module-too-large, reason = "declared type representation checking is one recursive checker with shared cycle and copyability state")

use std::collections::{BTreeMap, BTreeSet};

use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticResult,
    DiagnosticLabel, DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind,
    DiagnosticRelatedLocation, DiagnosticRelatedLocationKind, DiagnosticStoredTypeProblem,
    SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    AvailableCompilerKnownSymbols, DeclaredCopyContract, DeclaredStorageShape,
    DeclaredStructStorageMember, DeclaredTypeRepresentation, DeclaredUnionStorageMember,
    DeclaredUnionStorageVariant, GenericArgument, GenericArgumentTemplate,
    GenericParameterSymbolId, GenericTypeParameterSymbolId, NamedTypeSymbolId, TypeData,
    TypeExpressionTemplate, TypeId,
};

use crate::{CheckerFactError, CheckerFactResult, CheckerInfrastructureError, CheckerOutcome};

use super::model::{
    DeclaredStorageMember, DeclaredStorageMemberIdentity, DeclaredTypeDefinition,
    DeclaredUnionVariant, TypeRepresentationContext,
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

    match checker.check(subject, None) {
        Ok(value) => CheckerOutcome::Complete(DiagnosticResult::new(
            value.representation,
            checker.diagnostics,
        )),
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
    pub(super) non_copyable_members: BTreeSet<bray_source::SourceSpan>,
    stored_type_problems: BTreeSet<DiagnosticStoredTypeProblem>,
    recursive_cycles: Vec<RepresentationCycle>,
    pub(super) recovered: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RepresentationCycle {
    root: NamedTypeSymbolId,
    locations: Vec<SourceSpan>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActiveRepresentation {
    subject: NamedTypeSymbolId,
    declaration: SourceSpan,
    incoming_member: Option<SourceSpan>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CheckedRepresentation {
    representation: DeclaredTypeRepresentation,
    recursive_cycles: Vec<RepresentationCycle>,
}

impl MemberRepresentation {
    const SCALAR: Self = Self {
        finite: true,
        plain: true,
        copyable: Copyability::Always,
        copy_dependencies: BTreeSet::new(),
        non_copyable_members: BTreeSet::new(),
        stored_type_problems: BTreeSet::new(),
        recursive_cycles: Vec::new(),
        recovered: false,
    };

    fn invalid(problem: DiagnosticStoredTypeProblem) -> Self {
        Self {
            finite: false,
            plain: false,
            copyable: Copyability::Never,
            copy_dependencies: BTreeSet::new(),
            non_copyable_members: BTreeSet::new(),
            stored_type_problems: BTreeSet::from([problem]),
            recursive_cycles: Vec::new(),
            recovered: false,
        }
    }

    const fn recovered_invalid() -> Self {
        Self {
            finite: false,
            plain: false,
            copyable: Copyability::Never,
            copy_dependencies: BTreeSet::new(),
            non_copyable_members: BTreeSet::new(),
            stored_type_problems: BTreeSet::new(),
            recursive_cycles: Vec::new(),
            recovered: true,
        }
    }

    fn aggregate(values: impl IntoIterator<Item = Self>) -> Self {
        values.into_iter().fold(Self::SCALAR, |mut result, value| {
            result.finite &= value.finite;
            result.plain &= value.plain;
            result.copyable = combine_copyability(result.copyable, value.copyable);
            result.copy_dependencies.extend(value.copy_dependencies);

            result
                .non_copyable_members
                .extend(value.non_copyable_members);

            result.stored_type_problems.extend(value.stored_type_problems);

            for cycle in value.recursive_cycles {
                if !result.recursive_cycles.contains(&cycle) {
                    result.recursive_cycles.push(cycle);
                }
            }

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
    active: Vec<ActiveRepresentation>,
    completed: BTreeMap<NamedTypeSymbolId, DeclaredTypeRepresentation>,
    recursion_limit_reported: bool,
}

impl<'context, C> RepresentationChecker<'context, C>
where
    C: TypeRepresentationContext + ?Sized,
{
    fn new(context: &'context C) -> Self {
        Self {
            context,
            diagnostics: DiagnosticBag::new(),
            active: Vec::new(),
            completed: BTreeMap::new(),
            recursion_limit_reported: false,
        }
    }

    fn check(
        &mut self,
        subject: NamedTypeSymbolId,
        incoming_member: Option<SourceSpan>,
    ) -> CheckerFactResult<CheckedRepresentation> {
        if let Some(result) = self.completed.get(&subject) {
            // Completed contracts own Arc-backed tag storage and are cheap to share.
            return Ok(CheckedRepresentation {
                representation: result.clone(),
                recursive_cycles: Vec::new(),
            });
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

            return Ok(CheckedRepresentation {
                representation: result,
                recursive_cycles: Vec::new(),
            });
        }

        if let Some(cycle_start) = self
            .active
            .iter()
            .position(|active| active.subject == subject)
        {
            return Ok(CheckedRepresentation {
                representation: DeclaredTypeRepresentation::new(subject),
                recursive_cycles: vec![canonical_cycle(
                    &self.active[cycle_start..],
                    incoming_member,
                )],
            });
        }

        let definition = self.context.type_definition(subject)?;

        self.diagnostics
            .add_range(definition.diagnostics().iter().cloned());

        let maximum_recursion_depth = self.context.maximum_recursion_depth();

        if self.active.len() >= maximum_recursion_depth {
            if !self.recursion_limit_reported {
                self.add_limit_diagnostic(
                    DiagnosticKind::CheckingTypeRepresentationRecursionLimitExceeded,
                    definition.value().span(),
                    self.active.len().saturating_add(1),
                    maximum_recursion_depth,
                );

                self.recursion_limit_reported = true;
            }

            return Ok(CheckedRepresentation {
                representation: DeclaredTypeRepresentation::new(subject).with_properties(
                    DeclaredCopyContract::Absent,
                    false,
                    false,
                    true,
                ),
                recursive_cycles: Vec::new(),
            });
        }

        self.active.push(ActiveRepresentation {
            subject,
            declaration: definition.value().span(),
            incoming_member,
        });

        let result = self.check_definition(definition.value())?;

        self.active.pop();

        // The memoized and returned contracts share immutable tag storage.
        self.completed
            .insert(subject, result.representation.clone());

        Ok(result)
    }

    fn check_definition(
        &mut self,
        definition: &DeclaredTypeDefinition,
    ) -> CheckerFactResult<CheckedRepresentation> {
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

        let mut member_representation = MemberRepresentation::aggregate(members);
        let mut recovered = definition.is_recovered() || member_representation.recovered;

        let mut owned_cycles = Vec::new();

        member_representation.recursive_cycles.retain(|cycle| {
            if cycle.root == definition.subject() {
                owned_cycles.push(cycle.clone());

                false
            } else {
                true
            }
        });

        for cycle in &owned_cycles {
            self.add_recursive_representation_diagnostic(definition.span(), cycle);
            recovered = true;
        }

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
        let storage = storage_shape(definition);

        Ok(CheckedRepresentation {
            representation: DeclaredTypeRepresentation::new(definition.subject())
                .with_layout(
                    layout.mode,
                    layout.alignment,
                    layout.packing,
                    tag_type.map(|tag_type| tag_type.ty()),
                )
                .with_union_tags(tags)
                .with_storage(storage)
                .with_properties(copy, plain_storage, finite_size, recovered)
                .with_copy_dependencies(copy_dependencies),
            recursive_cycles: member_representation.recursive_cycles,
        })
    }

    fn check_member(
        &mut self,
        member: &DeclaredStorageMember,
    ) -> CheckerFactResult<MemberRepresentation> {
        let mut result = self.check_template(member.ty(), Some(member.span()))?;

        if result.copyable == Copyability::Never {
            result.non_copyable_members.insert(member.span());
        }

        result.recovered |= member.is_recovered();

        if !result.finite && result.recursive_cycles.is_empty() && result.stored_type_problems.is_empty() {
            result.stored_type_problems.insert(
                DiagnosticStoredTypeProblem::ReferencedTypeHasNoFiniteRepresentation,
            );
        }

        for problem in &result.stored_type_problems {
            self.add_invalid_stored_type_diagnostic(member.span(), *problem);
        }

        Ok(result)
    }

    fn check_template(
        &mut self,
        template: &TypeExpressionTemplate,
        origin: Option<SourceSpan>,
    ) -> CheckerFactResult<MemberRepresentation> {
        match template {
            TypeExpressionTemplate::Resolved(ty) => self.check_type(*ty, origin),
            TypeExpressionTemplate::Named {
                definition,
                parameters,
                arguments,
            } => self.check_named_template(*definition, parameters, arguments, origin),
            TypeExpressionTemplate::TypeValuedMemberProjection { .. } => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Conditional,
                copy_dependencies: BTreeSet::new(),
                non_copyable_members: BTreeSet::new(),
                stored_type_problems: BTreeSet::new(),
                recursive_cycles: Vec::new(),
                recovered: false,
            }),
            TypeExpressionTemplate::Tuple(elements) => elements
                .iter()
                .map(|element| self.check_template(element, origin))
                .collect::<Result<Vec<_>, _>>()
                .map(MemberRepresentation::aggregate),
            TypeExpressionTemplate::Array { element, .. }
            | TypeExpressionTemplate::Nullable(element) => self.check_template(element, origin),
            TypeExpressionTemplate::Slice(_) => Ok(MemberRepresentation::invalid(
                DiagnosticStoredTypeProblem::Slice,
            )),
            TypeExpressionTemplate::TraitView(_) => Ok(MemberRepresentation::invalid(
                DiagnosticStoredTypeProblem::TraitView,
            )),
            TypeExpressionTemplate::Borrow { kind, .. } => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: match kind {
                    bray_symbols::BorrowKind::Shared => Copyability::Always,
                    bray_symbols::BorrowKind::Mutable => Copyability::Never,
                },
                copy_dependencies: BTreeSet::new(),
                non_copyable_members: BTreeSet::new(),
                stored_type_problems: BTreeSet::new(),
                recursive_cycles: Vec::new(),
                recovered: false,
            }),
            TypeExpressionTemplate::OwnedIndirection { .. } => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Never,
                copy_dependencies: BTreeSet::new(),
                non_copyable_members: BTreeSet::new(),
                stored_type_problems: BTreeSet::new(),
                recursive_cycles: Vec::new(),
                recovered: false,
            }),
            TypeExpressionTemplate::Callable(_) => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Always,
                copy_dependencies: BTreeSet::new(),
                non_copyable_members: BTreeSet::new(),
                stored_type_problems: BTreeSet::new(),
                recursive_cycles: Vec::new(),
                recovered: false,
            }),
        }
    }

    fn check_named_template(
        &mut self,
        subject: NamedTypeSymbolId,
        parameters: &[GenericParameterSymbolId],
        arguments: &[GenericArgumentTemplate],
        origin: Option<SourceSpan>,
    ) -> CheckerFactResult<MemberRepresentation> {
        let checked = self.check(subject, origin)?;

        if parameters.is_empty()
            || !checked.representation.has_finite_size()
            || checked.representation.copy_dependencies().is_empty()
        {
            return Ok(member_representation(&checked));
        }

        let dependencies = checked
            .representation
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
            .map(|argument| self.check_template(argument, origin))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(apply_copy_dependencies(
            checked,
            MemberRepresentation::aggregate(dependencies),
        ))
    }

    fn check_type(
        &mut self,
        ty: TypeId,
        origin: Option<SourceSpan>,
    ) -> CheckerFactResult<MemberRepresentation> {
        let data = self.context.semantic_values().type_data(ty).map_err(|_| {
            CheckerFactError::Infrastructure(CheckerInfrastructureError::SemanticValueUnavailable)
        })?;

        match data.as_ref() {
            TypeData::Error => Ok(MemberRepresentation::recovered_invalid()),
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

                let representation = self.check(*definition, origin)?;

                if representation.representation.copy_dependencies().is_empty() {
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
                    .representation
                    .copy_dependencies()
                    .iter()
                    .filter_map(|parameter| {
                        substitution.argument_for(GenericParameterSymbolId::Type(*parameter))
                    })
                    .filter_map(|argument| match argument {
                        GenericArgument::Type(ty) => Some(ty),
                        GenericArgument::Constant(_) => None,
                    })
                    .map(|ty| self.check_type(ty, origin))
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(apply_copy_dependencies(
                    representation,
                    MemberRepresentation::aggregate(dependencies),
                ))
            }
            TypeData::TypeParameter(parameter) => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Conditional,
                copy_dependencies: BTreeSet::from([*parameter]),
                non_copyable_members: BTreeSet::new(),
                stored_type_problems: BTreeSet::new(),
                recursive_cycles: Vec::new(),
                recovered: false,
            }),
            TypeData::ContextualSelf(_) => Ok(MemberRepresentation::invalid(
                DiagnosticStoredTypeProblem::ContextualSelf,
            )),
            TypeData::TypeValuedMemberProjection { .. } => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Conditional,
                copy_dependencies: BTreeSet::new(),
                non_copyable_members: BTreeSet::new(),
                stored_type_problems: BTreeSet::new(),
                recursive_cycles: Vec::new(),
                recovered: false,
            }),
            TypeData::Tuple(elements) => elements
                .iter()
                .map(|element| self.check_type(*element, origin))
                .collect::<Result<Vec<_>, _>>()
                .map(MemberRepresentation::aggregate),
            TypeData::Array { element, .. } | TypeData::Nullable(element) => {
                self.check_type(*element, origin)
            }
            TypeData::Slice(_) => Ok(MemberRepresentation::invalid(
                DiagnosticStoredTypeProblem::Slice,
            )),
            TypeData::TraitView(_) => Ok(MemberRepresentation::invalid(
                DiagnosticStoredTypeProblem::TraitView,
            )),
            TypeData::Generator(_) => Ok(MemberRepresentation::invalid(
                DiagnosticStoredTypeProblem::Generator,
            )),
            TypeData::Borrow { kind, .. } => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: match kind {
                    bray_symbols::BorrowKind::Shared => Copyability::Always,
                    bray_symbols::BorrowKind::Mutable => Copyability::Never,
                },
                copy_dependencies: BTreeSet::new(),
                non_copyable_members: BTreeSet::new(),
                stored_type_problems: BTreeSet::new(),
                recursive_cycles: Vec::new(),
                recovered: false,
            }),
            TypeData::OwnedIndirection { .. } => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Never,
                copy_dependencies: BTreeSet::new(),
                non_copyable_members: BTreeSet::new(),
                stored_type_problems: BTreeSet::new(),
                recursive_cycles: Vec::new(),
                recovered: false,
            }),
            TypeData::Callable(_) => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Always,
                copy_dependencies: BTreeSet::new(),
                non_copyable_members: BTreeSet::new(),
                stored_type_problems: BTreeSet::new(),
                recursive_cycles: Vec::new(),
                recovered: false,
            }),
        }
    }

    fn add_limit_diagnostic(
        &mut self,
        kind: DiagnosticKind,
        span: bray_source::SourceSpan,
        actual: usize,
        maximum: usize,
    ) {
        let id = u32::try_from(self.diagnostics.len()).unwrap_or(u32::MAX);

        self.diagnostics.add(
            Diagnostic::new(DiagnosticId::new(id), kind, SeverityKind::Error)
                .with_primary_span(span)
                .with_label(DiagnosticLabel::primary(
                    DiagnosticLabelKind::InvalidTypeRepresentationContract,
                    span,
                ))
                .with_arg(DiagnosticArg::actual_count(
                    u64::try_from(actual).unwrap_or(u64::MAX),
                ))
                .with_arg(DiagnosticArg::maximum_count(
                    u64::try_from(maximum).unwrap_or(u64::MAX),
                )),
        );
    }

    fn add_invalid_stored_type_diagnostic(
        &mut self,
        span: SourceSpan,
        problem: DiagnosticStoredTypeProblem,
    ) {
        let id = u32::try_from(self.diagnostics.len()).unwrap_or(u32::MAX);

        self.diagnostics.add(
            Diagnostic::new(
                DiagnosticId::new(id),
                DiagnosticKind::CheckingInvalidStoredType,
                SeverityKind::Error,
            )
            .with_primary_span(span)
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::InvalidTypeRepresentationContract,
                span,
            ))
            .with_arg(DiagnosticArg::stored_type_problem(problem))
            .with_note(DiagnosticNote::new(
                DiagnosticNoteKind::StoredTypeRequiresIndirection,
            )),
        );
    }

    fn add_recursive_representation_diagnostic(
        &mut self,
        primary_span: SourceSpan,
        cycle: &RepresentationCycle,
    ) {
        let id = u32::try_from(self.diagnostics.len()).unwrap_or(u32::MAX);
        let actual = u64::try_from(cycle.locations.len()).unwrap_or(u64::MAX);

        let mut diagnostic = Diagnostic::new(
            DiagnosticId::new(id),
            DiagnosticKind::CheckingRecursiveTypeRepresentation,
            SeverityKind::Error,
        )
        .with_primary_span(primary_span)
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::InvalidTypeRepresentationContract,
            primary_span,
        ))
        .with_arg(DiagnosticArg::actual_count(actual));

        for location in cycle
            .locations
            .iter()
            .copied()
            .filter(|location| *location != primary_span)
        {
            diagnostic = diagnostic.with_related_location(DiagnosticRelatedLocation::new(
                DiagnosticRelatedLocationKind::RepresentationCycleLocation,
                location,
            ));
        }

        self.diagnostics.add(diagnostic);
    }
}

fn canonical_cycle(
    active: &[ActiveRepresentation],
    closing_member: Option<SourceSpan>,
) -> RepresentationCycle {
    let root_index = active
        .iter()
        .enumerate()
        .min_by_key(|(_, entry)| entry.subject)
        .map_or(0, |(index, _)| index);

    let mut locations = Vec::with_capacity(active.len().saturating_mul(2));

    for offset in 0..active.len() {
        let index = (root_index + offset) % active.len();
        let next_index = (index + 1) % active.len();

        push_distinct_location(&mut locations, active[index].declaration);

        let edge = if next_index == 0 {
            closing_member
        } else {
            active[next_index].incoming_member
        };

        if let Some(edge) = edge {
            push_distinct_location(&mut locations, edge);
        }
    }

    RepresentationCycle {
        root: active[root_index].subject,
        locations,
    }
}

fn push_distinct_location(locations: &mut Vec<SourceSpan>, location: SourceSpan) {
    if !locations.contains(&location) {
        locations.push(location);
    }
}

fn storage_shape(definition: &DeclaredTypeDefinition) -> DeclaredStorageShape {
    match definition.subject() {
        NamedTypeSymbolId::Struct(_) => DeclaredStorageShape::Structure(
            definition
                .fields()
                .iter()
                .filter_map(|member| match member.identity() {
                    DeclaredStorageMemberIdentity::StructField(field) => Some(
                        DeclaredStructStorageMember::new(Some(field), member.ty().clone()),
                    ),
                    DeclaredStorageMemberIdentity::UnionPayloadField(_) => None,
                })
                .collect(),
        ),
        NamedTypeSymbolId::Union(_) => DeclaredStorageShape::Union(
            definition
                .variants()
                .iter()
                .map(|variant| {
                    let members =
                        variant
                            .payload()
                            .iter()
                            .filter_map(|member| match member.identity() {
                                DeclaredStorageMemberIdentity::UnionPayloadField(field) => {
                                    Some(DeclaredUnionStorageMember::new(
                                        Some(field),
                                        member.ty().clone(),
                                    ))
                                }
                                DeclaredStorageMemberIdentity::StructField(_) => None,
                            });

                    DeclaredUnionStorageVariant::new(variant.id(), members)
                })
                .collect(),
        ),
    }
}

fn member_representation(checked: &CheckedRepresentation) -> MemberRepresentation {
    let representation = &checked.representation;

    MemberRepresentation {
        finite: representation.has_finite_size(),
        plain: representation.is_plain_storage(),
        copyable: match representation.copy_contract() {
            DeclaredCopyContract::Absent => Copyability::Never,
            DeclaredCopyContract::Unconditional => Copyability::Always,
            DeclaredCopyContract::Conditional => Copyability::Conditional,
        },
        copy_dependencies: representation.copy_dependencies().iter().copied().collect(),
        non_copyable_members: BTreeSet::new(),
        stored_type_problems: BTreeSet::new(),
        recursive_cycles: checked.recursive_cycles.clone(),
        recovered: representation.is_recovered(),
    }
}

fn apply_copy_dependencies(
    checked: CheckedRepresentation,
    dependencies: MemberRepresentation,
) -> MemberRepresentation {
    let representation = &checked.representation;
    let mut recursive_cycles = checked.recursive_cycles;

    for cycle in dependencies.recursive_cycles {
        if !recursive_cycles.contains(&cycle) {
            recursive_cycles.push(cycle);
        }
    }

    MemberRepresentation {
        finite: representation.has_finite_size(),
        plain: representation.is_plain_storage(),
        copyable: match representation.copy_contract() {
            DeclaredCopyContract::Absent => Copyability::Never,
            DeclaredCopyContract::Unconditional => Copyability::Always,
            DeclaredCopyContract::Conditional => dependencies.copyable,
        },
        copy_dependencies: dependencies.copy_dependencies,
        non_copyable_members: dependencies.non_copyable_members,
        stored_type_problems: dependencies.stored_type_problems,
        recursive_cycles,
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
            non_copyable_members: BTreeSet::new(),
            stored_type_problems: BTreeSet::new(),
            recursive_cycles: Vec::new(),
            recovered: false,
        },
        RepresentationRole::Future | RepresentationRole::Task | RepresentationRole::PanicReport => {
            MemberRepresentation {
                finite: true,
                plain: false,
                copyable: Copyability::Never,
                copy_dependencies: BTreeSet::new(),
                non_copyable_members: BTreeSet::new(),
                stored_type_problems: BTreeSet::new(),
                recursive_cycles: Vec::new(),
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
            non_copyable_members: BTreeSet::new(),
            stored_type_problems: BTreeSet::new(),
            recursive_cycles: Vec::new(),
            recovered: false,
        },
        RepresentationRole::BooleanTrue
        | RepresentationRole::BooleanFalse
        | RepresentationRole::UnitValue
        | RepresentationRole::NoneValue => MemberRepresentation::recovered_invalid(),
        _ => MemberRepresentation::recovered_invalid(),
    }
}
