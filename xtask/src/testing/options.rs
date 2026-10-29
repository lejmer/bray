use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Tier {
    Fast,
    Native,
    Proof,
    Exhaustive,
    All,
}

impl Tier {
    pub(super) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "fast" => Ok(Self::Fast),
            "native" => Ok(Self::Native),
            "proof" => Ok(Self::Proof),
            "exhaustive" => Ok(Self::Exhaustive),
            "all" => Ok(Self::All),
            _ => Err(format!("unknown compiler test tier: {value}")),
        }
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Native => "native",
            Self::Proof => "proof",
            Self::Exhaustive => "exhaustive",
            Self::All => "all",
        }
    }

    pub(super) fn reference_target(self) -> Duration {
        Duration::from_secs(match self {
            Self::Fast => 120,
            Self::Native => 300,
            Self::Proof | Self::Exhaustive | Self::All => 600,
        })
    }
}

pub(super) struct Options {
    pub(super) tier: Tier,
    pub(super) shard: usize,
    pub(super) shards: usize,
    pub(super) timeout: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            tier: Tier::Fast,
            shard: 1,
            shards: 1,
            timeout: Duration::from_secs(1800),
        }
    }
}

impl Options {
    pub(super) fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Self, String> {
        if arguments.next().as_deref() != Some("compiler") {
            return Err("usage: cargo xtask test compiler [--tier fast|native|proof|exhaustive|all] [--shard index/count] [--timeout-seconds seconds]".to_owned());
        }

        let mut options = Self::default();

        while let Some(argument) = arguments.next() {
            let value = arguments
                .next()
                .ok_or_else(|| format!("missing value for {argument}"))?;

            match argument.as_str() {
                "--tier" => options.tier = Tier::parse(&value)?,
                "--timeout-seconds" => {
                    let seconds: u32 = value
                        .parse()
                        .map_err(|_| "timeout must be a positive integer in seconds")?;

                    if seconds == 0 {
                        return Err("timeout must be greater than zero".to_owned());
                    }

                    options.timeout = Duration::from_secs(u64::from(seconds));
                }
                "--shard" => {
                    let (index, count) =
                        value.split_once('/').ok_or("shard must be index/count")?;

                    options.shard = index.parse().map_err(|_| "invalid shard index")?;
                    options.shards = count.parse().map_err(|_| "invalid shard count")?;

                    if options.shard == 0 || options.shards == 0 || options.shard > options.shards {
                        return Err("shard must satisfy 1 <= index <= count".to_owned());
                    }
                }
                _ => return Err(format!("unexpected argument: {argument}")),
            }
        }

        Ok(options)
    }
}

#[cfg(test)]
mod tests {
    use super::{Options, Tier};

    #[test]
    fn rejects_invalid_shards_and_tiers() {
        for arguments in [
            vec!["compiler", "--shard", "0/4"],
            vec!["compiler", "--shard", "5/4"],
            vec!["compiler", "--shard", "1/0"],
            vec!["compiler", "--tier", "unknown"],
            vec!["compiler", "--timeout-seconds", "0"],
            vec!["compiler", "--timeout-seconds", "-1"],
            vec!["compiler", "--timeout-seconds", "4294967296"],
        ] {
            assert!(Options::parse(arguments.into_iter().map(str::to_owned)).is_err());
        }

        let options = Options::parse(
            [
                "compiler",
                "--tier",
                "proof",
                "--shard",
                "2/4",
                "--timeout-seconds",
                "90",
            ]
            .map(str::to_owned)
            .into_iter(),
        )
        .expect("valid options");

        assert_eq!(
            (options.tier, options.shard, options.shards),
            (Tier::Proof, 2, 4)
        );

        assert_eq!(options.timeout, std::time::Duration::from_secs(90));
    }
}
