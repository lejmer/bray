/// Access to the host system entropy source.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct SystemEntropy;

impl SystemEntropy {
    /// Fills the complete destination from the host entropy source.
    pub fn fill(self, destination: &mut [u8]) -> Result<(), SystemEntropyError> {
        getrandom::fill(destination).map_err(|_| SystemEntropyError)
    }
}

/// Failure to obtain bytes from the host system entropy source.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SystemEntropyError;

#[cfg(test)]
mod tests {
    use super::SystemEntropy;

    #[test]
    fn system_entropy_accepts_empty_and_nonempty_destinations() {
        SystemEntropy
            .fill(&mut [])
            .unwrap_or_else(|_| panic!("empty entropy request must succeed"));

        let mut bytes = [0_u8; 32];

        SystemEntropy
            .fill(&mut bytes)
            .unwrap_or_else(|_| panic!("host entropy must be available"));
    }
}
