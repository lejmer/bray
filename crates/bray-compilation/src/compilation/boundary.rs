use crate::fact::FactQueryError;

pub(super) fn expect_uncancelled_query<T>(
    boundary: &'static str,
    result: Result<T, FactQueryError>,
) -> T {
    result.unwrap_or_else(|error| panic!("uncancellable boundary '{boundary}' failed: {error}"))
}
