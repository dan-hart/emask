// SPDX-License-Identifier: AGPL-3.0-or-later
//! The portable config file: one TOML document holding every provider and
//! its credentials. Copy it to another machine and `emask` just works there.
//!
//! Location (first match wins): `--config FILE`, `$EMASK_CONFIG`,
//! `~/.config/emask/config.toml` (macOS: `~/Library/Application Support/emask/config.toml`
//! is *not* used — we follow XDG on every platform for portability).

use crate::providers::Kind;
use crate::secret::Secret;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const ENV_CONFIG: &str = "EMASK_CONFIG";
pub const CURRENT_VERSION: u32 = 1;

const HEADER: &str = "\
# emask configuration — https://github.com/dan-hart/emask
# Contains secrets: keep it private (emask writes it with mode 0600).
# Portable: copy this file to ~/.config/emask/config.toml on another machine.

";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "current_version")]
    pub version: u32,
    /// Provider used when `emask` is run without a provider argument.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderConfig>,
}

fn current_version() -> u32 {
    CURRENT_VERSION
}

impl Default for Config {
    fn default() -> Self {
        Config {
            version: CURRENT_VERSION,
            default: None,
            providers: BTreeMap::new(),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub kind: Kind,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<Secret>,
    /// Fastmail: JMAP account id, discovered and cached automatically.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    /// DuckDuckGo: the Duck Address username used to log in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Default description/label for new addresses (Fastmail).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

fn yes() -> bool {
    true
}

impl std::fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("kind", &self.kind)
            .field("enabled", &self.enabled)
            .field("token", &self.token) // Secret's Debug is redacted
            .field("account_id", &self.account_id)
            .field("username", &self.username)
            .field("description", &self.description)
            .finish()
    }
}

impl ProviderConfig {
    pub fn new(kind: Kind) -> Self {
        ProviderConfig {
            kind,
            enabled: true,
            token: None,
            account_id: None,
            username: None,
            description: None,
        }
    }

    pub fn has_token(&self) -> bool {
        self.token.as_ref().is_some_and(|t| !t.is_empty())
    }

    /// Store a new token; anything derived from the old one is forgotten.
    pub fn set_token(&mut self, token: Secret) {
        self.token = Some(token);
        self.account_id = None;
    }
}

impl Config {
    /// Resolve the config file path from the CLI flag, the environment, or the default.
    pub fn path(explicit: Option<&Path>) -> Result<PathBuf> {
        if let Some(p) = explicit {
            return Ok(p.to_path_buf());
        }
        if let Some(p) = std::env::var_os(ENV_CONFIG).filter(|p| !p.is_empty()) {
            return Ok(PathBuf::from(p));
        }
        let home =
            dirs::home_dir().ok_or_else(|| anyhow!("cannot determine your home directory"))?;
        Ok(home.join(".config").join("emask").join("config.toml"))
    }

    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Config::default());
        }
        warn_if_permissive(path);
        let text =
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let config: Config = toml::from_str(text)?;
        if config.version > CURRENT_VERSION {
            bail!("config version {} is newer than this emask understands ({CURRENT_VERSION}); upgrade emask", config.version);
        }
        Ok(config)
    }

    pub fn to_toml(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = format!("{HEADER}{}", self.to_toml()?);
        write_private(path, &text).with_context(|| format!("writing {}", path.display()))
    }

    /// A copy safe to print: tokens reduced to their first and last characters.
    pub fn redacted(&self) -> Config {
        let mut copy = self.clone();
        for p in copy.providers.values_mut() {
            if let Some(t) = &p.token {
                p.token = Some(Secret::new(t.redacted()));
            }
        }
        copy
    }

    /// Turn what the user typed (a provider name, a kind alias, or nothing)
    /// into the name of exactly one configured provider.
    pub fn resolve_name(&self, wanted: Option<&str>) -> Result<String> {
        match wanted {
            Some(n) => {
                let n = n.trim();
                if self.providers.contains_key(n) {
                    return Ok(n.to_string());
                }
                if let Some(kind) = Kind::from_alias(n) {
                    let mut of_kind: Vec<&String> = self
                        .providers
                        .iter()
                        .filter(|(_, p)| p.kind == kind)
                        .map(|(k, _)| k)
                        .collect();
                    if of_kind.len() > 1 {
                        let enabled: Vec<&String> = self
                            .providers
                            .iter()
                            .filter(|(_, p)| p.kind == kind && p.enabled)
                            .map(|(k, _)| k)
                            .collect();
                        if enabled.len() == 1 {
                            of_kind = enabled;
                        }
                    }
                    return match of_kind.as_slice() {
                        [one] => Ok((*one).clone()),
                        [] => bail!("no {kind} provider configured. Add one with:\n  {}", kind.setup_hint(n)),
                        many => bail!(
                            "'{n}' is ambiguous: {} providers use {kind} ({}). Use one of those names.",
                            many.len(),
                            join(many)
                        ),
                    };
                }
                bail!("unknown provider '{n}'.{}", self.known_hint())
            }
            None => {
                if let Some(d) = &self.default {
                    if self.providers.contains_key(d) {
                        return Ok(d.clone());
                    }
                    bail!("default provider '{d}' is not in the config. Fix it with `emask default <PROVIDER>`");
                }
                let enabled: Vec<&String> = self
                    .providers
                    .iter()
                    .filter(|(_, p)| p.enabled)
                    .map(|(k, _)| k)
                    .collect();
                match enabled.as_slice() {
                    [one] => Ok((*one).clone()),
                    [] => bail!("no enabled providers.{}", self.known_hint()),
                    many => bail!(
                        "several providers are enabled ({}). Name one, e.g. `emask {}`, or set a default with `emask default <PROVIDER>`",
                        join(many),
                        many[0]
                    ),
                }
            }
        }
    }

    fn known_hint(&self) -> String {
        if self.providers.is_empty() {
            "\nNothing is configured yet. Start with:\n  emask auth set fm --token <TOKEN>       # Fastmail\n  emask auth login ddg --user <NAME>      # DuckDuckGo".to_string()
        } else {
            format!(
                " Configured: {}. Kind aliases: fm/fastmail, ddg/duckduckgo.",
                join(&self.providers.keys().collect::<Vec<_>>())
            )
        }
    }
}

fn join(names: &[&String]) -> String {
    names
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The config holds tokens; complain (once, on stderr) if other users could read it.
fn warn_if_permissive(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(path) {
            let mode = meta.permissions().mode() & 0o777;
            if mode & 0o077 != 0 {
                eprintln!(
                    "emask: warning: {} is readable by others (mode {mode:o}); fix with: chmod 600 '{}'",
                    path.display(),
                    path.display()
                );
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

/// Write `text` to `path` readable only by the current user (0600 on Unix),
/// creating the parent directory (0700) if needed.
pub fn write_private(path: &Path, text: &str) -> Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        if !dir.exists() {
            fs::create_dir_all(dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
            }
        }
    }
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(path)?;
    file.write_all(text.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
        version = 1
        default = "fm"
        [providers.fm]
        kind = "fastmail"
        token = "fmu1-abcdefghijklmnop"
        account_id = "uabc"
        [providers.ddg]
        kind = "duckduckgo"
        enabled = false
        token = "ddgtokenxyz123456"
    "#;

    #[test]
    fn parses_and_roundtrips() {
        let cfg = Config::parse(SAMPLE).unwrap();
        assert_eq!(cfg.default.as_deref(), Some("fm"));
        assert_eq!(cfg.providers["fm"].kind, Kind::Fastmail);
        assert!(cfg.providers["fm"].enabled, "enabled defaults to true");
        assert!(!cfg.providers["ddg"].enabled);
        let again = Config::parse(&cfg.to_toml().unwrap()).unwrap();
        assert_eq!(again.providers, cfg.providers);
    }

    #[test]
    fn resolves_name_alias_and_default() {
        let cfg = Config::parse(SAMPLE).unwrap();
        assert_eq!(cfg.resolve_name(Some("fm")).unwrap(), "fm");
        assert_eq!(cfg.resolve_name(Some("fastmail")).unwrap(), "fm");
        assert_eq!(cfg.resolve_name(Some("DDG")).unwrap(), "ddg");
        assert_eq!(cfg.resolve_name(None).unwrap(), "fm");
        assert!(cfg
            .resolve_name(Some("gmail"))
            .unwrap_err()
            .to_string()
            .contains("unknown provider"));
    }

    #[test]
    fn alias_prefers_the_single_enabled_provider() {
        let mut cfg = Config::parse(SAMPLE).unwrap();
        let mut second = ProviderConfig::new(Kind::Fastmail);
        second.enabled = false;
        cfg.providers.insert("fm-work".into(), second);
        assert_eq!(cfg.resolve_name(Some("fastmail")).unwrap(), "fm");
    }

    #[test]
    fn no_default_and_many_enabled_is_an_error() {
        let mut cfg = Config::parse(SAMPLE).unwrap();
        cfg.default = None;
        cfg.providers.get_mut("ddg").unwrap().enabled = true;
        let err = cfg.resolve_name(None).unwrap_err().to_string();
        assert!(err.contains("several providers"), "{err}");
    }

    #[test]
    fn redaction_hides_tokens() {
        let cfg = Config::parse(SAMPLE).unwrap().redacted();
        let t = cfg.providers["fm"].token.clone().unwrap();
        let t = t.expose();
        assert!(t.starts_with("fmu1") && t.ends_with("mnop") && t.contains('…'));
    }

    #[test]
    fn empty_config_gives_setup_hint() {
        let cfg = Config::default();
        let err = cfg.resolve_name(None).unwrap_err().to_string();
        assert!(err.contains("emask auth set fm"), "{err}");
    }
}
