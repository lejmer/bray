//! Shared, observable setup for the source/import ownership matrices.

use std::io::Write;
use std::time::Instant;

use crate::Compilation;

pub(super) struct Phase {
    kind: &'static str,
    name: String,
    imported: bool,
    started: Instant,
}

impl Phase {
    pub(super) fn start(kind: &'static str, name: &str, imported: bool) -> Self {
        let phase = Self { kind, name: name.to_owned(), imported, started: Instant::now() };

        phase.report("start");

        phase
    }

    fn report(&self, event: &str) {
        let line = format!("[matrix] {}-{} name={} imported={} elapsed_ms={} pid={} panicking={}\n",
            self.kind, event, self.name, self.imported, self.started.elapsed().as_millis(),
            std::process::id(),
            std::thread::panicking());

        eprint!("{line}");

        if let Some(directory) = std::env::var_os("BRAY_TEST_PROFILE_OUTPUT") {
            std::fs::create_dir_all(&directory).expect("matrix progress directory must be writable");

            let path = std::path::Path::new(&directory).join("matrix-progress.log");

            let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)
                .expect("matrix progress log must open");

            file.write_all(line.as_bytes()).expect("matrix progress must be recorded");
        }
    }
}

impl Drop for Phase {
    fn drop(&mut self) {
        self.report("finish");
    }
}

pub(super) fn provider(name: &str, api: &str) -> Compilation {
    let _phase = Phase::start("provider", name, false);
    let provider = super::fixtures::compilation_from_sources([api, "module app;"]);
    let check = Phase::start("provider-check", name, false);
    let diagnostics = provider.check_diagnostics();

    drop(check);

    assert!(!diagnostics.has_errors(), "{}", super::fixtures::diagnostic_messages(diagnostics));

    provider
}

macro_rules! cases {
    ($($name:ident = $index:literal,)*) => {
        #[test]
        fn registered_cases_match_catalogue() {
            check_matrix_catalogue(&[$(stringify!($name)),*]);
        }
        mod source {
            $(#[test] fn $name() { super::check_matrix_case(false, $index, stringify!($name)); })*
        }
        mod imported {
            $(#[test] fn $name() { super::check_matrix_case(true, $index, stringify!($name)); })*
        }
    };
}

pub(super) use cases;
