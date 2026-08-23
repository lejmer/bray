use std::borrow::Cow;
use std::env;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

const DATE_DIGEST: &str = "63373dc7f9e9802a7a9d2b6705e86674034f9716d7aa07f9d7d00d3fbf1ead00";
const TZDATA_DIGEST: &str = "9109885cd793b03fa905cc5e737498c393d590102359e893b8f3b3c5b6f9102a";

const PROVIDER_SHARED_FILES: &[&str] = &["include/bray_temporal.h", "src/provider.h"];

const PROVIDER_PARTITIONS: &[(&str, &str)] = &[
    ("civil_date", "src/civil.cpp"),
    ("parse_format", "src/text.cpp"),
    ("named_timezone", "src/timezone.cpp"),
];

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

    if env::var_os("CARGO_FEATURE_TEMPORAL").is_some() {
        build_temporal(&manifest);
    }
}

fn build_temporal(manifest: &Path) {
    let third_party = manifest.join("../../third-party/temporal");
    let dynamic = manifest.join("native/dynamic");
    let provider = manifest.join("native/temporal");
    let date = third_party.join("date");
    let tzdata = third_party.join("tzdata");
    let provenance_file = third_party.join("provenance.json");
    let provenance = read_provenance(&provenance_file);

    verify_files(&date, DATE_FILES, DATE_DIGEST, "date provider");
    verify_files(&tzdata, TZDATA_FILES, TZDATA_DIGEST, "timezone database");
    verify_provider(&provider, &provenance.provider);

    let out = PathBuf::from(
        env::var_os("OUT_DIR").unwrap_or_else(|| panic!("Cargo must provide OUT_DIR")),
    );

    let embedded = out.join("bray_tzdata.inc");

    write_embedded_tzdata(&tzdata, &embedded);

    let mut native = cc::Build::new();

    native
        .cpp(true)
        .std("c++17")
        .include(date.join("include"))
        .include(dynamic.join("include"))
        .include(provider.join("include"))
        .include(provider.join("src"))
        .include(&out)
        .define("AUTO_DOWNLOAD", "0")
        .define("HAS_REMOTE_API", "0")
        .define("USE_OS_TZDB", "0")
        .define("ONLY_C_LOCALE", "1")
        .define("NOMINMAX", None)
        .file(date.join("src/tz.cpp"))
        .file(dynamic.join("src/provider.cpp"))
        .warnings(false);

    for partition in &provenance.provider.capability_partitions {
        native.file(provider.join(&partition.translation_unit));
    }

    configure_discardable_sections(&mut native);
    native.compile("bray_native_providers");

    if env::var("CARGO_CFG_TARGET_OS").is_ok_and(|target| target == "windows") {
        println!("cargo:rustc-link-lib=shell32");
        println!("cargo:rustc-link-lib=ole32");
    }

    if env::var("CARGO_CFG_TARGET_OS").is_ok_and(|target| target == "linux") {
        println!("cargo:rustc-link-lib=dl");
    }

    println!("cargo:rerun-if-changed={}", date.display());
    println!("cargo:rerun-if-changed={}", tzdata.display());
    println!("cargo:rerun-if-changed={}", provenance_file.display());

    println!("cargo:rerun-if-changed={}", provider.display());
    println!("cargo:rerun-if-changed={}", dynamic.display());
}

#[derive(Deserialize)]
struct TemporalProvenance {
    provider: ProviderProvenance,
}

#[derive(Deserialize)]
struct ProviderProvenance {
    source_files_sha256: String,
    shared_sources: Vec<String>,
    capability_partitions: Vec<ProviderPartition>,
}

#[derive(Deserialize)]
struct ProviderPartition {
    capability: String,
    translation_unit: String,
}

fn read_provenance(path: &Path) -> TemporalProvenance {
    let bytes = fs::read(path).unwrap_or_else(|error| {
        panic!(
            "could not read temporal provenance {}: {error}",
            path.display()
        )
    });

    serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!(
            "could not decode temporal provenance {}: {error}",
            path.display()
        )
    })
}

fn verify_provider(root: &Path, provenance: &ProviderProvenance) {
    assert_eq!(
        provenance.shared_sources, PROVIDER_SHARED_FILES,
        "temporal provider shared-source provenance is not canonical"
    );

    assert_eq!(
        provenance.capability_partitions.len(),
        PROVIDER_PARTITIONS.len(),
        "temporal provider partition provenance is incomplete"
    );

    for (actual, expected) in provenance
        .capability_partitions
        .iter()
        .zip(PROVIDER_PARTITIONS)
    {
        assert_eq!(
            (actual.capability.as_str(), actual.translation_unit.as_str()),
            *expected,
            "temporal provider partition provenance is not canonical"
        );
    }

    let files = provenance
        .shared_sources
        .iter()
        .chain(
            provenance
                .capability_partitions
                .iter()
                .map(|partition| &partition.translation_unit),
        )
        .map(String::as_str)
        .collect::<Vec<_>>();

    verify_files(
        root,
        &files,
        &provenance.source_files_sha256,
        "temporal provider",
    );
}

fn configure_discardable_sections(native: &mut cc::Build) {
    if env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|environment| environment == "msvc") {
        native.flags(["/EHsc", "/Gy", "/Gw"]);
    } else {
        native.flags(["-ffunction-sections", "-fdata-sections"]);
    }
}

fn verify_files(root: &Path, files: &[&str], expected: &str, name: &str) {
    let mut hasher = Sha256::new();

    for file in files {
        let path = root.join(file);

        let bytes = fs::read(&path).unwrap_or_else(|error| {
            panic!("could not read pinned {name} {}: {error}", path.display())
        });

        let bytes = canonical_text(&bytes);

        hasher.update(file.as_bytes());
        hasher.update([0]);

        hasher.update(
            u64::try_from(bytes.len())
                .unwrap_or_else(|_| panic!("pinned {name} file is too large"))
                .to_le_bytes(),
        );

        hasher.update(bytes);
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

        let bytes = canonical_text(&bytes);

        write!(
            source,
            "static constexpr unsigned char BRAY_TZDATA_{index}[] = {{"
        )
        .unwrap_or_else(|_| panic!("generated timezone source must be writable"));

        for &byte in bytes.iter() {
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

fn canonical_text(bytes: &[u8]) -> Cow<'_, [u8]> {
    if !bytes.windows(2).any(|pair| pair == b"\r\n") {
        return Cow::Borrowed(bytes);
    }

    let mut normalized = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index..].starts_with(b"\r\n") {
            normalized.push(b'\n');
            index += 2;
        } else {
            normalized.push(bytes[index]);
            index += 1;
        }
    }

    Cow::Owned(normalized)
}
