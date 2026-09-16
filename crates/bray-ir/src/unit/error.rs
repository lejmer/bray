/// A unit-local identity table exceeded its compact representation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MirCapacityError {
    /// A block, operation, storage, value, or frame state would not fit its ID.
    IdentityCapacityExceeded,
}
