use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::external_tool::{response_file_materialization_path, response_file_path};

pub(crate) const REPORT_ENVIRONMENT: &str = "BRAY_LLD_OPTIMIZATION_REPORT";
const REPORT_SUFFIX: &str = ".bray-thinlto.json";
const FORMAT: u32 = 1;

/// Exact outcomes observed by the pinned linker during one ThinLTO product link.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct LinkOptimizationReport {
    toolchain: String,
    driver: String,
    imported_functions: u64,
    imported_data: u64,
    eliminated_functions: u64,
    eliminated_data: u64,
    eliminated_bytes: u64,
    cache_hits: u64,
    cache_misses: u64,
    cache_writes: u64,
    reused_partitions: u64,
    peak_resident_bytes: u64,
    active_workers: u64,
}

impl LinkOptimizationReport {
    /// Returns the exact pinned linker build identity that produced the report.
    pub fn toolchain(&self) -> &str {
        &self.toolchain
    }

    /// Returns the LLD driver flavor that produced the report.
    pub fn driver(&self) -> &str {
        &self.driver
    }

    /// Returns the number of function definitions imported across modules.
    pub const fn imported_functions(&self) -> u64 {
        self.imported_functions
    }

    /// Returns the number of data definitions imported across modules.
    pub const fn imported_data(&self) -> u64 {
        self.imported_data
    }

    /// Returns the number of function definitions removed by optimization.
    pub const fn eliminated_functions(&self) -> u64 {
        self.eliminated_functions
    }

    /// Returns the number of data definitions removed by optimization.
    pub const fn eliminated_data(&self) -> u64 {
        self.eliminated_data
    }

    /// Returns the LLVM IR representation bytes attributed to removed definitions.
    pub const fn eliminated_bytes(&self) -> u64 {
        self.eliminated_bytes
    }

    /// Returns the number of native partitions obtained from the ThinLTO cache.
    pub const fn cache_hits(&self) -> u64 {
        self.cache_hits
    }

    /// Returns the number of native partitions compiled after cache misses.
    pub const fn cache_misses(&self) -> u64 {
        self.cache_misses
    }

    /// Returns the number of compiled partitions committed to the cache.
    pub const fn cache_writes(&self) -> u64 {
        self.cache_writes
    }

    /// Returns the number of cached native partitions reused by the link.
    pub const fn reused_partitions(&self) -> u64 {
        self.reused_partitions
    }

    /// Returns the linker's peak resident memory in bytes.
    pub const fn peak_resident_bytes(&self) -> u64 {
        self.peak_resident_bytes
    }

    /// Returns the maximum number of simultaneously active LTO workers.
    pub const fn active_workers(&self) -> u64 {
        self.active_workers
    }
}

/// Exact contract problem found while collecting native optimization outcomes.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum LinkOptimizationReportProblem {
    /// The pinned linker did not publish its required report.
    Missing,
    /// The report could not be read or removed.
    Inaccessible(io::ErrorKind),
    /// The report was not valid JSON with the required typed fields.
    Malformed,
    /// The report uses an unsupported format number.
    UnsupportedFormat(u32),
    /// The report came from another pinned linker build.
    ToolchainMismatch(String),
    /// The report came from another LLD driver flavor.
    DriverMismatch(String),
    /// Cache outcome counters contradict each other.
    InconsistentCacheOutcomes,
    /// The linker did not provide its required resource measurements.
    MissingResourceMeasurement,
}

pub(crate) struct OptimizationReportRequest {
    path: PathBuf,
    partial_path: PathBuf,
    expected_toolchain: String,
    expected_driver: &'static str,
}

impl OptimizationReportRequest {
    pub(crate) fn new(
        primary_output: &Path,
        current_directory: Option<&Path>,
        expected_toolchain: &str,
        expected_driver: &'static str,
    ) -> Self {
        let reference = response_file_path(primary_output, REPORT_SUFFIX);
        let path = response_file_materialization_path(&reference, current_directory);
        let partial_path = partial_report_path(&path);

        Self {
            path,
            partial_path,
            expected_toolchain: expected_toolchain.to_owned(),
            expected_driver,
        }
    }

    pub(crate) fn environment(&self) -> (OsString, OsString) {
        (
            OsString::from(REPORT_ENVIRONMENT),
            self.path.as_os_str().to_owned(),
        )
    }

    pub(crate) fn prepare(&self) -> Result<(), LinkOptimizationReportProblem> {
        remove_if_present(&self.path)?;

        remove_if_present(&self.partial_path)
    }

    pub(crate) fn read(&self) -> Result<LinkOptimizationReport, LinkOptimizationReportProblem> {
        let bytes = std::fs::read(&self.path).map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => LinkOptimizationReportProblem::Missing,
            kind => LinkOptimizationReportProblem::Inaccessible(kind),
        })?;

        let raw: RawReport =
            serde_json::from_slice(&bytes).map_err(|_| LinkOptimizationReportProblem::Malformed)?;

        let report = raw.validate(&self.expected_toolchain, self.expected_driver)?;

        remove_if_present(&self.path)?;
        remove_if_present(&self.partial_path)?;

        Ok(report)
    }

    pub(crate) fn abandon(&self) {
        let _ = remove_if_present(&self.path);
        let _ = remove_if_present(&self.partial_path);
    }
}

pub(crate) fn partial_report_path(path: &Path) -> PathBuf {
    let mut partial = path.as_os_str().to_owned();
    partial.push(".partial");

    PathBuf::from(partial)
}

fn remove_if_present(path: &Path) -> Result<(), LinkOptimizationReportProblem> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(LinkOptimizationReportProblem::Inaccessible(error.kind())),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawReport {
    format: u32,
    toolchain: String,
    driver: String,
    imported_functions: u64,
    imported_data: u64,
    eliminated_functions: u64,
    eliminated_data: u64,
    eliminated_bytes: u64,
    cache_hits: u64,
    cache_misses: u64,
    cache_writes: u64,
    reused_partitions: u64,
    peak_resident_bytes: u64,
    active_workers: u64,
}

impl RawReport {
    fn validate(
        self,
        expected_toolchain: &str,
        expected_driver: &str,
    ) -> Result<LinkOptimizationReport, LinkOptimizationReportProblem> {
        if self.format != FORMAT {
            return Err(LinkOptimizationReportProblem::UnsupportedFormat(
                self.format,
            ));
        }

        if self.toolchain != expected_toolchain {
            return Err(LinkOptimizationReportProblem::ToolchainMismatch(
                self.toolchain,
            ));
        }

        if self.driver != expected_driver {
            return Err(LinkOptimizationReportProblem::DriverMismatch(self.driver));
        }

        if self.reused_partitions != self.cache_hits || self.cache_writes > self.cache_misses {
            return Err(LinkOptimizationReportProblem::InconsistentCacheOutcomes);
        }

        if self.peak_resident_bytes == 0 || (self.cache_misses > 0 && self.active_workers == 0) {
            return Err(LinkOptimizationReportProblem::MissingResourceMeasurement);
        }

        Ok(LinkOptimizationReport {
            toolchain: self.toolchain,
            driver: self.driver,
            imported_functions: self.imported_functions,
            imported_data: self.imported_data,
            eliminated_functions: self.eliminated_functions,
            eliminated_data: self.eliminated_data,
            eliminated_bytes: self.eliminated_bytes,
            cache_hits: self.cache_hits,
            cache_misses: self.cache_misses,
            cache_writes: self.cache_writes,
            reused_partitions: self.reused_partitions,
            peak_resident_bytes: self.peak_resident_bytes,
            active_workers: self.active_workers,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{LinkOptimizationReportProblem, OptimizationReportRequest, RawReport};
    use crate::test_support::TestOutput;

    #[test]
    fn report_validation_preserves_exact_outcomes() {
        let output = TestOutput::new("application.exe");
        let request = OptimizationReportRequest::new(output.path(), None, "22.1.8", "coff");

        let (_, report_path) = request.environment();

        let path = Path::new(&report_path);

        std::fs::write(
            path,
            br#"{
                "format": 1,
                "toolchain": "22.1.8",
                "driver": "coff",
                "imported_functions": 4,
                "imported_data": 3,
                "eliminated_functions": 2,
                "eliminated_data": 1,
                "eliminated_bytes": 96,
                "cache_hits": 5,
                "cache_misses": 2,
                "cache_writes": 2,
                "reused_partitions": 5,
                "peak_resident_bytes": 4096,
                "active_workers": 2
            }"#,
        )
        .unwrap_or_else(|error| panic!("test report must be written: {error}"));

        let report = request
            .read()
            .unwrap_or_else(|error| panic!("test report must validate: {error:?}"));

        assert_eq!(report.imported_functions(), 4);
        assert_eq!(report.eliminated_bytes(), 96);
        assert_eq!(report.cache_hits(), 5);
        assert_eq!(report.active_workers(), 2);
    }

    #[test]
    fn report_validation_identifies_the_exact_contract_problem() {
        let output = TestOutput::new("application.exe");
        let request = OptimizationReportRequest::new(output.path(), None, "22.1.8", "coff");

        let (_, report_path) = request.environment();

        let path = Path::new(&report_path);

        std::fs::write(
            path,
            br#"{
                "format": 1,
                "toolchain": "22.1.8",
                "driver": "elf",
                "imported_functions": 0,
                "imported_data": 0,
                "eliminated_functions": 0,
                "eliminated_data": 0,
                "eliminated_bytes": 0,
                "cache_hits": 0,
                "cache_misses": 1,
                "cache_writes": 1,
                "reused_partitions": 0,
                "peak_resident_bytes": 4096,
                "active_workers": 1
            }"#,
        )
        .unwrap_or_else(|error| panic!("test report must be written: {error}"));

        assert_eq!(
            request.read(),
            Err(LinkOptimizationReportProblem::DriverMismatch(
                "elf".to_owned()
            ))
        );

        request.abandon();
    }

    #[test]
    fn cache_misses_require_an_active_worker_measurement() {
        let report = RawReport {
            format: 1,
            toolchain: "exact-linker".to_owned(),
            driver: "coff".to_owned(),
            imported_functions: 0,
            imported_data: 0,
            eliminated_functions: 0,
            eliminated_data: 0,
            eliminated_bytes: 0,
            cache_hits: 0,
            cache_misses: 1,
            cache_writes: 1,
            reused_partitions: 0,
            peak_resident_bytes: 4096,
            active_workers: 0,
        };

        assert_eq!(
            report.validate("exact-linker", "coff"),
            Err(LinkOptimizationReportProblem::MissingResourceMeasurement)
        );
    }

    #[test]
    fn reports_require_the_exact_pinned_linker_identity() {
        let report = RawReport {
            format: 1,
            toolchain: "stale-linker".to_owned(),
            driver: "coff".to_owned(),
            imported_functions: 0,
            imported_data: 0,
            eliminated_functions: 0,
            eliminated_data: 0,
            eliminated_bytes: 0,
            cache_hits: 1,
            cache_misses: 0,
            cache_writes: 0,
            reused_partitions: 1,
            peak_resident_bytes: 4096,
            active_workers: 0,
        };

        assert_eq!(
            report.validate("current-linker", "coff"),
            Err(LinkOptimizationReportProblem::ToolchainMismatch(
                "stale-linker".to_owned()
            ))
        );
    }
}
