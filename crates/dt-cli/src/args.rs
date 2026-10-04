//! Hand-rolled argument parsing (no clap: binary size matters).

use std::collections::BTreeMap;
use std::path::PathBuf;

pub enum CliError {
    /// Bad invocation: exit code 2.
    Usage(String),
    /// Anything else: exit code 1.
    Fail(anyhow::Error),
}

impl<E: Into<anyhow::Error>> From<E> for CliError {
    fn from(e: E) -> Self {
        CliError::Fail(e.into())
    }
}

impl CliError {
    pub fn message(&self) -> String {
        match self {
            CliError::Usage(msg) => msg.clone(),
            CliError::Fail(e) => format!("{e:#}"),
        }
    }
}

pub type CliResult = Result<(), CliError>;

pub fn usage(msg: impl Into<String>) -> CliError {
    CliError::Usage(msg.into())
}

pub fn fail(msg: impl Into<String>) -> CliError {
    CliError::Fail(anyhow::anyhow!(msg.into()))
}

/// Parsed command arguments. Flags are declared per command; anything undeclared is a usage error.
#[derive(Debug, Default)]
pub struct Args {
    pub pos: Vec<String>,
    values: BTreeMap<&'static str, String>,
    switches: Vec<&'static str>,
    /// Everything after a bare `--`.
    pub rest: Vec<String>,
}

impl Args {
    pub fn parse(
        argv: &[String],
        values: &[&'static str],
        switches: &[&'static str],
    ) -> Result<Args, CliError> {
        let mut args = Args::default();
        let mut iter = argv.iter();
        while let Some(arg) = iter.next() {
            if arg == "--" {
                args.rest = iter.cloned().collect();
                break;
            }
            let Some(flag) = arg.strip_prefix("--") else {
                args.pos.push(arg.clone());
                continue;
            };
            let (name, inline) = match flag.split_once('=') {
                Some((name, value)) => (name, Some(value.to_string())),
                None => (flag, None),
            };
            if let Some(&name) = values.iter().find(|v| **v == name) {
                let value = match inline {
                    Some(value) => value,
                    None => iter
                        .next()
                        .cloned()
                        .ok_or_else(|| usage(format!("--{name} needs a value")))?,
                };
                args.values.insert(name, value);
            } else if let Some(&name) = switches.iter().find(|s| **s == name) {
                if inline.is_some() {
                    return Err(usage(format!("--{name} takes no value")));
                }
                args.switches.push(name);
            } else {
                return Err(usage(format!("unknown option --{name}")));
            }
        }
        Ok(args)
    }

    pub fn value(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    pub fn required(&self, name: &str) -> Result<&str, CliError> {
        self.value(name)
            .ok_or_else(|| usage(format!("--{name} is required")))
    }

    pub fn path(&self, name: &str) -> Result<PathBuf, CliError> {
        self.required(name).map(PathBuf::from)
    }

    pub fn switch(&self, name: &str) -> bool {
        self.switches.contains(&name)
    }

    /// Exactly `N` positionals.
    pub fn positionals<const N: usize>(&self, what: &str) -> Result<[&str; N], CliError> {
        let items: Vec<&str> = self.pos.iter().map(String::as_str).collect();
        items
            .try_into()
            .map_err(|_| usage(format!("expected {what}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_values_switches_positionals_and_rest() {
        let args = Args::parse(
            &argv(&[
                "a",
                "--profile",
                "p.toml",
                "--bridge=netcon:1",
                "--yes",
                "b",
                "--",
                "-x",
            ]),
            &["profile", "bridge"],
            &["yes"],
        )
        .ok()
        .unwrap();
        assert_eq!(args.pos, ["a", "b"]);
        assert_eq!(args.value("profile"), Some("p.toml"));
        assert_eq!(args.value("bridge"), Some("netcon:1"));
        assert!(args.switch("yes"));
        assert_eq!(args.rest, ["-x"]);
    }

    #[test]
    fn rejects_unknown_and_malformed_flags() {
        let err = |items: &[&str]| match Args::parse(&argv(items), &["profile"], &["yes"]) {
            Err(CliError::Usage(msg)) => msg,
            _ => panic!("{items:?} should be a usage error"),
        };
        assert!(err(&["--nope"]).contains("--nope"));
        assert!(err(&["--profile"]).contains("needs a value"));
        assert!(err(&["--yes=1"]).contains("takes no value"));
    }
}
