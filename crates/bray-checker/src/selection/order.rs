pub(super) fn canonicalize_by_key<T, K, F>(values: &mut [T], key: F) -> bool
where
    K: Ord,
    F: for<'item> Fn(&'item T) -> &'item K + Copy,
{
    values.sort_unstable_by(|left, right| key(left).cmp(key(right)));

    values.windows(2).all(|pair| key(&pair[0]) != key(&pair[1]))
}
