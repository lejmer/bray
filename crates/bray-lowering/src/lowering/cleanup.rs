mod control;
mod ordinary;
mod outcomes;

#[cfg(test)]
pub(super) use control::CleanupDestination;
pub(super) use control::TerminalState;
pub(super) use outcomes::AbnormalCleanupMachine;
