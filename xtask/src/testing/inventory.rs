use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use syn::visit::Visit;

use super::options::{Options, Tier};

#[derive(Serialize)]
pub(super) struct SelectedTest {
    pub(super) binary: String,
    pub(super) executable: std::path::PathBuf,
    pub(super) name: String,
    pub(super) tier: String,
}

pub(super) fn inventory(root: &Path) -> Result<Vec<SelectedTest>, String> {
    let mut build = std::process::Command::new("cargo");

    build.current_dir(root).args([
        "test",
        "-p",
        "bray-compilation",
        "--no-run",
        "--message-format=json",
    ]);

    let output = crate::command::require_success(build, "compiler test preparation")?;
    let tags = tier_tags(root)?;
    let mut tests = Vec::new();

    for line in output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let artifact: serde_json::Value = serde_json::from_slice(line)
            .map_err(|error| format!("invalid Cargo artifact: {error}"))?;

        if artifact["reason"] != "compiler-artifact" || artifact["profile"]["test"] != true {
            continue;
        }

        let Some(executable) = artifact["executable"].as_str() else {
            continue;
        };

        let binary = artifact["target"]["name"]
            .as_str()
            .ok_or("test artifact has no target name")?;

        let all = list(root, executable, false)?;
        let ignored = list(root, executable, true)?;

        for name in all {
            let tier = if ignored.contains(&name) {
                let short_name = name.rsplit("::").next().expect("test has a name");

                tags.get(short_name)
                    .copied()
                    .ok_or_else(|| format!("ignored compiler test {name} needs a tier tag"))?
            } else {
                Tier::Fast
            };

            tests.push(SelectedTest {
                binary: binary.to_owned(),
                executable: executable.into(),
                name,
                tier: tier.name().to_owned(),
            });
        }
    }

    tests.sort_by(|left, right| (&left.binary, &left.name).cmp(&(&right.binary, &right.name)));

    Ok(tests)
}

fn list(
    root: &Path,
    executable: &str,
    ignored: bool,
) -> Result<std::collections::BTreeSet<String>, String> {
    let mut command = std::process::Command::new(executable);

    command
        .current_dir(root.join("crates/bray-compilation"))
        .args(["--list", "--format", "terse"]);

    if ignored {
        command.arg("--ignored");
    }

    let output = crate::command::require_success(command, "compiler test discovery")?;

    let text =
        String::from_utf8(output.stdout).map_err(|error| format!("invalid test list: {error}"))?;

    Ok(text
        .lines()
        .filter_map(|line| line.strip_suffix(": test").map(str::to_owned))
        .collect())
}

pub(super) fn select(tests: Vec<SelectedTest>, options: &Options) -> Vec<SelectedTest> {
    tests
        .into_iter()
        .filter(|test| options.tier == Tier::All || test.tier == options.tier.name())
        .enumerate()
        .filter_map(|(index, test)| (index % options.shards == options.shard - 1).then_some(test))
        .collect()
}

// A bounded argument batch works on Windows as well as Unix. Each batch retains fixture caches.
pub(super) fn batches(tests: &[SelectedTest]) -> Vec<&[SelectedTest]> {
    let mut batches = Vec::new();
    let mut start = 0;
    let mut bytes = 0;

    for (index, test) in tests.iter().enumerate() {
        if index > start
            && (index - start == 16
                || tests[start].executable != test.executable
                || bytes + test.name.encode_utf16().count() + 3 > 24_000)
        {
            batches.push(&tests[start..index]);
            start = index;
            bytes = 0;
        }

        bytes += test.name.encode_utf16().count() + 3;
    }

    if start < tests.len() {
        batches.push(&tests[start..]);
    }

    batches
}

pub(super) fn tier_tags(root: &Path) -> Result<BTreeMap<String, Tier>, String> {
    let mut files = Vec::new();

    crate::workspace::collect_rust_files(&root.join("crates/bray-compilation"), &mut files)?;

    let mut tags = Tags::default();

    for path in files {
        let source = std::fs::read_to_string(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;

        let parsed = syn::parse_file(&source)
            .map_err(|error| format!("could not parse {}: {error}", path.display()))?;

        tags.visit_file(&parsed);
    }

    match tags.error {
        Some(error) => Err(error),
        None => Ok(tags.values),
    }
}

#[derive(Default)]
struct Tags {
    values: BTreeMap<String, Tier>,
    error: Option<String>,
}

impl<'ast> Visit<'ast> for Tags {
    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        for attribute in &function.attrs {
            if !attribute.path().is_ident("ignore") {
                continue;
            }

            let result = (|| {
                let syn::Meta::NameValue(value) = &attribute.meta else {
                    return Err("compiler ignore needs a tier reason".to_owned());
                };

                let syn::Expr::Lit(value) = &value.value else {
                    return Err("compiler tier must be a literal".to_owned());
                };

                let syn::Lit::Str(value) = &value.lit else {
                    return Err("compiler tier must be a string".to_owned());
                };

                let reason = value.value();

                let name = reason
                    .strip_prefix("tier:")
                    .ok_or("compiler ignore needs a tier: prefix")?;

                let tier = Tier::parse(name)?;

                if matches!(tier, Tier::Fast | Tier::All) {
                    return Err(
                        "ignored tests must belong to native, proof, or exhaustive".to_owned()
                    );
                }

                let name = function.sig.ident.to_string();

                if self.values.insert(name.clone(), tier).is_some() {
                    return Err(format!("ambiguous compiler tier tag: {name}"));
                }

                Ok(())
            })();

            if let Err(error) = result {
                self.error = Some(error);
            }
        }

        syn::visit::visit_item_fn(self, function);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{Options, SelectedTest, Tier, batches, select};

    fn fixture() -> Vec<SelectedTest> {
        ["a", "b", "c", "d"]
            .map(|name| SelectedTest {
                binary: "compiler".to_owned(),
                executable: "compiler".into(),
                name: name.to_owned(),
                tier: if name == "c" { "proof" } else { "fast" }.to_owned(),
            })
            .into_iter()
            .collect()
    }

    #[test]
    fn shards_cover_the_inventory_once_in_stable_order() {
        let mut names = BTreeSet::new();

        for shard in 1..=3 {
            for test in select(
                fixture(),
                &Options {
                    tier: Tier::All,
                    shard,
                    shards: 3,
                    ..Options::default()
                },
            ) {
                assert!(names.insert(test.name));
            }
        }

        assert_eq!(
            names,
            ["a", "b", "c", "d"]
                .map(str::to_owned)
                .into_iter()
                .collect()
        );

        assert_eq!(
            select(
                fixture(),
                &Options {
                    tier: Tier::Proof,
                    shard: 1,
                    shards: 1,
                    ..Options::default()
                }
            )
            .len(),
            1
        );
    }

    #[test]
    fn argument_batches_keep_every_test_and_separate_binaries() {
        let mut tests = fixture();

        tests[2].executable = "other".into();

        let groups = batches(&tests);

        assert_eq!(
            groups.iter().map(|group| group.len()).collect::<Vec<_>>(),
            [2, 1, 1]
        );

        assert_eq!(
            groups.iter().map(|group| group.len()).sum::<usize>(),
            tests.len()
        );

        let many: Vec<_> = (0..200)
            .map(|index| SelectedTest {
                binary: "compiler".to_owned(),
                executable: "compiler".into(),
                name: format!("case_{index}"),
                tier: "fast".to_owned(),
            })
            .collect();

        let groups = batches(&many);

        assert!(groups.iter().all(|group| group.len() <= 16));

        assert_eq!(
            groups
                .into_iter()
                .flatten()
                .map(|test| &test.name)
                .collect::<Vec<_>>(),
            many.iter().map(|test| &test.name).collect::<Vec<_>>()
        );
    }
}
