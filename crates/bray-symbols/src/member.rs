use std::collections::{BTreeMap, BTreeSet};

use crate::SymbolName;

/// The declaration-level visibility of an ordinary member.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MemberVisibility {
    /// Reachable wherever the owning declaration surface is reachable.
    Public,
    /// Reachable only from contexts granted internal access.
    Internal,
}

impl MemberVisibility {
    /// Returns this visibility's stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
        }
    }

    /// Returns whether this member is public at its declaration boundary.
    pub const fn is_public(self) -> bool {
        matches!(self, Self::Public)
    }
}

/// Whether a member has a usable declaration surface for ordinary lookup.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MemberValidity {
    /// The member has a well-formed lookup surface.
    Valid,
    /// The member surface contains recovery or another symbol-owned error.
    Malformed,
}

impl MemberValidity {
    /// Returns whether lookup must preserve this member as malformed.
    pub const fn is_malformed(self) -> bool {
        matches!(self, Self::Malformed)
    }
}

/// One typed member participating in an owner's ordinary-name surface.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MemberEntry<I> {
    id: I,
    name: SymbolName,
    visibility: MemberVisibility,
    validity: MemberValidity,
}

impl<I> MemberEntry<I> {
    /// Creates a typed ordinary-name member entry.
    pub const fn new(
        id: I,
        name: SymbolName,
        visibility: MemberVisibility,
        validity: MemberValidity,
    ) -> Self {
        Self {
            id,
            name,
            visibility,
            validity,
        }
    }

    /// Returns the exact or closed-family ID stored by this collection.
    pub const fn id(&self) -> I
    where
        I: Copy,
    {
        self.id
    }

    /// Returns the ordinary member name.
    pub const fn name(&self) -> &SymbolName {
        &self.name
    }

    /// Returns the member's declaration-level visibility.
    pub const fn visibility(&self) -> MemberVisibility {
        self.visibility
    }

    /// Returns whether the member's lookup surface is valid or malformed.
    pub const fn validity(&self) -> MemberValidity {
        self.validity
    }
}

/// A validation failure for an immutable typed member collection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemberCollectionBuildError<I> {
    /// The same semantic identity was supplied more than once.
    DuplicateMember(I),
}

/// The shape of a context-specific ordinary-name lookup.
///
/// `T` is the requested typed result. `C` retains the collection's candidate ID type for
/// diagnostics when lookup does not produce exactly one requested result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemberLookupResult<T, C = T> {
    /// Exactly one accessible, well-formed member matched.
    Found(T),
    /// No member occupies the requested ordinary name.
    NotFound,
    /// One member occupies the name but has the wrong semantic category.
    WrongKind(Box<[C]>),
    /// More than one accessible member occupies the ordinary name.
    Ambiguous(Box<[C]>),
    /// The name is occupied, but every candidate is inaccessible in the lookup context.
    Inaccessible(Box<[C]>),
    /// At least one accessible candidate has a malformed lookup surface.
    Malformed(Box<[C]>),
}

impl<T, C> MemberLookupResult<T, C> {
    /// Transforms the successful result and every retained diagnostic candidate.
    pub fn map<U, D>(
        self,
        map_found: impl FnOnce(T) -> U,
        map_candidate: impl FnMut(C) -> D,
    ) -> MemberLookupResult<U, D> {
        match self {
            Self::Found(value) => MemberLookupResult::Found(map_found(value)),
            Self::NotFound => MemberLookupResult::NotFound,
            Self::WrongKind(candidates) => MemberLookupResult::WrongKind(
                candidates
                    .into_vec()
                    .into_iter()
                    .map(map_candidate)
                    .collect(),
            ),
            Self::Ambiguous(candidates) => MemberLookupResult::Ambiguous(
                candidates
                    .into_vec()
                    .into_iter()
                    .map(map_candidate)
                    .collect(),
            ),
            Self::Inaccessible(candidates) => MemberLookupResult::Inaccessible(
                candidates
                    .into_vec()
                    .into_iter()
                    .map(map_candidate)
                    .collect(),
            ),
            Self::Malformed(candidates) => MemberLookupResult::Malformed(
                candidates
                    .into_vec()
                    .into_iter()
                    .map(map_candidate)
                    .collect(),
            ),
        }
    }
}

impl<I> MemberLookupResult<I>
where
    I: Copy,
{
    /// Validates the category of a uniquely resolved ordinary-name candidate.
    ///
    /// Ambiguity is resolved before category classification because member kinds do not create
    /// parallel lookup namespaces.
    pub fn classify<T>(self, classify: impl FnOnce(I) -> Option<T>) -> MemberLookupResult<T, I> {
        match self {
            Self::Found(id) => match classify(id) {
                Some(id) => MemberLookupResult::Found(id),
                None => MemberLookupResult::WrongKind(vec![id].into_boxed_slice()),
            },
            Self::NotFound => MemberLookupResult::NotFound,
            Self::WrongKind(candidates) => MemberLookupResult::WrongKind(candidates),
            Self::Ambiguous(candidates) => MemberLookupResult::Ambiguous(candidates),
            Self::Inaccessible(candidates) => MemberLookupResult::Inaccessible(candidates),
            Self::Malformed(candidates) => MemberLookupResult::Malformed(candidates),
        }
    }
}

/// An immutable collection of one exact or closed-family member ID type.
///
/// Unnamed semantic relationships remain representable because ordinary-name indexing is a
/// separate derived contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedMemberCollection<I> {
    members: Box<[I]>,
}

impl<I> TypedMemberCollection<I>
where
    I: Copy + Ord,
{
    /// Builds a typed collection in canonical enumeration order.
    pub fn new(
        members: impl IntoIterator<Item = I>,
    ) -> Result<Self, MemberCollectionBuildError<I>> {
        let members = members.into_iter().collect::<Vec<_>>().into_boxed_slice();

        validate_distinct_members(members.iter().copied())?;

        Ok(Self { members })
    }

    /// Aggregates direct members followed by canonical inherent-implementation contributions.
    ///
    /// Each contribution retains its own source order. The caller supplies inherent
    /// implementations in canonical declaration-table order.
    pub fn aggregate_type_associated<D, C, M>(
        direct_members: D,
        inherent_contributions: C,
    ) -> Result<Self, MemberCollectionBuildError<I>>
    where
        D: IntoIterator<Item = I>,
        C: IntoIterator<Item = M>,
        M: IntoIterator<Item = I>,
    {
        Self::new(aggregate_in_order(direct_members, inherent_contributions))
    }

    /// Returns members in canonical deterministic enumeration order.
    pub fn members(&self) -> &[I] {
        &self.members
    }

    /// Returns the number of members.
    pub const fn len(&self) -> usize {
        self.members.len()
    }

    /// Returns whether the collection contains no members.
    pub const fn is_empty(&self) -> bool {
        self.members.is_empty()
    }
}

/// An immutable ordinary-name index over typed member candidates.
///
/// The ID type should be a closed family containing every semantic category that competes in the
/// owner's ordinary namespace. Entry enumeration preserves candidate order. Map ordering never
/// creates lookup precedence.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MemberLookupIndex<I> {
    members: Box<[MemberEntry<I>]>,
    name_index: BTreeMap<SymbolName, Box<[usize]>>,
}

impl<I> MemberLookupIndex<I>
where
    I: Copy + Ord,
{
    /// Builds an ordinary-name index in canonical candidate order.
    pub fn new(
        members: impl IntoIterator<Item = MemberEntry<I>>,
    ) -> Result<Self, MemberCollectionBuildError<I>> {
        let members = members.into_iter().collect::<Vec<_>>().into_boxed_slice();
        let mut name_index = BTreeMap::<SymbolName, Vec<usize>>::new();

        validate_distinct_members(members.iter().map(MemberEntry::id))?;

        for (index, member) in members.iter().enumerate() {
            // The lookup key and member entry share one immutable allocation.
            name_index
                .entry(member.name.clone())
                .or_default()
                .push(index);
        }

        let name_index = name_index
            .into_iter()
            .map(|(name, indexes)| (name, indexes.into_boxed_slice()))
            .collect();

        Ok(Self {
            members,
            name_index,
        })
    }

    /// Indexes direct members followed by canonical inherent-implementation contributions.
    ///
    /// Each contribution retains its own source order. The caller supplies inherent
    /// implementations in canonical declaration-table order.
    pub fn aggregate_type_associated<D, C, M>(
        direct_members: D,
        inherent_contributions: C,
    ) -> Result<Self, MemberCollectionBuildError<I>>
    where
        D: IntoIterator<Item = MemberEntry<I>>,
        C: IntoIterator<Item = M>,
        M: IntoIterator<Item = MemberEntry<I>>,
    {
        Self::new(aggregate_in_order(direct_members, inherent_contributions))
    }

    /// Returns members in canonical deterministic enumeration order.
    pub fn members(&self) -> &[MemberEntry<I>] {
        &self.members
    }

    /// Returns the number of indexed candidates, including conflicting and malformed entries.
    pub const fn len(&self) -> usize {
        self.members.len()
    }

    /// Returns whether the index contains no named candidates.
    pub const fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// Returns one exact member's ordinary name when it belongs to this index.
    pub fn name(&self, id: I) -> Option<&SymbolName> {
        self.entry(id).map(MemberEntry::name)
    }

    /// Returns one exact member's immutable lookup entry.
    pub fn entry(&self, id: I) -> Option<&MemberEntry<I>> {
        self.members.iter().find(|member| member.id() == id)
    }

    /// Resolves an ordinary name with every candidate considered accessible.
    pub fn lookup(&self, name: &str) -> MemberLookupResult<I> {
        self.lookup_with_access(name, |_, _| true)
    }

    /// Resolves an ordinary name through the public declaration surface.
    pub fn lookup_public(&self, name: &str) -> MemberLookupResult<I> {
        self.lookup_with_access(name, |_, visibility| visibility.is_public())
    }

    /// Resolves an ordinary name using context-specific effective accessibility.
    ///
    /// The predicate can combine member visibility with owner, module, package, or other access
    /// conditions without making those context-dependent conditions part of the immutable collection.
    pub fn lookup_with_access(
        &self,
        name: &str,
        mut is_accessible: impl FnMut(I, MemberVisibility) -> bool,
    ) -> MemberLookupResult<I> {
        let Some(indexes) = self.name_index.get(name) else {
            return MemberLookupResult::NotFound;
        };

        let mut accessible = Vec::with_capacity(indexes.len());
        let mut has_malformed_candidate = false;

        for index in indexes {
            let member = &self.members[*index];

            if is_accessible(member.id(), member.visibility()) {
                accessible.push(member.id());
                has_malformed_candidate |= member.validity().is_malformed();
            }
        }

        if accessible.is_empty() {
            let candidates = indexes
                .iter()
                .map(|index| self.members[*index].id())
                .collect::<Vec<_>>()
                .into_boxed_slice();

            return MemberLookupResult::Inaccessible(candidates);
        }

        if has_malformed_candidate {
            return MemberLookupResult::Malformed(accessible.into_boxed_slice());
        }

        match accessible.as_slice() {
            [id] => MemberLookupResult::Found(*id),
            _ => MemberLookupResult::Ambiguous(accessible.into_boxed_slice()),
        }
    }
}

fn aggregate_in_order<T, D, C, M>(direct_members: D, contributions: C) -> Vec<T>
where
    D: IntoIterator<Item = T>,
    C: IntoIterator<Item = M>,
    M: IntoIterator<Item = T>,
{
    direct_members
        .into_iter()
        .chain(contributions.into_iter().flat_map(IntoIterator::into_iter))
        .collect()
}

fn validate_distinct_members<I>(
    members: impl IntoIterator<Item = I>,
) -> Result<(), MemberCollectionBuildError<I>>
where
    I: Copy + Ord,
{
    let mut member_ids = BTreeSet::new();

    for member in members {
        if !member_ids.insert(member) {
            return Err(MemberCollectionBuildError::DuplicateMember(member));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        MemberCollectionBuildError, MemberEntry, MemberLookupIndex, MemberLookupResult,
        MemberValidity, MemberVisibility, TypedMemberCollection,
    };
    use crate::SymbolName;

    #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
    enum TestMemberId {
        Function(u8),
        Structure(u8),
    }

    #[test]
    fn one_ordinary_namespace_preserves_cross_kind_ambiguity_in_source_order() {
        let collection = lookup_index([
            member(TestMemberId::Function(1), "item"),
            member(TestMemberId::Structure(2), "item"),
        ]);

        assert_eq!(
            collection.lookup("item"),
            MemberLookupResult::Ambiguous(
                vec![TestMemberId::Function(1), TestMemberId::Structure(2)].into_boxed_slice()
            )
        );
    }

    #[test]
    fn category_classification_distinguishes_wrong_kind_from_not_found() {
        let collection = lookup_index([member(TestMemberId::Structure(1), "Item")]);

        let result = collection.lookup("Item").classify(|id| match id {
            TestMemberId::Function(id) => Some(id),
            TestMemberId::Structure(_) => None,
        });

        assert_eq!(
            result,
            MemberLookupResult::WrongKind(vec![TestMemberId::Structure(1)].into_boxed_slice())
        );

        assert_eq!(
            collection.lookup("Missing").classify(|_| Some(0)),
            MemberLookupResult::NotFound
        );
    }

    #[test]
    fn lookup_mapping_preserves_failure_shape_and_candidate_order() {
        let result = MemberLookupResult::<TestMemberId>::Ambiguous(
            vec![TestMemberId::Function(1), TestMemberId::Structure(2)].into_boxed_slice(),
        );

        assert_eq!(
            result.map(|id| id, |id| id),
            MemberLookupResult::Ambiguous(
                vec![TestMemberId::Function(1), TestMemberId::Structure(2)].into_boxed_slice()
            )
        );
    }

    #[test]
    fn public_lookup_filters_internal_candidates_without_creating_precedence() {
        let collection = MemberLookupIndex::new([
            member_with(
                TestMemberId::Function(1),
                "run",
                MemberVisibility::Internal,
                MemberValidity::Valid,
            ),
            member(TestMemberId::Function(2), "run"),
            member_with(
                TestMemberId::Function(3),
                "hidden",
                MemberVisibility::Internal,
                MemberValidity::Valid,
            ),
        ]);

        let Ok(collection) = collection else {
            panic!("test members must have distinct IDs");
        };

        assert_eq!(
            collection.lookup("run"),
            MemberLookupResult::Ambiguous(
                vec![TestMemberId::Function(1), TestMemberId::Function(2)].into_boxed_slice()
            )
        );

        assert_eq!(
            collection.lookup_public("run"),
            MemberLookupResult::Found(TestMemberId::Function(2))
        );

        assert_eq!(
            collection.lookup_public("hidden"),
            MemberLookupResult::Inaccessible(vec![TestMemberId::Function(3)].into_boxed_slice())
        );
    }

    #[test]
    fn context_specific_access_can_use_member_identity_and_visibility() {
        let collection = lookup_index([
            member_with(
                TestMemberId::Function(1),
                "run",
                MemberVisibility::Internal,
                MemberValidity::Valid,
            ),
            member_with(
                TestMemberId::Function(2),
                "run",
                MemberVisibility::Internal,
                MemberValidity::Valid,
            ),
        ]);

        assert_eq!(
            collection.lookup_with_access("run", |id, _| id == TestMemberId::Function(2)),
            MemberLookupResult::Found(TestMemberId::Function(2))
        );
    }

    #[test]
    fn malformed_candidates_are_never_selected_arbitrarily() {
        let collection = MemberLookupIndex::new([
            member(TestMemberId::Function(1), "run"),
            member_with(
                TestMemberId::Function(2),
                "run",
                MemberVisibility::Public,
                MemberValidity::Malformed,
            ),
        ]);

        let Ok(collection) = collection else {
            panic!("test members must have distinct IDs");
        };

        assert_eq!(
            collection.lookup("run"),
            MemberLookupResult::Malformed(
                vec![TestMemberId::Function(1), TestMemberId::Function(2)].into_boxed_slice()
            )
        );
    }

    #[test]
    fn type_associated_aggregation_uses_direct_then_canonical_inherent_order() {
        let collection = TypedMemberCollection::aggregate_type_associated(
            [TestMemberId::Function(1)],
            [
                vec![TestMemberId::Function(2), TestMemberId::Function(3)],
                vec![TestMemberId::Function(4), TestMemberId::Function(5)],
            ],
        );

        let lookup_index = MemberLookupIndex::aggregate_type_associated(
            [member(TestMemberId::Function(1), "shared")],
            [
                vec![
                    member(TestMemberId::Function(2), "first"),
                    member(TestMemberId::Function(3), "shared"),
                ],
                vec![
                    member(TestMemberId::Function(4), "second"),
                    member(TestMemberId::Function(5), "shared"),
                ],
            ],
        );

        let Ok(collection) = collection else {
            panic!("test members must have distinct IDs");
        };

        let Ok(lookup_index) = lookup_index else {
            panic!("test lookup members must have distinct IDs");
        };

        assert_eq!(
            collection.members(),
            [
                TestMemberId::Function(1),
                TestMemberId::Function(2),
                TestMemberId::Function(3),
                TestMemberId::Function(4),
                TestMemberId::Function(5),
            ]
        );

        assert_eq!(
            lookup_index.lookup("shared"),
            MemberLookupResult::Ambiguous(
                vec![
                    TestMemberId::Function(1),
                    TestMemberId::Function(3),
                    TestMemberId::Function(5)
                ]
                .into_boxed_slice()
            )
        );
    }

    #[test]
    fn duplicate_semantic_ids_are_rejected_without_collapsing_entries() {
        let duplicate = TestMemberId::Function(1);

        let collection = TypedMemberCollection::new([duplicate, duplicate]);

        let lookup_index =
            MemberLookupIndex::new([member(duplicate, "first"), member(duplicate, "second")]);

        assert_eq!(
            collection,
            Err(MemberCollectionBuildError::DuplicateMember(duplicate))
        );

        assert_eq!(
            lookup_index,
            Err(MemberCollectionBuildError::DuplicateMember(duplicate))
        );
    }

    #[test]
    fn member_collections_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<TypedMemberCollection<TestMemberId>>();
        assert_send_sync::<MemberLookupIndex<TestMemberId>>();
        assert_send_sync::<MemberLookupResult<TestMemberId>>();
    }

    fn lookup_index(
        members: impl IntoIterator<Item = MemberEntry<TestMemberId>>,
    ) -> MemberLookupIndex<TestMemberId> {
        match MemberLookupIndex::new(members) {
            Ok(index) => index,
            Err(error) => panic!("test members must have distinct IDs: {error:?}"),
        }
    }

    fn member(id: TestMemberId, name: &str) -> MemberEntry<TestMemberId> {
        member_with(id, name, MemberVisibility::Public, MemberValidity::Valid)
    }

    fn member_with(
        id: TestMemberId,
        name: &str,
        visibility: MemberVisibility,
        validity: MemberValidity,
    ) -> MemberEntry<TestMemberId> {
        let Some(name) = SymbolName::try_new(name) else {
            panic!("test member names must be present");
        };

        MemberEntry::new(id, name, visibility, validity)
    }
}
