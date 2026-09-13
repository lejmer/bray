use crate::{CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext};
use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundStructuredExpressionKind,
};
use bray_symbols::{
    AnyLocalSymbolId, AnySymbolId, DependencyProjection, DependencyRequirement,
    DependencyRequirementKind, DependencySubject, DependencySubjectRoot, SymbolOrdinal,
};
use std::collections::BTreeSet;

use super::core::{ResultInference, extend};
use crate::dependency::projection::{binding_projection, pattern_projection};

impl<C: CheckerRequestContext + ?Sized> ResultInference<'_, C> {
    pub(super) fn borrow_requirement_kind(
        &self,
        mut expression: BoundExpressionId,
    ) -> Result<DependencyRequirementKind, CheckerInfrastructureError> {
        loop {
            let receiver = match self.request.view().expression(expression) {
                Some(BoundExpression::MemberAccess(member)) => Some(member.receiver()),
                Some(BoundExpression::TraitQualifiedMember(member)) => Some(member.receiver()),
                Some(BoundExpression::Structured(value))
                    if matches!(
                        value.kind(),
                        BoundStructuredExpressionKind::ElementIndex
                            | BoundStructuredExpressionKind::SliceIndex
                            | BoundStructuredExpressionKind::NullablePropagation
                    ) =>
                {
                    value.operands().first().copied()
                }
                _ => None,
            };

            let Some(receiver) = receiver else {
                return Ok(DependencyRequirementKind::StorageAlive);
            };

            let ty = self
                .types
                .expression(receiver)
                .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

            let ty = self
                .request
                .semantic_values()
                .type_data(ty.ty())
                .map_err(CheckerInfrastructureError::SemanticValueStore)?;

            if matches!(ty.as_ref(), bray_symbols::TypeData::Borrow { .. }) {
                // Reborrowing a reached value retains the existing borrow, not the slot storing it.
                return Ok(DependencyRequirementKind::ValueDependencies);
            }

            expression = receiver;
        }
    }

    pub(super) fn bind_pattern(
        &mut self,
        root: bray_bound_tree::BoundPatternId,
        expression: BoundExpressionId,
        values: BTreeSet<DependencyRequirement>,
    ) -> Result<bool, CheckerQueryError<C::UpstreamError>> {
        let mut changed = false;

        let mut pending = vec![(
            root,
            self.sources.get(&expression).cloned().unwrap_or_default(),
            Vec::new(),
        )];

        while let Some((id, mut sources, mut path)) = pending.pop() {
            let pattern = self
                .request
                .unit()
                .tree()
                .pattern(id)
                .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

            let checked = self
                .patterns
                .pattern(id)
                .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

            if let Some(projection) = checked.projection().and_then(pattern_projection) {
                sources = project_subjects(&sources, projection);
                path.push(projection);
            }

            for binding in checked.bindings(pattern) {
                let binding_type = self
                    .patterns
                    .binding_type(binding)
                    .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

                let projection =
                    binding_projection(pattern, binding_type).and_then(pattern_projection);

                let binding_path = path.iter().copied().chain(projection).collect::<Vec<_>>();

                let projected_values = (!binding_path.is_empty())
                    .then(|| self.projected_values(expression, &binding_path));

                let binding_values = projected_values.as_ref().unwrap_or(&values);

                // Each binding owns its inferred requirements while sibling projections advance.
                let binding_sources = match projection {
                    Some(projection) => project_subjects(&sources, projection),
                    None => sources.clone(),
                };

                changed |= extend(
                    self.local_sources.entry(binding).or_default(),
                    binding_sources.iter().cloned(),
                );

                let requirements = binding_sources.into_iter().map(|subject| {
                    DependencyRequirement::direct(
                        subject,
                        DependencyRequirementKind::ValueDependencies,
                    )
                });

                changed |= extend(
                    self.locals.entry(binding).or_default(),
                    requirements.chain(binding_values.iter().cloned()),
                );
            }

            // Sibling patterns independently retain the parent source set and projection.
            pending.extend(
                pattern
                    .children()
                    .iter()
                    .map(|child| (*child, sources.clone(), path.clone())),
            );
        }

        Ok(changed)
    }

    pub(super) fn expression_sources(
        &self,
        id: BoundExpressionId,
        expression: &BoundExpression,
    ) -> Result<BTreeSet<DependencySubject>, CheckerQueryError<C::UpstreamError>> {
        Ok(match expression {
            BoundExpression::Name(name) => match name.target() {
                BoundReferenceTarget::Surface(AnySymbolId::CallableParameter(parameter)) => self
                    .request
                    .symbols()
                    .callable_parameter(parameter)
                    .map(|parameter| {
                        DependencySubject::root(DependencySubjectRoot::Parameter(
                            SymbolOrdinal::new(parameter.ordinal()),
                        ))
                    })
                    .into_iter()
                    .collect(),
                BoundReferenceTarget::Surface(AnySymbolId::ReceiverParameter(_)) => {
                    BTreeSet::from([DependencySubject::root(DependencySubjectRoot::Receiver)])
                }
                BoundReferenceTarget::Local(AnyLocalSymbolId::Binding(binding)) => self
                    .local_sources
                    .get(&binding)
                    .into_iter()
                    .flatten()
                    .cloned()
                    .collect(),
                _ => BTreeSet::new(),
            },
            BoundExpression::MemberAccess(member) => self.project_sources(member.receiver(), id),
            BoundExpression::Call(_) => {
                let borrowed = self
                    .types
                    .expression(id)
                    .map(|ty| self.request.semantic_values().type_data(ty.ty()))
                    .transpose()
                    .map_err(CheckerInfrastructureError::SemanticValueStore)?;

                if borrowed.is_some_and(|data| {
                    matches!(data.as_ref(), bray_symbols::TypeData::Borrow { .. })
                }) {
                    self.values
                        .get(&id)
                        .into_iter()
                        .flatten()
                        .filter_map(requirement_subject)
                        .cloned()
                        .collect()
                } else {
                    BTreeSet::new()
                }
            }
            BoundExpression::PatternReference(reference) => self
                .local_sources
                .get(&reference.binding())
                .into_iter()
                .flatten()
                .cloned()
                .collect(),
            BoundExpression::TraitQualifiedMember(member) => {
                self.project_sources(member.receiver(), id)
            }
            BoundExpression::Structured(value)
                if matches!(
                    value.kind(),
                    BoundStructuredExpressionKind::Borrow
                        | BoundStructuredExpressionKind::ElementIndex
                        | BoundStructuredExpressionKind::SliceIndex
                        | BoundStructuredExpressionKind::NullablePropagation
                ) =>
            {
                value
                    .operands()
                    .first()
                    .and_then(|operand| self.sources.get(operand))
                    .cloned()
                    .unwrap_or_default()
            }
            _ => BTreeSet::new(),
        })
    }

    fn project_sources(
        &self,
        receiver: BoundExpressionId,
        expression: BoundExpressionId,
    ) -> BTreeSet<DependencySubject> {
        let projection = crate::dependency::assignment::member_projection(
            self.request.unit(),
            self.selections,
            expression,
        );

        if let Some(projection) = projection {
            let path = [projection];

            let (value, remaining) = self.inputs.project(receiver, &path);

            if remaining.is_empty() {
                return self
                    .values
                    .get(&value)
                    .into_iter()
                    .flatten()
                    .filter_map(requirement_subject)
                    .cloned()
                    .collect();
            }
        }

        self.sources
            .get(&receiver)
            .into_iter()
            .flatten()
            .map(|source| {
                normalized_subject(
                    source.subject_root(),
                    source.projections().iter().copied().chain(projection),
                )
            })
            .collect()
    }

    pub(super) fn projected_values(
        &self,
        value: BoundExpressionId,
        path: &[bray_symbols::DependencyProjection],
    ) -> BTreeSet<DependencyRequirement> {
        let mut requirements = BTreeSet::new();
        let mut pending = vec![(value, path.to_vec())];
        let mut visited = BTreeSet::new();

        while let Some((value, path)) = pending.pop() {
            let path = normalized_subject(DependencySubjectRoot::Result, path);
            let path = path.projections();

            if !visited.insert((value, path.to_vec())) {
                continue;
            }

            if !path.is_empty() {
                pending.extend(
                    self.inputs
                        .writes
                        .get(&value)
                        .into_iter()
                        .flatten()
                        .filter_map(|write| write.project(path)),
                );
            }

            let (value, path) = self.inputs.project(value, path);

            let sources = self.sources.get(&value);

            if path.is_empty() || sources.is_none_or(BTreeSet::is_empty) {
                requirements.extend(self.values.get(&value).into_iter().flatten().cloned());
                continue;
            }

            requirements.extend(sources.into_iter().flatten().map(|source| {
                DependencyRequirement::direct(
                    normalized_subject(
                        source.subject_root(),
                        source.projections().iter().chain(path).copied(),
                    ),
                    DependencyRequirementKind::ValueDependencies,
                )
            }));
        }

        requirements
    }
}

fn requirement_subject(requirement: &DependencyRequirement) -> Option<&DependencySubject> {
    match requirement {
        DependencyRequirement::Direct { subject, .. } => Some(subject),
        DependencyRequirement::Guarded(_) => None,
    }
}

fn project_subjects(
    sources: &BTreeSet<DependencySubject>,
    projection: DependencyProjection,
) -> BTreeSet<DependencySubject> {
    sources
        .iter()
        .map(|source| {
            normalized_subject(
                source.subject_root(),
                source.projections().iter().copied().chain([projection]),
            )
        })
        .collect()
}

pub(super) fn normalized_subject(
    root: bray_symbols::DependencySubjectRoot,
    projections: impl IntoIterator<Item = DependencyProjection>,
) -> DependencySubject {
    let mut path = Vec::new();

    for projection in projections {
        // A recursive field traversal retains the first repeated field's complete value contract.
        // This includes its nested dependencies and gives inference a finite set of storage paths.
        if matches!(
            projection,
            DependencyProjection::ProductField(_) | DependencyProjection::UnionPayloadField(_)
        ) && let Some(index) = path.iter().position(|existing| *existing == projection)
        {
            path.truncate(index + 1);
            break;
        }

        path.push(projection);
    }

    DependencySubject::new(root, path)
}
