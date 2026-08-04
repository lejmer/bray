use serde::Serialize;

use crate::tack::model::TackBuildConfiguration;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BuildProgressAction {
    CheckInterface,
    ProduceArtifacts,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BuildProgressPackage {
    identity: String,
    path: String,
    units: u64,
    action: BuildProgressAction,
}

impl BuildProgressPackage {
    pub(crate) fn new(
        identity: impl Into<String>,
        path: impl Into<String>,
        units: u64,
        action: BuildProgressAction,
    ) -> Self {
        Self {
            identity: identity.into(),
            path: path.into(),
            units,
            action,
        }
    }

    pub(crate) fn identity(&self) -> &str {
        &self.identity
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) const fn units(&self) -> u64 {
        self.units
    }

    pub(crate) const fn action(&self) -> BuildProgressAction {
        self.action
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BuildProgressPlan {
    product: String,
    configuration: TackBuildConfiguration,
    artifact: String,
    output_path: String,
    packages: Vec<BuildProgressPackage>,
}

impl BuildProgressPlan {
    pub(crate) fn new(
        product: impl Into<String>,
        configuration: TackBuildConfiguration,
        artifact: impl Into<String>,
        output_path: impl Into<String>,
        packages: Vec<BuildProgressPackage>,
    ) -> Self {
        Self {
            product: product.into(),
            configuration,
            artifact: artifact.into(),
            output_path: output_path.into(),
            packages,
        }
    }

    pub(crate) fn product(&self) -> &str {
        &self.product
    }

    pub(crate) const fn configuration(&self) -> TackBuildConfiguration {
        self.configuration
    }

    pub(crate) fn artifact(&self) -> &str {
        &self.artifact
    }

    pub(crate) fn output_path(&self) -> &str {
        &self.output_path
    }

    pub(crate) fn packages(&self) -> &[BuildProgressPackage] {
        &self.packages
    }

    pub(crate) fn total_units(&self) -> u64 {
        self.packages.iter().map(BuildProgressPackage::units).sum()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum BuildProgressStatus {
    Complete,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct PackageProgressReport {
    package: String,
    path: String,
    action: BuildProgressAction,
    status: BuildProgressStatus,
    completed_units: u64,
    total_units: u64,
    duration_milliseconds: u64,
}

impl PackageProgressReport {
    pub(super) fn new(
        package: &BuildProgressPackage,
        status: BuildProgressStatus,
        completed_units: u64,
        duration_milliseconds: u64,
    ) -> Self {
        Self {
            package: package.identity().to_owned(),
            path: package.path().to_owned(),
            action: package.action(),
            status,
            completed_units,
            total_units: package.units(),
            duration_milliseconds,
        }
    }

    pub(super) fn package(&self) -> &str {
        &self.package
    }

    pub(super) fn path(&self) -> &str {
        &self.path
    }

    pub(super) const fn status(&self) -> BuildProgressStatus {
        self.status
    }

    pub(super) const fn completed_units(&self) -> u64 {
        self.completed_units
    }

    pub(super) const fn total_units(&self) -> u64 {
        self.total_units
    }

    pub(super) const fn duration_milliseconds(&self) -> u64 {
        self.duration_milliseconds
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct BuildProgressReport {
    product: String,
    configuration: TackBuildConfiguration,
    status: BuildProgressStatus,
    artifact: String,
    output_path: String,
    completed_units: u64,
    total_units: u64,
    duration_milliseconds: u64,
    packages: Vec<PackageProgressReport>,
}

impl BuildProgressReport {
    pub(super) fn new(
        plan: &BuildProgressPlan,
        status: BuildProgressStatus,
        duration_milliseconds: u64,
        packages: Vec<PackageProgressReport>,
    ) -> Self {
        let completed_units = packages
            .iter()
            .map(PackageProgressReport::completed_units)
            .sum();

        Self {
            product: plan.product().to_owned(),
            configuration: plan.configuration(),
            status,
            artifact: plan.artifact().to_owned(),
            output_path: plan.output_path().to_owned(),
            completed_units,
            total_units: plan.total_units(),
            duration_milliseconds,
            packages,
        }
    }

    pub(super) fn product(&self) -> &str {
        &self.product
    }

    pub(super) const fn configuration(&self) -> TackBuildConfiguration {
        self.configuration
    }

    pub(super) const fn status(&self) -> BuildProgressStatus {
        self.status
    }

    pub(super) fn artifact(&self) -> &str {
        &self.artifact
    }

    pub(super) fn output_path(&self) -> &str {
        &self.output_path
    }

    pub(super) const fn completed_units(&self) -> u64 {
        self.completed_units
    }

    pub(super) const fn total_units(&self) -> u64 {
        self.total_units
    }

    pub(super) const fn duration_milliseconds(&self) -> u64 {
        self.duration_milliseconds
    }

    pub(super) fn packages(&self) -> &[PackageProgressReport] {
        &self.packages
    }
}
