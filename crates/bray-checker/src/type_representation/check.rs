// rust-style: allow(module-too-large, reason = "declared type representation checking is one recursive checker with shared cycle and copyability state")

use std::collections::{BTreeMap, BTreeSet};

use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind, DiagnosticRelatedLocation,
    DiagnosticRelatedLocationKind, DiagnosticResult, DiagnosticStoredTypeProblem, SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    AvailableCompilerKnownSymbols, DeclaredCopyContract, DeclaredStorageShape,
    DeclaredStructStorageMember, DeclaredTypeRepresentation, DeclaredUnionStorageMember,
    DeclaredUnionStorageVariant, GenericArgument, GenericArgumentTemplate,
    GenericParameterSymbolId, GenericTypeParameterSymbolId, NamedTypeSymbolId, TypeData,
    TypeExpressionTemplate, TypeId,
};

use crate::{CheckerInfrastructureError, CheckerOutcome, CheckerQueryError, CheckerQueryResult};

use super::model::{
    DeclaredStorageMember, DeclaredStorageMemberIdentity, DeclaredTypeDefinition,
    DeclaredUnionVariant, TypeRepresentationContext,
};

type RepresentationQueryResult<C, T> =
    CheckerQueryResult<T, <C as TypeRepresentationContext>::UpstreamError>;

/// Derives one source-level named type representation contract.
pub fn check_declared_type_representation<C>(
    context: &C,
    subject: NamedTypeSymbolId,
) -> CheckerOutcome<DeclaredTypeRepresentation, C::UpstreamError>
where
    C: TypeRepresentationContext + ?Sized,
{
    let mut checker = RepresentationChecker::new(context);

    match checker.check(subject, None) {
        Ok(value) => CheckerOutcome::Complete(DiagnosticResult::new(
            value.representation,
            checker.diagnostics,
        )),
        Err(CheckerQueryError::Cancelled) => CheckerOutcome::Cancelled,
        Err(CheckerQueryError::Infrastructure(error)) => {
            CheckerOutcome::InfrastructureFailure(error)
        }
        Err(CheckerQueryError::Upstream(error)) => CheckerOutcome::UpstreamFailure(error),
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
    c_compatible: bool,
    flexible: bool,
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
        c_compatible: true,
        flexible: false,
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
            c_compatible: false,
            flexible: false,
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
            c_compatible: false,
            flexible: false,
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
            result.c_compatible &= value.c_compatible;
            result.flexible |= value.flexible;
            result.copyable = combine_copyability(result.copyable, value.copyable);
            result.copy_dependencies.extend(value.copy_dependencies);

            result
                .non_copyable_members
                .extend(value.non_copyable_members);

            result
                .stored_type_problems
                .extend(value.stored_type_problems);

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
    ) -> RepresentationQueryResult<C, CheckedRepresentation> {
        if let Some(result) = self.completed.get(&subject) {
            // Completed contracts own Arc-backed tag storage and are cheap to share.
            return Ok(CheckedRepresentation {
                representation: result.clone(),
                recursive_cycles: Vec::new(),
            });
        }

        if self.context.cancellation().is_cancelled() {
            return Err(CheckerQueryError::Cancelled);
        }

        let imported = self.context.imported_type_representation(subject)?;

        self.diagnostics
            .add_range(imported.diagnostics().iter().cloned());

        if let Some(result) = imported.value() {
            // Imported representation records own Arc-backed tag storage.
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
    ) -> RepresentationQueryResult<C, CheckedRepresentation> {
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

        let flexible_members = members
            .iter()
            .enumerate()
            .filter(|(_, member)| member.flexible)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();

        let mut member_representation = MemberRepresentation::aggregate(members.iter().cloned());
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

        if !flexible_members.is_empty() {
            let valid_position = matches!(definition.subject(), NamedTypeSymbolId::Struct(_))
                && flexible_members.as_slice() == [definition.fields().len().saturating_sub(1)];

            let valid_element = flexible_members
                .iter()
                .all(|index| members[*index].plain && members[*index].c_compatible);

            if !valid_position || layout.mode != bray_symbols::DeclaredLayoutMode::C {
                for index in &flexible_members {
                    self.add_invalid_stored_type_diagnostic(
                        definition
                            .fields()
                            .get(*index)
                            .map_or(definition.span(), DeclaredStorageMember::span),
                        DiagnosticStoredTypeProblem::FlexibleArrayRequiresFinalCStructField,
                    );
                }

                recovered = true;
            }

            if !valid_element {
                for index in &flexible_members {
                    if !members[*index].plain || !members[*index].c_compatible {
                        self.add_invalid_stored_type_diagnostic(
                            definition
                                .fields()
                                .get(*index)
                                .map_or(definition.span(), DeclaredStorageMember::span),
                            DiagnosticStoredTypeProblem::FlexibleArrayElementRequiresPlainCStorage,
                        );
                    }
                }

                recovered = true;
            }
        }

        let (tags, tag_type) = self.check_union_tags(
            definition,
            layout.mode,
            layout.tag_type,
            layout.tagless,
            &mut recovered,
        )?;

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

        let opaque = !definition.has_body() && layout.size.is_some() && layout.alignment.is_some();
        let incomplete = !definition.has_body() && !opaque;

        let plain_storage =
            opaque || (!incomplete && !definition.has_lifecycle() && member_representation.plain);

        let flexible = !flexible_members.is_empty();
        let finite_size = opaque || (!incomplete && !flexible && member_representation.finite);
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
                .with_tagless_union(layout.tagless)
                .with_opaque_size(layout.size)
                .with_incomplete(incomplete)
                .with_storage(storage)
                .with_properties(copy, plain_storage, finite_size, recovered)
                .with_copy_dependencies(copy_dependencies),
            recursive_cycles: member_representation.recursive_cycles,
        })
    }

    fn check_member(
        &mut self,
        member: &DeclaredStorageMember,
    ) -> RepresentationQueryResult<C, MemberRepresentation> {
        let mut result = self.check_template(member.ty(), Some(member.span()))?;

        if result.copyable == Copyability::Never {
            result.non_copyable_members.insert(member.span());
        }

        result.recovered |= member.is_recovered();

        if !result.finite
            && !result.flexible
            && result.recursive_cycles.is_empty()
            && result.stored_type_problems.is_empty()
        {
            result
                .stored_type_problems
                .insert(DiagnosticStoredTypeProblem::ReferencedTypeHasNoFiniteRepresentation);
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
    ) -> RepresentationQueryResult<C, MemberRepresentation> {
        match template {
            TypeExpressionTemplate::Resolved(ty) => self.check_type(*ty, origin),
            TypeExpressionTemplate::Named {
                definition,
                parameters,
                arguments,
            } => self.check_named_template(*definition, parameters, arguments, origin),
            TypeExpressionTemplate::CallableContract { .. } => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                c_compatible: false,
                flexible: false,
                copyable: Copyability::Always,
                copy_dependencies: BTreeSet::new(),
                non_copyable_members: BTreeSet::new(),
                stored_type_problems: BTreeSet::new(),
                recursive_cycles: Vec::new(),
                recovered: false,
            }),
            TypeExpressionTemplate::TypeValuedMemberProjection { .. } => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                c_compatible: false,
                flexible: false,
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
            TypeExpressionTemplate::FlexibleArray(element) => {
                let mut representation = self.check_template(element, origin)?;

                representation.flexible = true;
                representation.finite = false;
                representation.copyable = Copyability::Never;

                Ok(representation)
            }
            TypeExpressionTemplate::Slice(_) => Ok(MemberRepresentation::invalid(
                DiagnosticStoredTypeProblem::Slice,
            )),
            TypeExpressionTemplate::TraitView(_) => Ok(MemberRepresentation::invalid(
                DiagnosticStoredTypeProblem::TraitView,
            )),
            TypeExpressionTemplate::Borrow { kind, .. } => Ok(MemberRepresentation {
                finite: true,
                plain: false,
                c_compatible: false,
                flexible: false,
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
                c_compatible: false,
                flexible: false,
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
                c_compatible: false,
                flexible: false,
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
    ) -> RepresentationQueryResult<C, MemberRepresentation> {
        if named_representation_role(self.context.available_compiler_known_symbols(), subject)
            == Some(RepresentationRole::Uninit)
        {
            let [GenericParameterSymbolId::Type(_)] = parameters else {
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput.into());
            };

            let [argument] = arguments else {
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput.into());
            };

            let element = match argument {
                GenericArgumentTemplate::Resolved(GenericArgument::Type(ty)) => {
                    self.check_type(*ty, origin)?
                }
                GenericArgumentTemplate::Type(ty) => self.check_template(ty, origin)?,
                GenericArgumentTemplate::Resolved(GenericArgument::Constant(_))
                | GenericArgumentTemplate::Constant(_) => {
                    return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput.into());
                }
            };

            return Ok(uninit_representation(element));
        }

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
                GenericArgumentTemplate::Resolved(GenericArgument::Type(ty)) => {
                    Some(self.check_type(*ty, origin))
                }
                GenericArgumentTemplate::Resolved(GenericArgument::Constant(_))
                | GenericArgumentTemplate::Constant(_) => None,
                GenericArgumentTemplate::Type(ty) => Some(self.check_template(ty, origin)),
            })
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
    ) -> RepresentationQueryResult<C, MemberRepresentation> {
        let data = self.context.semantic_values().type_data(ty).map_err(|_| {
            CheckerQueryError::Infrastructure(CheckerInfrastructureError::SemanticValueUnavailable)
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
                    if role == RepresentationRole::Range {
                        let element = self
                            .context
                            .available_compiler_known_symbols()
                            .unary_representation_argument(
                                self.context.semantic_values(),
                                RepresentationRole::Range,
                                ty,
                            )
                            .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

                        self.validate_range_element_type(element, origin)?;
                    }

                    if role == RepresentationRole::Uninit {
                        let element = self
                            .context
                            .available_compiler_known_symbols()
                            .unary_representation_argument(
                                self.context.semantic_values(),
                                RepresentationRole::Uninit,
                                ty,
                            )
                            .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

                        return self.check_type(element, origin).map(uninit_representation);
                    }

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
                        CheckerQueryError::Infrastructure(
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
                c_compatible: false,
                flexible: false,
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
                c_compatible: false,
                flexible: false,
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
            TypeData::FlexibleArray(element) => {
                let mut representation = self.check_type(*element, origin)?;

                representation.flexible = true;
                representation.finite = false;
                representation.copyable = Copyability::Never;

                Ok(representation)
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
                c_compatible: false,
                flexible: false,
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
                c_compatible: false,
                flexible: false,
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
                c_compatible: false,
                flexible: false,
                copyable: Copyability::Always,
                copy_dependencies: BTreeSet::new(),
                non_copyable_members: BTreeSet::new(),
                stored_type_problems: BTreeSet::new(),
                recursive_cycles: Vec::new(),
                recovered: false,
            }),
        }
    }

    fn validate_range_element_type(
        &mut self,
        element: TypeId,
        origin: Option<SourceSpan>,
    ) -> RepresentationQueryResult<C, ()> {
        let role = crate::representation::type_representation_for_values(
            self.context.semantic_values(),
            self.context.available_compiler_known_symbols(),
            element,
        )?;

        if role
            .and_then(RepresentationRole::integer_representation)
            .is_some()
        {
            return Ok(());
        }

        let data = self
            .context
            .semantic_values()
            .type_data(element)
            .map_err(|_| {
                CheckerQueryError::Infrastructure(
                    CheckerInfrastructureError::SemanticValueUnavailable,
                )
            })?;

        let actual = match data.as_ref() {
            TypeData::TypeParameter(_) => return Ok(()),
            _ => role
                .and_then(crate::diagnostic::diagnostic_representation)
                .unwrap_or(bray_diagnostics::DiagnosticType::Unknown),
        };

        let Some(span) = origin else {
            return Ok(());
        };

        let id = u32::try_from(self.diagnostics.len()).unwrap_or(u32::MAX);

        self.diagnostics.add(
            Diagnostic::new(
                DiagnosticId::new(id),
                DiagnosticKind::CheckingRangeElementTypeMustBeInteger,
                SeverityKind::Error,
            )
            .with_primary_span(span)
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::InvalidRangeElementType,
                span,
            ))
            .with_arg(DiagnosticArg::actual_type(actual)),
        );

        Ok(())
    }

    fn add_limit_diagnostic(
        &mut self,
        kind: DiagnosticKind,
        span: SourceSpan,
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
        c_compatible: matches!(
            representation.layout(),
            bray_symbols::DeclaredLayoutMode::C | bray_symbols::DeclaredLayoutMode::Transparent
        ),
        flexible: false,
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
        c_compatible: matches!(
            representation.layout(),
            bray_symbols::DeclaredLayoutMode::C | bray_symbols::DeclaredLayoutMode::Transparent
        ),
        flexible: false,
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

fn uninit_representation(mut element: MemberRepresentation) -> MemberRepresentation {
    element.plain = false;
    element.copyable = Copyability::Never;
    element.copy_dependencies.clear();
    element.non_copyable_members.clear();

    element
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
        RepresentationRole::String
        | RepresentationRole::Range
        | RepresentationRole::RawPointer
        | RepresentationRole::DevicePointer => MemberRepresentation {
            finite: true,
            plain: false,
            c_compatible: false,
            flexible: false,
            copyable: Copyability::Always,
            copy_dependencies: BTreeSet::new(),
            non_copyable_members: BTreeSet::new(),
            stored_type_problems: BTreeSet::new(),
            recursive_cycles: Vec::new(),
            recovered: false,
        },
        RepresentationRole::Atomic
        | RepresentationRole::Future
        | RepresentationRole::Uninit
        | RepresentationRole::Task
        | RepresentationRole::PanicReport => MemberRepresentation {
            finite: true,
            plain: false,
            c_compatible: false,
            flexible: false,
            copyable: Copyability::Never,
            copy_dependencies: BTreeSet::new(),
            non_copyable_members: BTreeSet::new(),
            stored_type_problems: BTreeSet::new(),
            recursive_cycles: Vec::new(),
            recovered: false,
        },
        RepresentationRole::Result
        | RepresentationRole::RunResult
        | RepresentationRole::ConversionError => MemberRepresentation {
            finite: true,
            plain: false,
            c_compatible: false,
            flexible: false,
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use bray_diagnostics::DiagnosticStoredTypeProblem;

    use super::{Copyability, MemberRepresentation, uninit_representation};

    #[test]
    fn uninit_preserves_element_storage_constraints() {
        let result = uninit_representation(MemberRepresentation::invalid(
            DiagnosticStoredTypeProblem::Slice,
        ));

        assert!(!result.finite);
        assert!(!result.plain);
        assert_eq!(result.copyable, Copyability::Never);

        assert_eq!(
            result.stored_type_problems,
            BTreeSet::from([DiagnosticStoredTypeProblem::Slice])
        );
    }

    #[test]
    fn uninit_is_not_plain_or_copyable() {
        let result = uninit_representation(MemberRepresentation::SCALAR);

        assert!(result.finite);
        assert!(!result.plain);
        assert_eq!(result.copyable, Copyability::Never);
    }
}
