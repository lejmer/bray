use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    BoundControlTransferKind, BoundExpression, BoundExpressionId, BoundReferenceTarget,
    BoundStructuredExpressionKind, SemanticSelection,
};
use bray_symbols::{
    AnySymbolId, DependencyBorrowRelation, DependencyProjection, DependencyReferenceKind,
    DependencySubject, DependencySubjectRoot, TypeData,
};

use super::core::ResultInference;
use crate::{CheckerQueryError, CheckerRequestContext};

struct Exposure {
    root: BoundReferenceTarget,
    path: Vec<Option<DependencyProjection>>,
    includes_slot: bool,
}

impl<C: CheckerRequestContext + ?Sized> ResultInference<'_, C> {
    pub(super) fn borrow_relations(
        &self,
    ) -> Result<Vec<DependencyBorrowRelation>, CheckerQueryError<C::UpstreamError>>
    where
        C: crate::CheckerSemanticQueryProvider<bray_symbols::CallableSignatureQuery>,
    {
        // Captures are checked in their own body and have no exact carrier summary here.
        if self
            .request
            .unit()
            .tree()
            .expressions()
            .any(|(_, node)| matches!(node, BoundExpression::AnonymousCallable(_)))
        {
            return Ok(Vec::new());
        }

        let exposures = self.borrow_exposures();

        let mut returns = self
            .request
            .unit()
            .tree()
            .expressions()
            .filter_map(|(_, node)| {
                let BoundExpression::ControlTransfer(transfer) = node else {
                    return None;
                };

                (transfer.kind() == BoundControlTransferKind::Return)
                    .then(|| transfer.operand())
                    .flatten()
            })
            .collect::<Vec<_>>();

        if let bray_bound_tree::BoundUnitRoot::Expression(expression) = self.request.unit().root() {
            returns.push(expression);
        }

        let mut result: Option<
            BTreeMap<Vec<DependencyProjection>, (DependencySubject, DependencyReferenceKind)>,
        > = None;

        for expression in returns {
            // An absent nullable result has no borrowed slot whose identity could disagree.
            if matches!(self.request.view().expression(expression), Some(BoundExpression::Structured(value)) if value.kind() == BoundStructuredExpressionKind::Absence)
            {
                continue;
            }

            let Some(target) = self.types.callable_result_type() else {
                continue;
            };

            let Some(prefix) = self.borrow_conversion(expression, target) else {
                return Ok(Vec::new());
            };

            let mut paths = Vec::new();

            self.borrow_paths(expression, &mut paths, &mut BTreeSet::new())?;

            let mut identities = BTreeMap::new();

            for (path, kind) in paths {
                if let Some(origin) =
                    self.borrow_origin(expression, &path, &exposures, &mut BTreeSet::new())?
                {
                    identities.insert(prefix.iter().copied().chain(path).collect(), (origin, kind));
                }
            }

            match &mut result {
                Some(result) => result.retain(|path, origin| identities.get(path) == Some(origin)),
                None => result = Some(identities),
            }
        }

        // Implicit error returns participate in the same must-identity intersection.
        if self.inputs.returned_errors().next().is_some() {
            result = None;
        }

        let mut relations = result
            .into_iter()
            .flatten()
            .map(|(path, (origin, kind))| {
                DependencyBorrowRelation::new(
                    origin,
                    DependencySubject::new(DependencySubjectRoot::Result, path),
                    kind,
                )
            })
            .collect::<Vec<_>>();

        for (expression, _) in self.request.unit().tree().expressions() {
            let Some(kind) = self.reference_kind(expression) else {
                continue;
            };

            let Some(subject) = self.formal_place(expression) else {
                continue;
            };

            if subject.projections().is_empty()
                || !self.borrow_slot_unchanged(expression, &exposures)
                || !self.borrow_slot_survives_cleanup(expression)?
            {
                continue;
            }

            // Both endpoints share one immutable path. This preserves the stored borrow,
            // never the state of its referent or the callable's acquisition predicates.
            relations.push(DependencyBorrowRelation::new(
                subject.clone(),
                subject,
                kind,
            ));
        }

        Ok(relations)
    }

    fn reference_kind(&self, expression: BoundExpressionId) -> Option<DependencyReferenceKind> {
        let ty = self.types.expression(expression)?.ty();

        if matches!(
            self.request.semantic_values().type_data(ty).as_ref(),
            TypeData::Borrow { .. }
        ) {
            Some(DependencyReferenceKind::Borrow)
        } else if crate::representation::type_representation(self.request, ty)
            == Some(bray_compiler_known::RepresentationRole::RawPointer)
        {
            Some(DependencyReferenceKind::Address)
        } else {
            None
        }
    }

    fn borrow_paths(
        &self,
        expression: BoundExpressionId,
        paths: &mut Vec<(Vec<DependencyProjection>, DependencyReferenceKind)>,
        visited: &mut BTreeSet<BoundExpressionId>,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        if !visited.insert(expression) {
            return Ok(());
        }

        if let Some(kind) = self.reference_kind(expression) {
            paths.push((Vec::new(), kind));
        } else if let Some(BoundExpression::Structured(value)) =
            self.request.view().expression(expression)
            && value.kind() == BoundStructuredExpressionKind::TrustBoundary
            && let Some(operand) = value.operands().first()
        {
            self.borrow_paths(*operand, paths, visited)?;
        } else if let Some(initializer) = self.inputs.initializers.get(&expression) {
            if let Some(target) = self.types.expression(expression)
                && let Some(prefix) = self.borrow_conversion(*initializer, target.ty())
            {
                let mut children = Vec::new();

                self.borrow_paths(*initializer, &mut children, visited)?;

                paths.extend(
                    children
                        .into_iter()
                        .map(|(child, kind)| (prefix.iter().copied().chain(child).collect(), kind)),
                );
            }
        } else if let Some(SemanticSelection::Call(call)) = self.selections.expression(expression) {
            let template = self.selected_result_template(call)?;

            paths.extend(
                template
                    .template
                    .borrows()
                    .iter()
                    .filter(|relation| {
                        relation.exit().subject_root() == DependencySubjectRoot::Result
                    })
                    .map(|relation| (relation.exit().projections().to_vec(), relation.kind())),
            );
        } else {
            for ((owner, projection), value) in &self.inputs.projections {
                if *owner != expression {
                    continue;
                }

                let Some(target) = self.borrow_component_type(expression, *projection)? else {
                    continue;
                };

                let Some(prefix) = self.borrow_conversion(*value, target) else {
                    continue;
                };

                let mut children = Vec::new();

                self.borrow_paths(*value, &mut children, visited)?;

                paths.extend(children.into_iter().map(|(child, kind)| {
                    (
                        std::iter::once(*projection)
                            .chain(prefix.iter().copied())
                            .chain(child)
                            .collect(),
                        kind,
                    )
                }));
            }
        }

        visited.remove(&expression);

        Ok(())
    }

    fn borrow_origin(
        &self,
        expression: BoundExpressionId,
        path: &[DependencyProjection],
        exposures: &[Exposure],
        visited: &mut BTreeSet<(BoundExpressionId, Vec<DependencyProjection>)>,
    ) -> Result<Option<DependencySubject>, CheckerQueryError<C::UpstreamError>> {
        let key = (expression, path.to_vec());

        if !visited.insert(key.clone()) {
            return Ok(None);
        }

        let result = self.borrow_origin_inner(expression, path, exposures, visited);

        visited.remove(&key);

        result
    }

    fn borrow_origin_inner(
        &self,
        expression: BoundExpressionId,
        path: &[DependencyProjection],
        exposures: &[Exposure],
        visited: &mut BTreeSet<(BoundExpressionId, Vec<DependencyProjection>)>,
    ) -> Result<Option<DependencySubject>, CheckerQueryError<C::UpstreamError>> {
        if self
            .inputs
            .writes
            .values(expression)
            .any(|write| write.matches(path))
        {
            return Ok(None);
        }

        if let Some(initializer) = self.inputs.initializers.get(&expression) {
            if !self.borrow_slot_unchanged(expression, &exposures) {
                return Ok(None);
            }

            let Some(target) = self.types.expression(expression) else {
                return Ok(None);
            };

            let Some(prefix) = self.borrow_conversion(*initializer, target.ty()) else {
                return Ok(None);
            };

            let Some(path) = path.strip_prefix(prefix.as_slice()) else {
                return Ok(None);
            };

            return self.borrow_origin(*initializer, path, exposures, visited);
        }

        if let Some((projection, rest)) = path.split_first()
            && let Some(value) = self.inputs.projections.get(&(expression, *projection))
        {
            let Some(target) = self.borrow_component_type(expression, *projection)? else {
                return Ok(None);
            };

            let Some(prefix) = self.borrow_conversion(*value, target) else {
                return Ok(None);
            };

            let Some(rest) = rest.strip_prefix(prefix.as_slice()) else {
                return Ok(None);
            };

            return self.borrow_origin(*value, rest, exposures, visited);
        }

        if let Some(SemanticSelection::Call(call)) = self.selections.expression(expression) {
            let template = self.selected_result_template(call)?;

            for relation in template.template.borrows() {
                if relation.exit().subject_root() == DependencySubjectRoot::Result
                    && path.starts_with(relation.exit().projections())
                    && let Some(argument) =
                        crate::dependency::result_argument(call, relation.entry().subject_root())
                {
                    let composed = relation
                        .entry()
                        .projections()
                        .iter()
                        .copied()
                        .chain(path[relation.exit().projections().len()..].iter().copied())
                        .collect::<Vec<_>>();

                    let mut path = composed.as_slice();

                    if let DependencySubjectRoot::Parameter(ordinal) =
                        relation.entry().subject_root()
                    {
                        let Some(bray_bound_tree::SelectedArgument::Explicit { conversion, .. }) = call.arguments().iter().find(|argument| matches!(argument, bray_bound_tree::SelectedArgument::Explicit { ordinal: candidate, .. } if *candidate == ordinal.raw())) else { continue; };

                        let Some(prefix) =
                            self.borrow_conversion(argument, conversion.target_type())
                        else {
                            continue;
                        };

                        let Some(rest) = path.strip_prefix(prefix.as_slice()) else {
                            continue;
                        };

                        path = rest;
                    }

                    return self.borrow_origin(argument, path, exposures, visited);
                }
            }

            return Ok(None);
        }

        if let Some(BoundExpression::Structured(value)) = self.request.view().expression(expression)
            && matches!(
                value.kind(),
                BoundStructuredExpressionKind::Borrow
                    | BoundStructuredExpressionKind::TrustBoundary
            )
            && let Some(operand) = value.operands().first()
        {
            return self.borrow_origin(*operand, path, exposures, visited);
        }

        if let Some(receiver) = match self.request.view().expression(expression) {
            Some(BoundExpression::MemberAccess(member)) => Some(member.receiver()),
            Some(BoundExpression::TraitQualifiedMember(member)) => Some(member.receiver()),
            _ => None,
        } {
            if self.reference_kind(expression).is_some()
                && !self.borrow_slot_unchanged(expression, exposures)
            {
                return Ok(None);
            }

            if let Some(projection) = crate::dependency::assignment::member_projection(
                self.request.unit(),
                self.selections,
                expression,
            ) {
                let projected = std::iter::once(projection)
                    .chain(path.iter().copied())
                    .collect::<Vec<_>>();

                return self.borrow_origin(receiver, &projected, exposures, visited);
            }
        }

        if let Some(subject) = self.formal_place(expression) {
            if self.reference_kind(expression).is_some()
                && !self.borrow_slot_unchanged(expression, &exposures)
            {
                return Ok(None);
            }

            return Ok(Some(DependencySubject::new(
                subject.subject_root(),
                subject
                    .projections()
                    .iter()
                    .copied()
                    .chain(path.iter().copied()),
            )));
        }

        Ok(None)
    }

    // Only identity and nullable wrapping preserve these exact storage paths. Other
    // conversions need their selected callable's proof rather than a shape assumption.
    fn borrow_conversion(
        &self,
        expression: BoundExpressionId,
        mut target: bray_symbols::TypeId,
    ) -> Option<Vec<DependencyProjection>> {
        let source = self.types.expression(expression)?.ty();
        let mut prefix = Vec::new();

        while target != source {
            let data = self.request.semantic_values().type_data(target);

            let TypeData::Nullable(inner) = data.as_ref() else {
                return None;
            };

            prefix.push(DependencyProjection::NullableValue);
            target = *inner;
        }

        Some(prefix)
    }

    fn borrow_component_type(
        &self,
        expression: BoundExpressionId,
        projection: DependencyProjection,
    ) -> Result<Option<bray_symbols::TypeId>, CheckerQueryError<C::UpstreamError>> {
        let Some(owner) = self.types.expression(expression) else {
            return Ok(None);
        };

        let ty = crate::storage::projected_value_type(self.request, owner.ty(), projection)?;

        Ok(*ty.value())
    }

    fn formal_place(&self, expression: BoundExpressionId) -> Option<DependencySubject> {
        match self.request.view().expression(expression)? {
            BoundExpression::Name(name) => match name.target() {
                BoundReferenceTarget::Surface(AnySymbolId::ReceiverParameter(_)) => {
                    Some(DependencySubject::root(DependencySubjectRoot::Receiver))
                }
                BoundReferenceTarget::Surface(AnySymbolId::CallableParameter(parameter)) => {
                    Some(DependencySubject::root(DependencySubjectRoot::Parameter(
                        bray_symbols::SymbolOrdinal::new(
                            self.request
                                .symbols()
                                .callable_parameter(parameter)?
                                .ordinal(),
                        ),
                    )))
                }
                _ => None,
            },
            BoundExpression::MemberAccess(member) => {
                self.project_formal(member.receiver(), expression)
            }
            BoundExpression::TraitQualifiedMember(member) => {
                self.project_formal(member.receiver(), expression)
            }
            _ => None,
        }
    }

    fn project_formal(
        &self,
        receiver: BoundExpressionId,
        expression: BoundExpressionId,
    ) -> Option<DependencySubject> {
        let subject = self.formal_place(receiver)?;

        let projection = crate::dependency::assignment::member_projection(
            self.request.unit(),
            self.selections,
            expression,
        )?;

        Some(DependencySubject::new(
            subject.subject_root(),
            subject.projections().iter().copied().chain([projection]),
        ))
    }

    fn borrow_slot_survives_cleanup(
        &self,
        expression: BoundExpressionId,
    ) -> Result<bool, CheckerQueryError<C::UpstreamError>>
    where
        C: crate::CheckerSemanticQueryProvider<bray_symbols::CallableSignatureQuery>,
    {
        let mut ancestors = Vec::new();
        let mut root = expression;

        loop {
            let receiver = match self.request.view().expression(root) {
                Some(BoundExpression::MemberAccess(member)) => member.receiver(),
                Some(BoundExpression::TraitQualifiedMember(member)) => member.receiver(),
                _ => break,
            };

            ancestors.push(receiver);
            root = receiver;
        }

        let mut destructor_receiver = false;

        if let Some(BoundExpression::Name(name)) = self.request.view().expression(root)
            && let BoundReferenceTarget::Surface(AnySymbolId::ReceiverParameter(parameter)) =
                name.target()
        {
            let receiver = self
                .request
                .symbols()
                .receiver_parameter(parameter)
                .expect("formal receiver retains its symbol");

            let signature =
                self.request
                    .resolve_symbol_query(bray_symbols::SymbolQueryRequest::<
                        bray_symbols::CallableSignatureQuery,
                    >::new(receiver.owner()))?;

            let mode = signature
                .value()
                .receiver()
                .expect("receiver symbol belongs to a method")
                .mode();

            if matches!(
                mode,
                bray_symbols::ReceiverMode::Shared | bray_symbols::ReceiverMode::Mutable
            ) {
                return Ok(true);
            }

            destructor_receiver = self.request.unit().key().declared_owner().kind()
                == bray_symbols::SymbolKind::Destructor;
        }

        // The current destructor already checks its receiver body. Other owned carriers
        // may execute custom cleanup after the explicit body and replace the stored borrow.
        for ancestor in ancestors.into_iter().rev() {
            if destructor_receiver && ancestor == root {
                continue;
            }

            let Some(ty) = self.types.expression(ancestor) else {
                return Ok(false);
            };

            match self.request.semantic_values().type_data(ty.ty()).as_ref() {
                TypeData::Borrow { .. } => return Ok(true),
                TypeData::Named { definition, .. } => {
                    let lifecycle = self.request.declared_type_has_lifecycle(*definition)?;

                    if *lifecycle.value() || lifecycle.diagnostics().has_errors() {
                        return Ok(false);
                    }
                }
                TypeData::TypeParameter(_) => return Ok(false),
                _ => {}
            }
        }

        Ok(true)
    }

    fn borrow_slot_unchanged(&self, expression: BoundExpressionId, exposures: &[Exposure]) -> bool {
        let places = crate::dependency::assignment::value_places(
            self.request.unit(),
            self.selections,
            &self.inputs,
            expression,
        );

        !places.is_empty()
            && self.inputs.writes.values(expression).next().is_none()
            && !exposures.iter().any(|exposure| {
                places.iter().any(|(root, path)| {
                    exposure.root == *root
                        && (exposure.path.len() < path.len()
                            || exposure.includes_slot && exposure.path.len() == path.len())
                        && exposure
                            .path
                            .iter()
                            .zip(path)
                            .all(|(left, right)| left.is_none() || right.is_none() || left == right)
                })
            })
    }

    fn borrow_exposures(&self) -> Vec<Exposure> {
        let mut uses = BTreeMap::<BoundExpressionId, bool>::new();

        for (parent_id, parent) in self.request.unit().tree().expressions() {
            for child in parent.child_expressions() {
                let projection = matches!(parent, BoundExpression::MemberAccess(member) if member.receiver() == child)
                    || matches!(parent, BoundExpression::TraitQualifiedMember(member) if member.receiver() == child);

                // Structural construction transfers values into fresh storage. Explicit borrows
                // within its children still expose the corresponding source places below.
                let construction = matches!(
                    self.selections.expression(parent_id),
                    Some(SemanticSelection::Operation(
                        bray_bound_tree::SelectedOperation::Construction(_)
                    ))
                ) || matches!(parent, BoundExpression::Structured(value) if value.kind() == BoundStructuredExpressionKind::Tuple);

                if projection
                    || construction
                    || matches!(parent, BoundExpression::ControlTransfer(transfer) if transfer.kind() == BoundControlTransferKind::Return)
                {
                    continue;
                }

                let mut includes_slot = self.reference_kind(child).is_none()
                    || matches!(parent, BoundExpression::Structured(value) if value.kind() == BoundStructuredExpressionKind::Borrow)
                    || matches!(parent, BoundExpression::Assignment(_));

                if let Some(SemanticSelection::Call(call)) = self.selections.expression(parent_id) {
                    includes_slot |= call.receiver().is_some_and(|receiver| {
                        receiver.expression() == child
                            && receiver.mode() == bray_symbols::ReceiverMode::Mutable
                    });

                    includes_slot |= call.arguments().iter().any(|argument| matches!(argument, bray_bound_tree::SelectedArgument::Explicit { expression, conversion, .. } if *expression == child && self.types.expression(child).is_none_or(|ty| ty.ty() != conversion.target_type())));
                }

                uses.entry(child)
                    .and_modify(|existing| *existing |= includes_slot)
                    .or_insert(includes_slot);
            }
        }

        uses.into_iter()
            .flat_map(|(expression, includes_slot)| {
                crate::dependency::assignment::value_places(
                    self.request.unit(),
                    self.selections,
                    &self.inputs,
                    expression,
                )
                .into_iter()
                .map(move |(root, path)| Exposure {
                    root,
                    path,
                    includes_slot,
                })
            })
            .collect()
    }
}
