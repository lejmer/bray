pub(crate) fn heading(elapsed_nanoseconds: u64) -> String {
    let milliseconds = elapsed_nanoseconds / 1_000_000;
    let fractional_milliseconds = (elapsed_nanoseconds % 1_000_000) / 1_000;

    format!("Compiler profile: {milliseconds}.{fractional_milliseconds:03} ms")
}

pub(crate) fn queries(
    requests: u64,
    evaluations: u64,
    cache_hits: u64,
    cache_misses: u64,
) -> String {
    format!(
        "Queries: {requests} requested, {evaluations} evaluated, {cache_hits} cache hits, {cache_misses} cache misses"
    )
}

pub(crate) fn trace(events: usize, dropped_events: u64) -> String {
    format!("Trace: {events} events, {dropped_events} dropped")
}
