use bray_binder::BinderFactContext;
use bray_declarations::DeclarationId;
use bray_symbols::{
    AnySymbolId, InherentImplementationSymbol, MemberEntry, MemberValidity, MemberVisibility,
    SymbolKey, TypeAssociatedLifecycleSlot,
};

use super::query::NamedTypeRecord;
use crate::compilation::binder::CompilationBinderFacts;
use crate::fact::FactQueryError;

type LifecycleMemberInput = (AnySymbolId, TypeAssociatedLifecycleSlot);
type MemberInput = (AnySymbolId, Option<MemberEntry<AnySymbolId>>);
type CollectedMembers = (Vec<MemberInput>, Vec<LifecycleMemberInput>);

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum MemberOrderKey {
    Source(DeclarationId),
    Stable(SymbolKey),
}

pub(super) fn collect_named_type_members(
    facts: &CompilationBinderFacts<'_>,
    record: NamedTypeRecord<'_>,
) -> Result<CollectedMembers, FactQueryError> {
    let mut members = Vec::new();
    let mut lifecycle = Vec::new();

    macro_rules! collect {
        ($record:expr, $representation:ident) => {{
            let record = $record;

            members.extend(
                record
                    .$representation()
                    .iter()
                    .copied()
                    .map(AnySymbolId::from),
            );

            members.extend(
                record
                    .callable_members()
                    .iter()
                    .copied()
                    .map(AnySymbolId::from),
            );

            members.extend(record.constants().iter().copied().map(AnySymbolId::from));
            members.extend(record.predicates().iter().copied().map(AnySymbolId::from));
            members.extend(record.type_members().iter().copied().map(AnySymbolId::from));

            members.extend(
                record
                    .callable_overloads()
                    .iter()
                    .copied()
                    .map(AnySymbolId::from),
            );

            collect_constructors(facts, record.constructors(), &mut members, &mut lifecycle)?;

            collect_lifecycle(
                record.finalizers(),
                TypeAssociatedLifecycleSlot::Finalizer,
                &mut lifecycle,
            );

            collect_lifecycle(
                record.destructors(),
                TypeAssociatedLifecycleSlot::Destructor,
                &mut lifecycle,
            );

            collect_lifecycle(
                record.scope_enters(),
                TypeAssociatedLifecycleSlot::ScopeEnter,
                &mut lifecycle,
            );

            collect_lifecycle(
                record.scope_exits(),
                TypeAssociatedLifecycleSlot::ScopeExit,
                &mut lifecycle,
            );
        }};
    }

    match record {
        NamedTypeRecord::Struct(record) => collect!(record, fields),
        NamedTypeRecord::Union(record) => collect!(record, variants),
    }

    let members = sort_symbols(facts, members)?;
    let members = member_inputs(facts, members)?;
    let lifecycle = sort_lifecycle(facts, lifecycle)?;

    Ok((members, lifecycle))
}

pub(super) fn collect_implementation_members(
    facts: &CompilationBinderFacts<'_>,
    record: &InherentImplementationSymbol,
) -> Result<CollectedMembers, FactQueryError> {
    let mut members = Vec::new();
    let mut lifecycle = Vec::new();

    members.extend(
        record
            .callable_members()
            .iter()
            .copied()
            .map(AnySymbolId::from),
    );

    members.extend(record.constants().iter().copied().map(AnySymbolId::from));
    members.extend(record.predicates().iter().copied().map(AnySymbolId::from));
    members.extend(record.type_members().iter().copied().map(AnySymbolId::from));

    members.extend(
        record
            .callable_overloads()
            .iter()
            .copied()
            .map(AnySymbolId::from),
    );

    collect_constructors(facts, record.constructors(), &mut members, &mut lifecycle)?;

    collect_lifecycle(
        record.finalizers(),
        TypeAssociatedLifecycleSlot::Finalizer,
        &mut lifecycle,
    );

    collect_lifecycle(
        record.destructors(),
        TypeAssociatedLifecycleSlot::Destructor,
        &mut lifecycle,
    );

    collect_lifecycle(
        record.scope_enters(),
        TypeAssociatedLifecycleSlot::ScopeEnter,
        &mut lifecycle,
    );

    collect_lifecycle(
        record.scope_exits(),
        TypeAssociatedLifecycleSlot::ScopeExit,
        &mut lifecycle,
    );

    Ok((
        member_inputs(facts, sort_symbols(facts, members)?)?,
        sort_lifecycle(facts, lifecycle)?,
    ))
}

fn collect_constructors(
    facts: &CompilationBinderFacts<'_>,
    constructors: &[bray_symbols::ConstructorSymbolId],
    members: &mut Vec<AnySymbolId>,
    lifecycle: &mut Vec<LifecycleMemberInput>,
) -> Result<(), FactQueryError> {
    for constructor in constructors {
        let id = AnySymbolId::from(*constructor);

        members.push(id);

        if member_entry(facts, id)?.is_none() {
            lifecycle.push((id, TypeAssociatedLifecycleSlot::PrimaryConstructor));
        }
    }

    Ok(())
}

fn collect_lifecycle<I>(
    declarations: &[I],
    slot: TypeAssociatedLifecycleSlot,
    lifecycle: &mut Vec<LifecycleMemberInput>,
) where
    I: Copy + Into<AnySymbolId>,
{
    lifecycle.extend(
        declarations
            .iter()
            .copied()
            .map(Into::into)
            .map(|id| (id, slot)),
    );
}

fn member_inputs(
    facts: &CompilationBinderFacts<'_>,
    members: Vec<AnySymbolId>,
) -> Result<Vec<MemberInput>, FactQueryError> {
    members
        .into_iter()
        .map(|member| Ok((member, member_entry(facts, member)?)))
        .collect()
}

pub(super) fn member_entry(
    facts: &CompilationBinderFacts<'_>,
    member: AnySymbolId,
) -> Result<Option<MemberEntry<AnySymbolId>>, FactQueryError> {
    if let Some(entry) = facts.symbols().member_entry(member) {
        // The surface owns lookup metadata independently of the source symbol graph.
        return Ok(Some(entry.clone()));
    }

    if facts.symbols().symbol_key(member).is_some() {
        return Ok(None);
    }

    let imported = facts
        .imported_symbols()
        .map_err(crate::compilation::binder::binder_fact_error)?;

    let Some(imported) = imported else {
        return Ok(None);
    };

    let Some(name) = imported.member_name(member) else {
        return Ok(None);
    };

    // Compiled interfaces contain only validated public lookup surfaces.
    // The surface owns the Arc-backed imported name beyond the skeleton borrow.
    Ok(Some(MemberEntry::new(
        member,
        name.clone(),
        MemberVisibility::Public,
        MemberValidity::Valid,
    )))
}

pub(super) fn sort_symbols<I>(
    facts: &CompilationBinderFacts<'_>,
    symbols: Vec<I>,
) -> Result<Vec<I>, FactQueryError>
where
    I: Copy + Into<AnySymbolId>,
{
    sort_symbols_by(facts, symbols, |key| match key.source_declaration_id() {
        Some(declaration) => MemberOrderKey::Source(declaration),
        // Sorting owns stable Arc-backed keys beyond provider borrows.
        None => MemberOrderKey::Stable(key.clone()),
    })
}

pub(super) fn sort_symbols_by_key<I>(
    facts: &CompilationBinderFacts<'_>,
    symbols: Vec<I>,
) -> Result<Vec<I>, FactQueryError>
where
    I: Copy + Into<AnySymbolId>,
{
    sort_symbols_by(facts, symbols, Clone::clone)
}

fn sort_symbols_by<I, K>(
    facts: &CompilationBinderFacts<'_>,
    symbols: Vec<I>,
    order: impl Fn(&SymbolKey) -> K,
) -> Result<Vec<I>, FactQueryError>
where
    I: Copy + Into<AnySymbolId>,
    K: Ord,
{
    let imported = facts
        .imported_symbols()
        .map_err(crate::compilation::binder::binder_fact_error)?;

    let mut keyed = symbols
        .into_iter()
        .map(|symbol| {
            let erased = symbol.into();

            let key = facts
                .symbols()
                .symbol_key(erased)
                .or_else(|| imported.and_then(|symbols| symbols.symbol_key(erased)))
                .ok_or(FactQueryError::InfrastructureFailure)?;

            Ok((order(key), symbol))
        })
        .collect::<Result<Vec<_>, _>>()?;

    keyed.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));

    Ok(keyed.into_iter().map(|(_, symbol)| symbol).collect())
}

fn sort_lifecycle(
    facts: &CompilationBinderFacts<'_>,
    lifecycle: Vec<LifecycleMemberInput>,
) -> Result<Vec<LifecycleMemberInput>, FactQueryError> {
    let ids = lifecycle.iter().map(|(id, _)| *id).collect::<Vec<_>>();

    let order = sort_symbols(facts, ids)?
        .into_iter()
        .enumerate()
        .map(|(ordinal, id)| (id, ordinal))
        .collect::<std::collections::BTreeMap<_, _>>();

    let mut lifecycle = lifecycle;

    lifecycle.sort_unstable_by_key(|(id, _)| order[id]);

    Ok(lifecycle)
}
