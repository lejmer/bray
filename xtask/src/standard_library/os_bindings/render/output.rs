use super::super::model::Description;
use super::{bray::bray, probe::probe};

pub(in crate::standard_library::os_bindings) fn render_all(
    description: &Description,
    digest: &str,
) -> Result<Vec<RenderedTarget>, String> {
    description
        .targets
        .iter()
        .map(|target| {
            Ok(RenderedTarget {
                target: target.target.clone(),
                file_stem: target.target.replace('-', "_"),
                bray: bray(target, digest)?,
                probe: probe(target, digest),
            })
        })
        .collect()
}

pub(in crate::standard_library::os_bindings) struct RenderedTarget {
    pub(in crate::standard_library::os_bindings) target: String,
    pub(in crate::standard_library::os_bindings) file_stem: String,
    pub(in crate::standard_library::os_bindings) bray: String,
    pub(in crate::standard_library::os_bindings) probe: String,
}
