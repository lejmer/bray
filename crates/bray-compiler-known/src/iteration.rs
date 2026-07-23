define_catalog_enum! {
    /// Identifies one declaration participating in the language iteration protocols.
    #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
    pub enum CompilerKnownIterationRole {
        /// The `Iterable` trait definition.
        IterableTrait => "IterableTrait",
        /// The `Iterable.Element` type-valued member.
        IterableElement => "IterableElement",
        /// The `Iterable.Cursor` type-valued member.
        IterableCursor => "IterableCursor",
        /// The `Iterable.iterate` callable.
        IterableIterate => "IterableIterate",
        /// The `Iterator` trait definition.
        IteratorTrait => "IteratorTrait",
        /// The `Iterator.Element` type-valued member.
        IteratorElement => "IteratorElement",
        /// The `Iterator.next` callable.
        IteratorNext => "IteratorNext",
    }
}
