use bray_codegen::{CodegenCallSite, CodegenInstanceDependencyKind, CodegenInstanceKey};
use bray_ir::MirHelperReference;

use super::super::specialization::ConcreteCodegenInstance;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum NativeDemandReason {
    ExecutableEntry,
    TestEntry,
    LibraryExport,
    ImplementationFulfillment,
    RuntimeRole,
    NativeExport,
    StaticLifecycle,
    CallableDefault,
    DirectCall,
    AddressedFunction,
    GeneratedHelper,
    NativeReference,
    HostedRoot,
}

impl NativeDemandReason {
    pub(crate) const fn for_call_site(site: CodegenCallSite) -> Self {
        match site {
            CodegenCallSite::Operation(_) | CodegenCallSite::Terminator(_) => Self::DirectCall,
            CodegenCallSite::InlineAssemblyOperation { .. }
            | CodegenCallSite::InlineAssemblyTerminator { .. } => Self::NativeReference,
        }
    }

    pub(crate) const fn for_helper(reference: &MirHelperReference) -> Self {
        match reference {
            MirHelperReference::AnonymousCallable(_) | MirHelperReference::DeclaredCallable(_) => {
                Self::AddressedFunction
            }
            _ => Self::GeneratedHelper,
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct NativeDemand {
    predecessor: Option<CodegenInstanceKey>,
    target: CodegenInstanceKey,
    reason: NativeDemandReason,
}

impl NativeDemand {
    pub(crate) fn root(target: CodegenInstanceKey, reason: NativeDemandReason) -> Self {
        Self {
            predecessor: None,
            target,
            reason,
        }
    }

    pub(crate) fn dependency(
        predecessor: CodegenInstanceKey,
        target: CodegenInstanceKey,
        reason: NativeDemandReason,
    ) -> Self {
        Self {
            predecessor: Some(predecessor),
            target,
            reason,
        }
    }

    pub(crate) const fn predecessor(&self) -> Option<&CodegenInstanceKey> {
        self.predecessor.as_ref()
    }

    pub(crate) const fn target(&self) -> &CodegenInstanceKey {
        &self.target
    }

    pub(crate) const fn reason(&self) -> NativeDemandReason {
        self.reason
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ConcreteCodegenRoot {
    instance: ConcreteCodegenInstance,
    reason: NativeDemandReason,
}

impl ConcreteCodegenRoot {
    pub(crate) const fn new(
        instance: ConcreteCodegenInstance,
        reason: NativeDemandReason,
    ) -> Self {
        Self { instance, reason }
    }

    pub(crate) const fn instance(&self) -> &ConcreteCodegenInstance {
        &self.instance
    }

    pub(crate) const fn reason(&self) -> NativeDemandReason {
        self.reason
    }

    pub(crate) fn key(&self) -> &CodegenInstanceKey {
        self.instance.key()
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ConcreteCodegenDemand {
    instance: ConcreteCodegenInstance,
    scheduling: CodegenInstanceDependencyKind,
    reason: NativeDemandReason,
}

impl ConcreteCodegenDemand {
    pub(crate) const fn definition(
        instance: ConcreteCodegenInstance,
        reason: NativeDemandReason,
    ) -> Self {
        Self {
            instance,
            scheduling: CodegenInstanceDependencyKind::Definition,
            reason,
        }
    }

    pub(crate) const fn static_lifecycle(instance: ConcreteCodegenInstance) -> Self {
        Self::definition(instance, NativeDemandReason::StaticLifecycle)
    }

    pub(crate) const fn instance(&self) -> &ConcreteCodegenInstance {
        &self.instance
    }

    pub(crate) const fn scheduling(&self) -> CodegenInstanceDependencyKind {
        self.scheduling
    }

    pub(crate) const fn reason(&self) -> NativeDemandReason {
        self.reason
    }

    pub(crate) fn key(&self) -> &CodegenInstanceKey {
        self.instance.key()
    }
}
