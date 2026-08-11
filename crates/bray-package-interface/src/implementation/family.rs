use bray_symbols::InterfaceSymbolId;

use super::model::InterfaceExecutableTemplate;

/// Returns the first owner whose canonically sorted templates do not form one complete family.
pub(crate) fn invalid_executable_template_family(
    templates: &[InterfaceExecutableTemplate],
) -> Option<InterfaceSymbolId> {
    let mut index = 0;

    while let Some(first) = templates.get(index) {
        let owner = first.owner();
        let family_size = first.family_size();
        let start = index;

        while templates
            .get(index)
            .is_some_and(|template| template.owner() == owner)
        {
            let template = &templates[index];

            let Ok(expected) = u32::try_from(index - start) else {
                return Some(owner);
            };

            if template.identity().raw() != expected || template.family_size() != family_size {
                return Some(owner);
            }

            index += 1;
        }

        if usize::try_from(family_size).ok() != Some(index - start) {
            return Some(owner);
        }
    }

    None
}
