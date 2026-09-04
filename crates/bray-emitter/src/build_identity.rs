/// Immutable identities needed to prove that a published native test product is reusable.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProductBuildIdentity {
    inputs: [u8; 32],
    compiler: [u8; 32],
    toolchain: [u8; 32],
    standard_library: [u8; 32],
    runtime: [u8; 32],
    catalog_protocol: u32,
    runner_protocol: u32,
}

impl ProductBuildIdentity {
    /// Creates the complete identity chain for one reusable product generation.
    #[expect(
        clippy::too_many_arguments,
        reason = "each independent reusable-build identity remains explicit"
    )]
    pub const fn new(
        inputs: [u8; 32],
        compiler: [u8; 32],
        toolchain: [u8; 32],
        standard_library: [u8; 32],
        runtime: [u8; 32],
        catalog_protocol: u32,
        runner_protocol: u32,
    ) -> Self {
        Self {
            inputs,
            compiler,
            toolchain,
            standard_library,
            runtime,
            catalog_protocol,
            runner_protocol,
        }
    }

    /// Returns the first exact part that differs from another identity chain.
    pub fn mismatch(&self, expected: &Self) -> Option<ProductBuildIdentityPart> {
        if self.inputs != expected.inputs {
            Some(ProductBuildIdentityPart::Inputs)
        } else if self.compiler != expected.compiler {
            Some(ProductBuildIdentityPart::Compiler)
        } else if self.toolchain != expected.toolchain {
            Some(ProductBuildIdentityPart::Toolchain)
        } else if self.standard_library != expected.standard_library {
            Some(ProductBuildIdentityPart::StandardLibrary)
        } else if self.runtime != expected.runtime {
            Some(ProductBuildIdentityPart::Runtime)
        } else if self.catalog_protocol != expected.catalog_protocol {
            Some(ProductBuildIdentityPart::CatalogProtocol)
        } else if self.runner_protocol != expected.runner_protocol {
            Some(ProductBuildIdentityPart::RunnerProtocol)
        } else {
            None
        }
    }
}

/// Exact part of a reusable product identity that failed validation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProductBuildIdentityPart {
    /// Project, product, target, profile, native-link, or source inputs.
    Inputs,
    /// Exact compiler executable.
    Compiler,
    /// Toolchain inputs outside the separately identified standard library and runtime.
    Toolchain,
    /// Selected standard-library bundle or source tree.
    StandardLibrary,
    /// Selected target runtime artifacts.
    Runtime,
    /// Test-catalog wire revision.
    CatalogProtocol,
    /// Native test-runner wire revision.
    RunnerProtocol,
}

#[cfg(test)]
mod tests {
    use super::{ProductBuildIdentity, ProductBuildIdentityPart};

    #[test]
    fn identity_mismatches_preserve_the_first_exact_category() {
        let baseline = identity([[0; 32]; 5], [1, 1]);

        let cases = [
            (
                identity([[1; 32], [0; 32], [0; 32], [0; 32], [0; 32]], [1, 1]),
                ProductBuildIdentityPart::Inputs,
            ),
            (
                identity([[0; 32], [1; 32], [0; 32], [0; 32], [0; 32]], [1, 1]),
                ProductBuildIdentityPart::Compiler,
            ),
            (
                identity([[0; 32], [0; 32], [1; 32], [0; 32], [0; 32]], [1, 1]),
                ProductBuildIdentityPart::Toolchain,
            ),
            (
                identity([[0; 32], [0; 32], [0; 32], [1; 32], [0; 32]], [1, 1]),
                ProductBuildIdentityPart::StandardLibrary,
            ),
            (
                identity([[0; 32], [0; 32], [0; 32], [0; 32], [1; 32]], [1, 1]),
                ProductBuildIdentityPart::Runtime,
            ),
            (
                identity([[0; 32]; 5], [2, 1]),
                ProductBuildIdentityPart::CatalogProtocol,
            ),
            (
                identity([[0; 32]; 5], [1, 2]),
                ProductBuildIdentityPart::RunnerProtocol,
            ),
        ];

        for (actual, expected) in cases {
            assert_eq!(actual.mismatch(&baseline), Some(expected));
        }

        assert_eq!(baseline.mismatch(&baseline), None);
    }

    fn identity(digests: [[u8; 32]; 5], protocols: [u32; 2]) -> ProductBuildIdentity {
        ProductBuildIdentity::new(
            digests[0],
            digests[1],
            digests[2],
            digests[3],
            digests[4],
            protocols[0],
            protocols[1],
        )
    }
}
