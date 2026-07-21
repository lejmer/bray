use crate::{
    ExternalSymbolKey, ImportedInterfaceId, ImportedLookupEdge, ImportedPackageIdentitySurface,
    ImportedSymbolIdentityInput, ImportedSymbolRelationship, ImportedSymbolSkeletonInput,
    InterfaceSymbolId, ModulePathKey, PackageIdentity, SymbolKind, SymbolName,
    SymbolRelationshipKind,
};

#[cfg(test)]
use crate::{ImportedSymbolSkeleton, SymbolId};

#[cfg(test)]
pub(crate) struct InterfaceFixture {
    pub(crate) input: ImportedSymbolSkeletonInput,
    pub(crate) package_key: ExternalSymbolKey,
    pub(crate) module_key: ExternalSymbolKey,
    pub(crate) function_key: ExternalSymbolKey,
}

pub(crate) struct LookupInputFixture {
    pub(crate) input: ImportedSymbolSkeletonInput,
    pub(crate) declaration_key: ExternalSymbolKey,
}

#[derive(Clone, Copy)]
pub(crate) enum ModuleRouteShape {
    Exact,
    DeclaredPrefixes,
}

pub(crate) struct LookupInputSpec<'fixture> {
    pub(crate) package_name: &'fixture str,
    pub(crate) defining_module_path: &'fixture [std::sync::Arc<str>],
    pub(crate) exporting_module_path: &'fixture [std::sync::Arc<str>],
    pub(crate) declaration_kind: SymbolKind,
    pub(crate) declaration_name: &'fixture str,
    pub(crate) lookup_name: &'fixture str,
    pub(crate) route_shape: ModuleRouteShape,
}

#[cfg(test)]
pub(crate) fn interface_fixture(
    interface: u32,
    package_name: &str,
    function_name: &str,
) -> InterfaceFixture {
    interface_fixture_at_module(interface, package_name, ["api"], function_name)
}

#[cfg(test)]
pub(crate) fn interface_fixture_at_module<I, S>(
    interface: u32,
    package_name: &str,
    module_path: I,
    function_name: &str,
) -> InterfaceFixture
where
    I: IntoIterator<Item = S>,
    S: Into<std::sync::Arc<str>>,
{
    interface_fixture_with_lookup(
        interface,
        package_name,
        module_path,
        function_name,
        function_name,
    )
}

#[cfg(test)]
pub(crate) fn interface_fixture_with_lookup<I, S>(
    interface: u32,
    package_name: &str,
    module_path: I,
    function_name: &str,
    lookup_name: &str,
) -> InterfaceFixture
where
    I: IntoIterator<Item = S>,
    S: Into<std::sync::Arc<str>>,
{
    let module_path = module_path.into_iter().map(Into::into).collect::<Vec<_>>();

    let fixture = lookup_input_fixture(
        interface,
        package_name,
        &module_path,
        &module_path,
        SymbolKind::Function,
        function_name,
        lookup_name,
    );

    let package_key = ExternalSymbolKey::package(package_identity(package_name));
    let module_key = module_key(&package_key, &module_path);

    InterfaceFixture {
        input: fixture.input,
        package_key,
        module_key,
        function_key: fixture.declaration_key,
    }
}

#[cfg(test)]
pub(crate) fn lookup_input_fixture(
    interface: u32,
    package_name: &str,
    defining_module_path: &[std::sync::Arc<str>],
    exporting_module_path: &[std::sync::Arc<str>],
    declaration_kind: SymbolKind,
    declaration_name: &str,
    lookup_name: &str,
) -> LookupInputFixture {
    lookup_input_fixture_with_routes(
        interface,
        LookupInputSpec {
            package_name,
            defining_module_path,
            exporting_module_path,
            declaration_kind,
            declaration_name,
            lookup_name,
            route_shape: ModuleRouteShape::Exact,
        },
    )
}

pub(crate) fn lookup_input_fixture_with_routes(
    interface: u32,
    spec: LookupInputSpec<'_>,
) -> LookupInputFixture {
    let package = package_identity(spec.package_name);
    let package_key = ExternalSymbolKey::package(package.clone());

    let mut module_paths = Vec::<Vec<std::sync::Arc<str>>>::new();

    if matches!(spec.route_shape, ModuleRouteShape::DeclaredPrefixes) {
        for length in 1..=spec.defining_module_path.len() {
            module_paths.push(spec.defining_module_path[..length].to_vec());
        }
    } else {
        module_paths.push(spec.defining_module_path.to_vec());
    }

    if !module_paths
        .iter()
        .any(|path| path.as_slice() == spec.exporting_module_path)
    {
        module_paths.push(spec.exporting_module_path.to_vec());
    }

    let module_keys = module_paths
        .iter()
        .map(|path| module_key(&package_key, path))
        .collect::<Vec<_>>();

    let defining_module = module_paths
        .iter()
        .position(|path| path.as_slice() == spec.defining_module_path)
        .unwrap_or_else(|| panic!("defining module path must be present"));

    let exporting_module = module_paths
        .iter()
        .position(|path| path.as_slice() == spec.exporting_module_path)
        .unwrap_or_else(|| panic!("exporting module path must be present"));

    let defining_module_id = interface_symbol_id(defining_module + 1);
    let exporting_module_id = interface_symbol_id(exporting_module + 1);
    let declaration_id = interface_symbol_id(module_paths.len() + 1);

    let Some(declaration_key) = ExternalSymbolKey::named(
        module_keys[defining_module].clone(),
        spec.declaration_kind,
        symbol_name(spec.declaration_name),
    ) else {
        panic!("module key must own test declaration key");
    };

    let mut identities = vec![ImportedSymbolIdentityInput::new(
        InterfaceSymbolId::new(0),
        package_key,
        SymbolKind::Package,
        None,
    )];

    identities.extend(module_keys.into_iter().enumerate().map(|(index, key)| {
        ImportedSymbolIdentityInput::new(
            interface_symbol_id(index + 1),
            key,
            SymbolKind::Module,
            Some(InterfaceSymbolId::new(0)),
        )
    }));

    identities.push(ImportedSymbolIdentityInput::new(
        declaration_id,
        declaration_key.clone(),
        spec.declaration_kind,
        Some(defining_module_id),
    ));

    let symbols = ImportedPackageIdentitySurface::try_new(package, identities)
        .unwrap_or_else(|error| panic!("test identity surface must be valid: {error:?}"));

    let mut relationships = module_paths
        .iter()
        .enumerate()
        .map(|(index, _)| {
            ImportedSymbolRelationship::new(
                SymbolRelationshipKind::PackageModule,
                InterfaceSymbolId::new(0),
                interface_symbol_id(index + 1),
                test_u32(index),
            )
        })
        .collect::<Vec<_>>();

    relationships.push(ImportedSymbolRelationship::new(
        SymbolRelationshipKind::ModuleMember,
        defining_module_id,
        declaration_id,
        0,
    ));

    let lookups = [ImportedLookupEdge::new(
        exporting_module_id,
        symbol_name(spec.lookup_name),
        declaration_key.clone(),
    )];

    let input = ImportedSymbolSkeletonInput::new(
        ImportedInterfaceId::new(interface),
        symbols,
        relationships,
        lookups,
    );

    LookupInputFixture {
        input,
        declaration_key,
    }
}

fn interface_symbol_id(index: usize) -> InterfaceSymbolId {
    InterfaceSymbolId::new(test_u32(index))
}

fn test_u32(value: usize) -> u32 {
    let Ok(value) = u32::try_from(value) else {
        panic!("test index must fit in u32");
    };

    value
}

fn module_key(
    package_key: &ExternalSymbolKey,
    module_path: &[std::sync::Arc<str>],
) -> ExternalSymbolKey {
    let Some(module_path) = ModulePathKey::try_new(module_path.iter().cloned()) else {
        panic!("test module path must be valid");
    };

    let Some(module_key) = ExternalSymbolKey::module(package_key.clone(), module_path) else {
        panic!("package key must own test module key");
    };

    module_key
}

pub(crate) fn package_identity(value: &str) -> PackageIdentity {
    match PackageIdentity::try_new(value) {
        Some(identity) => identity,
        None => panic!("test package identity must be valid"),
    }
}

pub(crate) fn symbol_name(value: &str) -> SymbolName {
    match SymbolName::try_new(value) {
        Some(name) => name,
        None => panic!("test symbol name must be valid"),
    }
}

#[cfg(test)]
pub(crate) fn build_skeleton(
    inputs: impl IntoIterator<Item = ImportedSymbolSkeletonInput>,
) -> ImportedSymbolSkeleton {
    match ImportedSymbolSkeleton::try_new(SymbolId::new(10), inputs) {
        Ok(skeleton) => skeleton,
        Err(error) => panic!("test imported skeleton must build: {error:?}"),
    }
}
