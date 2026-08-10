use std::fmt;
use std::path::Path;
use std::process::Command;

pub(crate) fn require_no_normal_dependencies(
    root: &Path,
    package: &'static str,
) -> Result<(), DependencyAuditError> {
    let names = normal_dependency_names(root, package)?;

    if let Some(dependency) = names.iter().find(|name| name.as_str() != package) {
        return Err(DependencyAuditError::UnexpectedDependency {
            package,
            dependency: dependency.clone(),
        });
    }

    Ok(())
}

pub(crate) fn require_absent_normal_dependencies(
    root: &Path,
    package: &'static str,
    forbidden: &[&'static str],
) -> Result<(), DependencyAuditError> {
    let names = normal_dependency_names(root, package)?;

    if let Some(dependency) = forbidden
        .iter()
        .find(|dependency| names.iter().any(|name| name == **dependency))
    {
        return Err(DependencyAuditError::ForbiddenDependency {
            package,
            dependency,
        });
    }

    Ok(())
}

fn normal_dependency_names(
    root: &Path,
    package: &'static str,
) -> Result<Vec<String>, DependencyAuditError> {
    let output = Command::new("cargo")
        .current_dir(root)
        .args([
            "tree",
            "--locked",
            "--package",
            package,
            "--edges",
            "normal",
            "--no-default-features",
            "--prefix",
            "none",
        ])
        .output()
        .map_err(DependencyAuditError::Cargo)?;

    if !output.status.success() {
        return Err(DependencyAuditError::CargoFailed(package));
    }

    let output = String::from_utf8(output.stdout)
        .map_err(|_| DependencyAuditError::NonUtf8Output(package))?;

    Ok(output
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .collect())
}

#[derive(Debug)]
pub(crate) enum DependencyAuditError {
    Cargo(std::io::Error),
    CargoFailed(&'static str),
    NonUtf8Output(&'static str),
    UnexpectedDependency {
        package: &'static str,
        dependency: String,
    },
    ForbiddenDependency {
        package: &'static str,
        dependency: &'static str,
    },
}

impl fmt::Display for DependencyAuditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cargo(error) => write!(formatter, "could not inspect Cargo dependencies: {error}"),
            Self::CargoFailed(package) => {
                write!(formatter, "Cargo dependency inspection failed for {package}")
            }
            Self::NonUtf8Output(package) => {
                write!(formatter, "Cargo dependency output for {package} is not UTF-8")
            }
            Self::UnexpectedDependency {
                package,
                dependency,
            } => write!(
                formatter,
                "{package} has forbidden normal dependency {dependency}"
            ),
            Self::ForbiddenDependency {
                package,
                dependency,
            } => write!(
                formatter,
                "{package} reaches forbidden normal dependency {dependency}"
            ),
        }
    }
}
