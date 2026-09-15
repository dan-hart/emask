// SPDX-License-Identifier: AGPL-3.0-or-later
//! Provider registry. Each provider knows how to turn a token into a fresh
//! masked address. Adding a provider means: a new [`Kind`] variant, a module,
//! and two match arms below.

pub mod duckduckgo;
pub mod fastmail;

use crate::config::ProviderConfig;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

/// The service behind a configured provider entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Fastmail Masked Email (JMAP)
    #[value(name = "fastmail", alias = "fm")]
    Fastmail,
    /// DuckDuckGo Email Protection (@duck.com)
    #[value(name = "duckduckgo", alias = "ddg")]
    DuckDuckGo,
}

impl Kind {
    pub const ALL: [Kind; 2] = [Kind::Fastmail, Kind::DuckDuckGo];

    /// Accepts the kind name or any of its short aliases, case-insensitively.
    pub fn from_alias(s: &str) -> Option<Kind> {
        let s = s.trim().to_ascii_lowercase();
        Kind::ALL
            .into_iter()
            .find(|k| k.aliases().contains(&s.as_str()))
    }

    pub fn aliases(self) -> &'static [&'static str] {
        match self {
            Kind::Fastmail => &["fastmail", "fm"],
            Kind::DuckDuckGo => &["duckduckgo", "ddg", "duck"],
        }
    }

    /// Where to get credentials for this kind — shown whenever a token is missing.
    pub fn token_help(self) -> &'static str {
        match self {
            Kind::Fastmail => "Create an API token at https://app.fastmail.com/settings/security/tokens/new with only the \"Masked Email\" scope.",
            Kind::DuckDuckGo => "Run `emask auth login <NAME> --user <duck-username>` to log in with a one-time passphrase (or paste the access token from the DuckDuckGo browser's Email Protection settings).",
        }
    }

    pub fn setup_hint(self, name: &str) -> String {
        match self {
            Kind::Fastmail => format!("emask auth set {name} --token <TOKEN>"),
            Kind::DuckDuckGo => format!("emask auth login {name} --user <duck-username>"),
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Kind::Fastmail => "Fastmail",
            Kind::DuckDuckGo => "DuckDuckGo",
        })
    }
}

/// User-supplied details for a new address. Providers use what they support.
#[derive(Debug, Default, Clone)]
pub struct CreateOptions {
    pub site: Option<String>,
    pub description: Option<String>,
    pub prefix: Option<String>,
}

/// A freshly minted address plus where it came from.
#[derive(Debug, Clone, Serialize)]
pub struct Mask {
    pub email: String,
    pub provider: String,
    pub kind: Kind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Create a mask. `cfg` may be updated (Fastmail caches its account id); the
/// caller decides whether to persist it.
pub fn create(name: &str, cfg: &mut ProviderConfig, opts: &CreateOptions) -> Result<Mask> {
    if !cfg.enabled {
        bail!("provider '{name}' is disabled. Enable it with `emask enable {name}`");
    }
    match cfg.kind {
        Kind::Fastmail => fastmail::create(name, cfg, opts),
        Kind::DuckDuckGo => duckduckgo::create(name, cfg, opts),
    }
}

/// Verify credentials without creating anything (where the API allows it).
pub fn check(name: &str, cfg: &mut ProviderConfig) -> Result<String> {
    match cfg.kind {
        Kind::Fastmail => fastmail::check(name, cfg),
        Kind::DuckDuckGo => duckduckgo::check(name, cfg),
    }
}

pub fn require_token<'a>(name: &str, cfg: &'a ProviderConfig) -> Result<&'a str> {
    match cfg
        .token
        .as_ref()
        .filter(|t| !t.is_empty())
        .map(|t| t.expose().trim())
    {
        Some(t) => Ok(t),
        None => bail!(
            "provider '{name}' has no token.\n{}\nThen run: emask auth set {name} --token <TOKEN>",
            cfg.kind.token_help()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_resolve_case_insensitively() {
        assert_eq!(Kind::from_alias("FM"), Some(Kind::Fastmail));
        assert_eq!(Kind::from_alias("fastmail"), Some(Kind::Fastmail));
        assert_eq!(Kind::from_alias("ddg"), Some(Kind::DuckDuckGo));
        assert_eq!(Kind::from_alias(" duck "), Some(Kind::DuckDuckGo));
        assert_eq!(Kind::from_alias("gmail"), None);
    }
}
