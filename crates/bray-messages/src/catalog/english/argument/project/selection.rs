use super::super::source::{format_english_path, format_english_quoted_text};

pub(crate) fn format_english_project_selection_problem(
    problem: &bray_diagnostics::DiagnosticProjectSelectionProblem,
) -> String {
    use bray_diagnostics::DiagnosticProjectSelectionProblem as Problem;

    match problem {
        Problem::WorkerCountZero => "worker count must be greater than zero".to_owned(),
        Problem::ProfileRequiresCompilation => {
            "profiling is available only for commands that invoke the compiler".to_owned()
        }
        Problem::InvalidStandardLibraryRoot { path } => format!(
            "standard-library root {} must be an absolute path",
            format_english_path(path)
        ),
        Problem::InvalidPackageIdentity(value) => format!(
            "{} is not a valid package identity",
            format_english_quoted_text(value)
        ),
        Problem::InvalidSourcePackageIdentity(value) => format!(
            "{} is not a valid source-package identity",
            format_english_quoted_text(value)
        ),
        Problem::InvalidPackageVersion(value) => format!(
            "{} is not a valid semantic package version",
            format_english_quoted_text(value)
        ),
        Problem::InvalidProductIdentity(value) => format!(
            "{} cannot form a valid product identity",
            format_english_quoted_text(value)
        ),
        Problem::DependencyArgumentCountMismatch {
            products,
            interfaces,
            implementations,
        } => format!(
            "dependency options contain {products} products, {interfaces} interface paths, and {implementations} implementation paths. Each selected product requires one interface path and either zero or one implementation path"
        ),
        Problem::InvalidDependencyProduct(value) => format!(
            "{} is not a valid dependency product identity",
            format_english_quoted_text(value)
        ),
        Problem::InvalidPlatformServiceBinding(value) => format!(
            "{} is not a valid platform-service binding",
            format_english_quoted_text(value)
        ),
        Problem::NoMatchingProduct(selection) => format!(
            "no product and target match selection {}",
            format_english_quoted_text(selection)
        ),
        Problem::SingleProductRequired { actual } => {
            format!("this command requires exactly one product and target, but {actual} matched")
        }
        Problem::InvalidPackageSelector(value) => format!(
            "{} is not a valid package selector",
            format_english_quoted_text(value)
        ),
        Problem::UnknownPackage(value) => format!(
            "project has no package {}",
            format_english_quoted_text(value)
        ),
        Problem::UnknownTarget(value) => format!(
            "project has no target {}",
            format_english_quoted_text(value)
        ),
        Problem::ProductTargetUnavailable { product, target } => format!(
            "product {} is not available for target {}",
            format_english_quoted_text(product),
            format_english_quoted_text(target)
        ),
        Problem::InvalidInstallName(value) => format!(
            "{} is not a portable dependency installation name",
            format_english_quoted_text(value)
        ),
        Problem::MissingExecutable => {
            "the completed build did not publish an executable artifact".to_owned()
        }
        Problem::MissingExecutableOutput => {
            "the completed build did not publish an executable output path".to_owned()
        }
        Problem::MissingTestExecutableOutput => {
            "the completed test build did not publish its executable output".to_owned()
        }
        Problem::MissingTestCatalogOutput => {
            "the completed test build did not publish its test catalog".to_owned()
        }
        Problem::MissingTestHost => {
            "the completed test build did not publish its test host".to_owned()
        }
        Problem::MissingReusableBuildIdentity(product) => format!(
            "the retained test product {} has no reusable build identity",
            format_english_quoted_text(product),
        ),
        Problem::ReusableBuildIdentityMismatch { product, part } => format!(
            "the retained test product {} has a mismatched {} identity",
            format_english_quoted_text(product),
            part.as_str().replace('_', " ")
        ),
        Problem::UnsupportedInspectionProduct(value) => format!(
            "inspection is unavailable for product {}",
            format_english_quoted_text(value)
        ),
        Problem::TargetRequired => "this command requires an explicit target".to_owned(),
        Problem::InvalidTestFilter(value) => format!(
            "{} is not a valid test-name filter",
            format_english_quoted_text(value)
        ),
    }
}
