use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_runtime_abi::{NativeProductIdentity, NativeStaticIdentity};
use bray_runtime_interface::BinarySymbolName;
use bray_symbols::StaticStorageDuration;

use crate::CodegenUnitKey;

/// Returns the retained object section for static-host contributions.
pub const fn static_host_section_name(format: bray_target::ObjectFormat) -> &'static str {
    match format {
        bray_target::ObjectFormat::Coff => ".bray$S",
        bray_target::ObjectFormat::Elf | bray_target::ObjectFormat::WebAssembly => {
            "bray_static_hosts"
        }
        bray_target::ObjectFormat::MachO => "__DATA,__bray_static",
        bray_target::ObjectFormat::Xcoff => ".bray_static_hosts",
    }
}

/// One static-entry identity referenced by a product-host descriptor.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenProductHostStatic {
    host_symbol: BinarySymbolName,
    identity: NativeStaticIdentity,
    duration: StaticStorageDuration,
    order: u64,
    dependencies: Arc<[NativeStaticIdentity]>,
}

impl CodegenProductHostStatic {
    /// Creates one ordered static-host contribution.
    pub fn new(
        host_symbol: BinarySymbolName,
        identity: NativeStaticIdentity,
        duration: StaticStorageDuration,
        order: u64,
        dependencies: impl IntoIterator<Item = NativeStaticIdentity>,
    ) -> Self {
        let mut dependencies: Vec<_> = dependencies.into_iter().collect();

        dependencies.sort_unstable();
        dependencies.dedup();

        Self {
            host_symbol,
            identity,
            duration,
            order,
            dependencies: dependencies.into(),
        }
    }

    /// Returns the linked static-entry record symbol.
    pub const fn host_symbol(&self) -> &BinarySymbolName {
        &self.host_symbol
    }

    /// Returns the exact static identity.
    pub const fn identity(&self) -> NativeStaticIdentity {
        self.identity
    }

    /// Returns the storage owner category.
    pub const fn duration(&self) -> StaticStorageDuration {
        self.duration
    }

    /// Returns the deterministic cleanup ordinal.
    pub const fn order(&self) -> u64 {
        self.order
    }

    /// Returns direct lifecycle dependency identities.
    pub fn dependencies(&self) -> &[NativeStaticIdentity] {
        &self.dependencies
    }
}

/// Complete backend-neutral mapping for one compiler-generated product host.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenProductHostMapping {
    owner: CodegenUnitKey,
    identity: NativeProductIdentity,
    descriptor_symbol: BinarySymbolName,
    control_symbol: BinarySymbolName,
    control_role: bray_runtime_interface::RuntimeAbiRole,
    statics: Arc<[CodegenProductHostStatic]>,
}

impl CodegenProductHostMapping {
    /// Creates one validated product-host mapping.
    pub fn try_new(
        owner: CodegenUnitKey,
        identity: NativeProductIdentity,
        descriptor_symbol: BinarySymbolName,
        control_symbol: BinarySymbolName,
        control_role: bray_runtime_interface::RuntimeAbiRole,
        statics: impl IntoIterator<Item = CodegenProductHostStatic>,
    ) -> Option<Self> {
        if descriptor_symbol == control_symbol
            || !matches!(
                control_role,
                bray_runtime_interface::RuntimeAbiRole::ProductHostControl
                    | bray_runtime_interface::RuntimeAbiRole::AsynchronousProductHostControl
            )
        {
            return None;
        }

        let mut statics: Vec<_> = statics.into_iter().collect();

        statics.sort_unstable_by_key(CodegenProductHostStatic::order);

        let identities = statics
            .iter()
            .map(CodegenProductHostStatic::identity)
            .collect::<BTreeSet<_>>();

        let orders = statics
            .iter()
            .map(CodegenProductHostStatic::order)
            .collect::<BTreeSet<_>>();

        let symbols = statics
            .iter()
            .map(CodegenProductHostStatic::host_symbol)
            .collect::<BTreeSet<_>>();

        if identities.len() != statics.len()
            || orders.len() != statics.len()
            || symbols.len() != statics.len()
        {
            return None;
        }

        let by_identity = statics
            .iter()
            .map(|entry| (entry.identity(), entry.order()))
            .collect::<BTreeMap<_, _>>();

        for entry in &statics {
            if entry.dependencies().iter().any(|dependency| {
                by_identity
                    .get(dependency)
                    .is_none_or(|dependency_order| *dependency_order <= entry.order())
            }) {
                return None;
            }
        }

        Some(Self {
            owner,
            identity,
            descriptor_symbol,
            control_symbol,
            control_role,
            statics: statics.into(),
        })
    }

    /// Returns the code generation unit that defines the descriptor and control surface.
    pub const fn owner(&self) -> &CodegenUnitKey {
        &self.owner
    }

    /// Returns the stable loaded-product identity.
    pub const fn identity(&self) -> NativeProductIdentity {
        self.identity
    }

    /// Returns the unique descriptor symbol.
    pub const fn descriptor_symbol(&self) -> &BinarySymbolName {
        &self.descriptor_symbol
    }

    /// Returns the unique control symbol.
    pub const fn control_symbol(&self) -> &BinarySymbolName {
        &self.control_symbol
    }

    /// Returns the runtime component that admits and controls this product host.
    pub const fn control_role(&self) -> bray_runtime_interface::RuntimeAbiRole {
        self.control_role
    }

    /// Returns static contributions in deterministic cleanup order.
    pub fn statics(&self) -> &[CodegenProductHostStatic] {
        &self.statics
    }
}

#[cfg(test)]
mod tests {
    use bray_runtime_abi::{NativeProductIdentity, NativeStaticIdentity};
    use bray_runtime_interface::BinarySymbolName;

    use super::{CodegenProductHostMapping, CodegenProductHostStatic, static_host_section_name};

    #[test]
    fn static_host_sections_cover_every_object_format() {
        assert_eq!(
            static_host_section_name(bray_target::ObjectFormat::Coff),
            ".bray$S"
        );

        assert_eq!(
            static_host_section_name(bray_target::ObjectFormat::MachO),
            "__DATA,__bray_static"
        );
    }

    #[test]
    fn product_host_mappings_require_dependency_order() {
        let owner = crate::test_support::codegen_unit_key(1);
        let first = NativeStaticIdentity::new([1; 32]);
        let second = NativeStaticIdentity::new([2; 32]);

        let symbol = |name| {
            BinarySymbolName::try_new(name).unwrap_or_else(|| panic!("test symbol must be valid"))
        };

        let mapping = CodegenProductHostMapping::try_new(
            owner,
            NativeProductIdentity::new([3; 32]),
            symbol("descriptor"),
            symbol("control"),
            bray_runtime_interface::RuntimeAbiRole::ProductHostControl,
            [
                CodegenProductHostStatic::new(
                    symbol("first"),
                    first,
                    bray_symbols::StaticStorageDuration::Product,
                    0,
                    [second],
                ),
                CodegenProductHostStatic::new(
                    symbol("second"),
                    second,
                    bray_symbols::StaticStorageDuration::Product,
                    1,
                    [],
                ),
            ],
        );

        assert!(mapping.is_some());
    }
}
