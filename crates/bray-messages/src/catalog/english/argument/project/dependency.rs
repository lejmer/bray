use super::super::source::format_english_quoted_text;

pub(crate) fn format_english_project_dependency_cycle_member(
    member: &bray_diagnostics::DiagnosticProjectDependencyCycleMember,
) -> String {
    use bray_diagnostics::DiagnosticProjectDependencyCycleMember as Member;

    match member {
        Member::Package { identity } => {
            format!("package {}", format_english_quoted_text(identity))
        }
        Member::Product { package, product } => format!(
            "product {}/{}",
            format_english_quoted_text(package),
            format_english_quoted_text(product)
        ),
    }
}
