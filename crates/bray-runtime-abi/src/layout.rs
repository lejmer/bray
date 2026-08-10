macro_rules! assert_abi_layout {
    ($type:ty, size: $size:expr, align: $align:expr, fields: { $($field:ident: $offset:expr),* $(,)? }) => {{
        assert_eq!(std::mem::size_of::<$type>(), $size);
        assert_eq!(std::mem::align_of::<$type>(), $align);
        $(assert_eq!(std::mem::offset_of!($type, $field), $offset);)*
    }};
}
