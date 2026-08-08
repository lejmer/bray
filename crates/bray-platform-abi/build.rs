use std::env;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

const DATE_DIGEST: &str = "63373dc7f9e9802a7a9d2b6705e86674034f9716d7aa07f9d7d00d3fbf1ead00";
const TZDATA_DIGEST: &str = "9109885cd793b03fa905cc5e737498c393d590102359e893b8f3b3c5b6f9102a";

const DATE_FILES: &[&str] = &[
    "include/date/date.h",
    "include/date/tz.h",
    "include/date/tz_private.h",
    "src/tz.cpp",
];

const TZDATA_FILES: &[&str] = &[
    "africa",
    "antarctica",
    "asia",
    "australasia",
    "backward",
    "etcetera",
    "europe",
    "leapseconds",
    "northamerica",
    "southamerica",
    "version",
    "windowsZones.xml",
];

fn main() {
    let manifest = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|| panic!("Cargo must provide CARGO_MANIFEST_DIR")),
    );
    let provider = manifest.join("../../third-party/temporal");
    let date = provider.join("date");
    let tzdata = provider.join("tzdata");

    verify_files(&date, DATE_FILES, DATE_DIGEST, "date provider");
    verify_files(&tzdata, TZDATA_FILES, TZDATA_DIGEST, "timezone database");

    let out = PathBuf::from(
        env::var_os("OUT_DIR").unwrap_or_else(|| panic!("Cargo must provide OUT_DIR")),
    );
    let embedded = out.join("bray_tzdata.inc");

    write_embedded_tzdata(&tzdata, &embedded);

    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .include(date.join("include"))
        .include(provider.join("provider/include"))
        .include(&out)
        .define("AUTO_DOWNLOAD", "0")
        .define("HAS_REMOTE_API", "0")
        .define("USE_OS_TZDB", "0")
        .define("ONLY_C_LOCALE", "1")
        .define("NOMINMAX", None)
        .file(date.join("src/tz.cpp"))
        .file(provider.join("provider/src/provider.cpp"))
        .warnings(false)
        .compile("bray_temporal_provider");

    if env::var("CARGO_CFG_TARGET_OS").is_ok_and(|target| target == "windows") {
        println!("cargo:rustc-link-lib=shell32");
        println!("cargo:rustc-link-lib=ole32");
    }

    println!("cargo:rerun-if-changed={}", date.display());
    println!("cargo:rerun-if-changed={}", tzdata.display());
    println!(
        "cargo:rerun-if-changed={}",
        provider.join("provider").display()
    );
}

fn verify_files(root: &Path, files: &[&str], expected: &str, name: &str) {
    let mut hasher = Sha256::new();

    for file in files {
        let path = root.join(file);
        let bytes = fs::read(&path).unwrap_or_else(|error| {
            panic!("could not read pinned {name} {}: {error}", path.display())
        });

        hasher.update(file.as_bytes());
        hasher.update([0]);
        hasher.update(
            u64::try_from(bytes.len())
                .unwrap_or_else(|_| panic!("pinned {name} file is too large"))
                .to_le_bytes(),
        );
        hasher.update(&bytes);
    }

    let actual = format!("{:x}", hasher.finalize());

    assert_eq!(
        actual, expected,
        "pinned {name} content does not match provenance"
    );
}

fn write_embedded_tzdata(root: &Path, destination: &Path) {
    let mut source = String::from(
        "struct BrayEmbeddedFile { const char* name; const unsigned char* bytes; std::size_t length; };\n",
    );

    for (index, file) in TZDATA_FILES.iter().enumerate() {
        let bytes = fs::read(root.join(file))
            .unwrap_or_else(|error| panic!("could not embed timezone data {file}: {error}"));

        write!(
            source,
            "static constexpr unsigned char BRAY_TZDATA_{index}[] = {{"
        )
        .unwrap_or_else(|_| panic!("generated timezone source must be writable"));

        for byte in bytes {
            write!(source, "{byte},")
                .unwrap_or_else(|_| panic!("generated timezone source must be writable"));
        }

        source.push_str("};\n");
    }

    source.push_str("static constexpr BrayEmbeddedFile BRAY_TZDATA_FILES[] = {\n");

    for (index, file) in TZDATA_FILES.iter().enumerate() {
        writeln!(
            source,
            "    {{\"{file}\", BRAY_TZDATA_{index}, sizeof(BRAY_TZDATA_{index})}},"
        )
        .unwrap_or_else(|_| panic!("generated timezone source must be writable"));
    }

    source.push_str("};\n");

    fs::write(destination, source)
        .unwrap_or_else(|error| panic!("could not write embedded timezone source: {error}"));
}
