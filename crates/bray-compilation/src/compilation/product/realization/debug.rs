use std::collections::BTreeMap;
use std::num::NonZeroU32;

use bray_codegen::{CodegenDebugLocation, CodegenSourceFile, CodegenUnit, demanded_debug_sources};
use bray_ir::MirSourceAnchor;
use bray_source::{LineIndex, SourceSnapshot};

use super::super::super::{CodegenPreparationError, Compilation};
use super::super::{ProductDataKind, ProductQueryContext, ProductQueryFailure};

impl Compilation {
    pub(super) fn codegen_debug_locations(
        &self,
        unit: &CodegenUnit,
    ) -> Result<Vec<CodegenDebugLocation>, CodegenPreparationError> {
        let mut sources = BTreeMap::new();

        let generated_file = CodegenSourceFile::try_new("generated.bray").ok_or_else(|| {
            ProductQueryFailure::InvalidCodegenSourceFile {
                source: None,
                path: "generated.bray".to_owned(),
            }
        })?;

        let mut locations = Vec::new();

        for anchor in demanded_debug_sources(unit) {
            let debug_location = match &anchor {
                MirSourceAnchor::Source(origin) => {
                    let source_anchor = origin.source_anchor();
                    let syntax = source_anchor.syntax();

                    if !sources.contains_key(&syntax.source_id()) {
                        let source = self.source(syntax.source_id());
                        let actual_version = source.as_ref().map(|source| source.version());

                        let source = source
                            .filter(|source| source.version() == source_anchor.source_version())
                            .ok_or_else(|| ProductQueryFailure::SourceSnapshotMismatch {
                                source: syntax.source_id(),
                                expected: source_anchor.source_version(),
                                actual: actual_version,
                            })?;

                        let index = LineIndex::new(source.text()).map_err(|cause| {
                            ProductQueryFailure::SourceIndex {
                                source: source.source_id(),
                                cause,
                            }
                        })?;

                        let file = codegen_source_file(source)?;
                        sources.insert(syntax.source_id(), (index, file));
                    }

                    let (index, file) = sources.get(&syntax.source_id()).ok_or_else(|| {
                        ProductQueryFailure::missing(
                            ProductQueryContext::Source(syntax.source_id()),
                            ProductDataKind::SourceLineIndex,
                        )
                    })?;

                    let location = index
                        .line_column(syntax.full_range().start())
                        .ok_or_else(|| missing_source_location(&syntax))?;

                    let line = NonZeroU32::new(location.line())
                        .ok_or_else(|| missing_source_location(&syntax))?;

                    let column = NonZeroU32::new(location.column())
                        .ok_or_else(|| missing_source_location(&syntax))?;

                    // The clone retains the shared immutable normalized path.
                    CodegenDebugLocation::new(anchor, file.clone(), line, column)
                }
                MirSourceAnchor::ImportedExecutable(_)
                | MirSourceAnchor::CompilerProvidedCallable(_)
                | MirSourceAnchor::ExecutableHost(_)
                | MirSourceAnchor::GeneratedLifecycle(_) => {
                    // The clone retains the shared immutable generated path.
                    CodegenDebugLocation::new(
                        anchor,
                        generated_file.clone(),
                        NonZeroU32::MIN,
                        NonZeroU32::MIN,
                    )
                }
            };

            locations.push(debug_location);
        }

        Ok(locations)
    }
}

fn missing_source_location(syntax: &bray_declarations::SyntaxAnchor) -> ProductQueryFailure {
    ProductQueryFailure::missing(
        ProductQueryContext::SourceLocation {
            source: syntax.source_id(),
            offset: syntax.full_range().start(),
        },
        ProductDataKind::SourceLocation,
    )
}

fn codegen_source_file(
    source: &SourceSnapshot,
) -> Result<CodegenSourceFile, CodegenPreparationError> {
    let origin = source.origin();

    let path = origin
        .file_path()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .or_else(|| origin.virtual_name().map(str::to_owned))
        .or_else(|| origin.generated_name().map(str::to_owned))
        .or_else(|| origin.lsp_uri().map(str::to_owned))
        .or_else(|| origin.test_fixture_name().map(str::to_owned))
        .unwrap_or_else(|| format!("source-{}.bray", source.identity().raw()));

    CodegenSourceFile::try_new(path.clone()).ok_or_else(|| {
        ProductQueryFailure::InvalidCodegenSourceFile {
            source: Some(source.source_id()),
            path,
        }
        .into()
    })
}
