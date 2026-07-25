/// Scheduling priority for one compiler fact request.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum QueryPriority {
    /// Latency-sensitive work requested by interactive language tooling.
    Interactive,
    /// Ordinary compiler work.
    #[default]
    Normal,
    /// Speculative or broad work that may yield to interactive requests.
    Background,
}
