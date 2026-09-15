// SPDX-License-Identifier: AGPL-3.0-or-later
//! Command surface. `emask <provider>` is the hot path; everything else is
//! setup and housekeeping. All secrets enter through [`read_secret`].

use crate::clipboard;
use crate::config::{write_private, Config, ProviderConfig};
use crate::http;
use crate::providers::{self, duckduckgo, CreateOptions, Kind};
use crate::secret::Secret;
use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, CommandFactory, Parser, Subcommand};
use serde_json::json;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use zeroize::Zeroize;

const LONG_ABOUT: &str = "\
emask mints a fresh masked (alias) email address from a provider you have
configured, prints it, and copies it to your clipboard. One short command,
no browser, no menus.

Providers are named entries in a single portable TOML file. Use a provider by
its name or by its kind alias (fm/fastmail, ddg/duckduckgo).";

const EXAMPLES: &str = "\
Examples:
  emask fm                       Fastmail masked address, copied to the clipboard
  emask ddg                      DuckDuckGo private address (…@duck.com)
  emask fm --for example.com     Label the address with the site it is for
  emask                          Use the default provider (see `emask default`)
  emask fm --json --no-copy      Machine-readable output, leave the clipboard alone

Setup:
  emask auth set fm              Fastmail API token (prompted, hidden)
  emask auth login ddg --user NAME
                                 DuckDuckGo: one-time passphrase login
  emask providers                What is configured
  emask config export FILE       Move everything to another machine

Config: ~/.config/emask/config.toml  (override with --config or $EMASK_CONFIG)
Docs:   https://github.com/dan-hart/emask";

#[derive(Parser, Debug)]
#[command(
    name = "emask",
    version,
    about = "Mint a masked email address and copy it to your clipboard.",
    long_about = LONG_ABOUT,
    after_help = EXAMPLES,
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true
)]
pub struct Cli {
    /// Provider name or kind alias (fm, fastmail, ddg, duckduckgo). Omit for the default.
    #[arg(value_name = "PROVIDER")]
    provider: Option<String>,

    #[command(flatten)]
    create: CreateArgs,

    /// Config file to use [env: EMASK_CONFIG] [default: ~/.config/emask/config.toml]
    #[arg(long, global = true, value_name = "FILE")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Args, Debug, Clone, Default)]
struct CreateArgs {
    /// Site or domain the address is for (Fastmail stores it; DuckDuckGo cannot)
    #[arg(short = 'f', long = "for", value_name = "SITE")]
    site: Option<String>,

    /// Description / label for the address (Fastmail). Defaults to SITE.
    #[arg(short, long, value_name = "TEXT")]
    desc: Option<String>,

    /// Requested local-part prefix, e.g. "shop" → shop.xxxx@… (Fastmail only)
    #[arg(short, long, value_name = "PREFIX")]
    prefix: Option<String>,

    /// Print the address but do not touch the clipboard
    #[arg(long)]
    no_copy: bool,

    /// Emit JSON ({"email","provider","kind","copied"}) instead of plain text
    #[arg(long)]
    json: bool,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Create a new masked address (same as `emask [PROVIDER]`)
    #[command(after_help = "Examples:\n  emask new fm --for example.com\n  emask new ddg --json")]
    New {
        /// Provider name or kind alias
        #[arg(value_name = "PROVIDER")]
        provider: Option<String>,
        #[command(flatten)]
        create: CreateArgs,
    },

    /// List configured providers and their status
    Providers,

    /// Enable a provider so it can mint addresses
    Enable {
        #[arg(value_name = "PROVIDER")]
        provider: String,
    },

    /// Disable a provider (kept in the config, refused when minting)
    Disable {
        #[arg(value_name = "PROVIDER")]
        provider: String,
    },

    /// Show or set the provider used when none is given
    #[command(
        after_help = "Examples:\n  emask default          Show the current default\n  emask default fm       Use Fastmail when running plain `emask`\n  emask default --clear  Require an explicit provider again"
    )]
    Default {
        #[arg(value_name = "PROVIDER")]
        provider: Option<String>,
        /// Remove the default
        #[arg(long, conflicts_with = "provider")]
        clear: bool,
    },

    /// Add, update, inspect, or remove provider credentials
    Auth {
        #[command(subcommand)]
        cmd: AuthCmd,
    },

    /// Verify a provider's credentials without minting an address
    Check {
        /// Provider to check; omit to check every provider
        #[arg(value_name = "PROVIDER")]
        provider: Option<String>,
    },

    /// Show, export, import, or edit the config file
    Config {
        #[command(subcommand)]
        cmd: ConfigCmd,
    },

    /// Print shell completions (bash, zsh, fish, elvish, powershell)
    #[command(
        after_help = "Examples:\n  emask completions zsh > ~/.zfunc/_emask\n  emask completions bash > /etc/bash_completion.d/emask"
    )]
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand, Debug)]
enum AuthCmd {
    /// Add or update a provider and store its token
    ///
    /// The token is read from a hidden prompt (or stdin when piped). Passing it
    /// with --token works for scripts but leaks into shell history and `ps`;
    /// prefer --token-file or the prompt.
    #[command(
        after_help = "Examples:\n  emask auth set fm                        Prompt for a Fastmail token\n  emask auth set work --kind fastmail      A second Fastmail account named 'work'\n  emask auth set ddg --token-file ~/.ddg   Read the token's first line from a file\n  cat token.txt | emask auth set fm        Read the token from stdin"
    )]
    Set {
        /// Provider name to create or update (fm, ddg, work, …)
        #[arg(value_name = "PROVIDER")]
        provider: String,
        /// Provider kind; inferred from the name when it is a known alias
        #[arg(short, long, value_enum, value_name = "KIND")]
        kind: Option<Kind>,
        /// Token on the command line (discouraged: visible in history and process lists)
        #[arg(long, value_name = "TOKEN", conflicts_with = "token_file")]
        token: Option<String>,
        /// Read the token from the first line of FILE ("-" for stdin)
        #[arg(long, value_name = "FILE")]
        token_file: Option<PathBuf>,
        /// Fastmail JMAP account id (discovered automatically when omitted)
        #[arg(long, value_name = "ID")]
        account_id: Option<String>,
        /// Default description for addresses minted with this provider
        #[arg(long, value_name = "TEXT")]
        description: Option<String>,
        /// Add the provider in a disabled state
        #[arg(long)]
        disabled: bool,
    },

    /// Log in to DuckDuckGo Email Protection with a one-time passphrase
    ///
    /// DuckDuckGo emails a passphrase to the inbox behind <USER>@duck.com;
    /// paste it when prompted and emask stores the resulting access token.
    #[command(after_help = "Examples:\n  emask auth login ddg --user alice")]
    Login {
        /// Provider name to create or update (defaults to a DuckDuckGo provider)
        #[arg(value_name = "PROVIDER", default_value = "ddg")]
        provider: String,
        /// Your Duck Address username (the part before @duck.com)
        #[arg(short, long, value_name = "USER")]
        user: String,
    },

    /// Show a provider's settings (token redacted unless --reveal)
    Show {
        #[arg(value_name = "PROVIDER")]
        provider: String,
        /// Print the full token
        #[arg(long)]
        reveal: bool,
    },

    /// Remove a provider and its token
    Remove {
        #[arg(value_name = "PROVIDER")]
        provider: String,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCmd {
    /// Print the path of the config file in use
    Path,
    /// Print the config (tokens redacted unless --reveal)
    Show {
        /// Include full tokens
        #[arg(long)]
        reveal: bool,
    },
    /// Write the complete config, including tokens, to FILE (or stdout)
    ///
    /// The output is exactly what another machine needs: `emask config import FILE` there.
    Export {
        #[arg(value_name = "FILE")]
        file: Option<PathBuf>,
    },
    /// Load providers from FILE, replacing the current config (or --merge into it)
    Import {
        #[arg(value_name = "FILE")]
        file: PathBuf,
        /// Keep existing providers; entries in FILE win on name clashes
        #[arg(long)]
        merge: bool,
    },
    /// Open the config in $VISUAL / $EDITOR and validate it afterwards
    Edit,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let path = Config::path(cli.config.as_deref())?;

    match cli.command {
        None => create(&path, cli.provider.as_deref(), &cli.create),
        Some(Command::New {
            provider,
            create: args,
        }) => create(&path, provider.as_deref(), &args),
        Some(Command::Providers) => list_providers(&path),
        Some(Command::Enable { provider }) => set_enabled(&path, &provider, true),
        Some(Command::Disable { provider }) => set_enabled(&path, &provider, false),
        Some(Command::Default { provider, clear }) => {
            set_default(&path, provider.as_deref(), clear)
        }
        Some(Command::Auth { cmd }) => auth(&path, cmd),
        Some(Command::Check { provider }) => check(&path, provider.as_deref()),
        Some(Command::Config { cmd }) => config_cmd(&path, cmd),
        Some(Command::Completions { shell }) => {
            clap_complete::generate(shell, &mut Cli::command(), "emask", &mut io::stdout());
            Ok(())
        }
    }
}

// ---------------------------------------------------------------- minting

fn create(path: &Path, provider: Option<&str>, args: &CreateArgs) -> Result<()> {
    let mut config = Config::load(path)?;
    let name = config.resolve_name(provider)?;
    let before_account = config.providers[&name].account_id.clone();

    let opts = CreateOptions {
        site: args.site.clone(),
        description: args.desc.clone(),
        prefix: args.prefix.clone(),
    };
    let mask = {
        let cfg = config
            .providers
            .get_mut(&name)
            .expect("resolved name exists");
        providers::create(&name, cfg, &opts)?
    };
    if config.providers[&name].account_id != before_account {
        config.save(path)?; // cache what we discovered
    }

    let copied = if args.no_copy {
        false
    } else {
        match clipboard::copy(&mask.email) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("emask: warning: could not copy to clipboard: {e:#}");
                false
            }
        }
    };

    if args.json {
        let out = json!({
            "email": mask.email,
            "provider": mask.provider,
            "kind": mask.kind,
            "copied": copied,
            "note": mask.note,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("{}", mask.email);
        if copied {
            eprintln!("copied to clipboard");
        }
        if let Some(note) = &mask.note {
            eprintln!("note: {note}");
        }
    }
    Ok(())
}

// ------------------------------------------------------------- providers

fn list_providers(path: &Path) -> Result<()> {
    let config = Config::load(path)?;
    if config.providers.is_empty() {
        eprintln!("No providers configured ({}).\n", path.display());
        eprintln!("  emask auth set fm                    # Fastmail (prompts for the token)");
        eprintln!("  emask auth login ddg --user <NAME>   # DuckDuckGo");
        return Ok(());
    }
    let width = config
        .providers
        .keys()
        .map(String::len)
        .max()
        .unwrap_or(4)
        .max(4);
    println!(
        "{:<width$}  {:<11}  {:<8}  {:<5}  DEFAULT",
        "NAME", "KIND", "ENABLED", "TOKEN"
    );
    for (name, p) in &config.providers {
        println!(
            "{:<width$}  {:<11}  {:<8}  {:<5}  {}",
            name,
            p.kind.to_string(),
            if p.enabled { "yes" } else { "no" },
            if p.has_token() { "yes" } else { "none" },
            if config.default.as_deref() == Some(name) {
                "*"
            } else {
                ""
            },
        );
    }
    Ok(())
}

fn set_enabled(path: &Path, provider: &str, enabled: bool) -> Result<()> {
    let mut config = Config::load(path)?;
    let name = config.resolve_name(Some(provider))?;
    config.providers.get_mut(&name).expect("resolved").enabled = enabled;
    config.save(path)?;
    eprintln!("{} '{name}'", if enabled { "enabled" } else { "disabled" });
    Ok(())
}

fn set_default(path: &Path, provider: Option<&str>, clear: bool) -> Result<()> {
    let mut config = Config::load(path)?;
    if clear {
        config.default = None;
        config.save(path)?;
        eprintln!("default cleared; `emask` now needs an explicit provider");
        return Ok(());
    }
    match provider {
        None => {
            match &config.default {
                Some(d) => println!("{d}"),
                None => eprintln!("no default set. Set one with `emask default <PROVIDER>`"),
            }
            Ok(())
        }
        Some(p) => {
            let name = config.resolve_name(Some(p))?;
            config.default = Some(name.clone());
            config.save(path)?;
            eprintln!("default provider is now '{name}'");
            Ok(())
        }
    }
}

// -------------------------------------------------------------------- auth

fn auth(path: &Path, cmd: AuthCmd) -> Result<()> {
    match cmd {
        AuthCmd::Set {
            provider,
            kind,
            token,
            token_file,
            account_id,
            description,
            disabled,
        } => {
            let mut config = Config::load(path)?;
            let kind = match (kind, config.providers.get(&provider)) {
                (Some(k), _) => k,
                (None, Some(existing)) => existing.kind,
                (None, None) => Kind::from_alias(&provider).ok_or_else(|| {
                    anyhow!("cannot infer the kind of '{provider}'. Add --kind fastmail or --kind duckduckgo")
                })?,
            };

            let secret = match (token, token_file) {
                (Some(mut raw), _) => {
                    eprintln!("emask: warning: --token is visible in shell history and process lists; prefer the prompt or --token-file");
                    let s = Secret::new(raw.trim());
                    raw.zeroize();
                    s
                }
                (None, Some(file)) => read_secret_file(&file)?,
                (None, None) => {
                    eprintln!("{}", kind.token_help());
                    read_secret(&format!("{kind} token for '{provider}' (hidden): "))?
                }
            };
            if secret.is_empty() {
                bail!("empty token — nothing saved");
            }

            let entry = config
                .providers
                .entry(provider.clone())
                .or_insert_with(|| ProviderConfig::new(kind));
            entry.kind = kind;
            entry.set_token(secret);
            entry.account_id = account_id;
            if description.is_some() {
                entry.description = description;
            }
            if disabled {
                entry.enabled = false;
            }
            if config.default.is_none() && config.providers.len() == 1 {
                config.default = Some(provider.clone());
            }
            config.save(path)?;
            eprintln!("saved provider '{provider}' ({kind}) → {}", path.display());

            if kind == Kind::Fastmail {
                let entry = config.providers.get_mut(&provider).expect("just inserted");
                match providers::check(&provider, entry) {
                    Ok(msg) => {
                        eprintln!("{msg}");
                        config.save(path)?;
                    }
                    Err(e) => {
                        eprintln!("emask: warning: token saved but verification failed: {e:#}")
                    }
                }
            }
            Ok(())
        }

        AuthCmd::Login { provider, user } => {
            let mut config = Config::load(path)?;
            if let Some(existing) = config.providers.get(&provider) {
                if existing.kind != Kind::DuckDuckGo {
                    bail!(
                        "'{provider}' is a {} provider; `auth login` is only for DuckDuckGo",
                        existing.kind
                    );
                }
            } else if matches!(Kind::from_alias(&provider), Some(k) if k != Kind::DuckDuckGo) {
                bail!("'{provider}' names a non-DuckDuckGo kind; pick another provider name");
            }
            let user = user.trim().trim_end_matches("@duck.com").to_string();
            if user.is_empty() {
                bail!("--user must be your Duck Address username");
            }

            let agent = http::agent();
            duckduckgo::request_passphrase(&agent, &user)?;
            eprintln!(
                "DuckDuckGo emailed a one-time passphrase to the inbox behind {user}@duck.com."
            );
            let mut passphrase = read_line("Paste the passphrase (words separated by spaces): ")?;
            if passphrase.trim().is_empty() {
                bail!("no passphrase entered");
            }
            let token = duckduckgo::exchange_passphrase(&agent, &user, &passphrase)?;
            passphrase.zeroize();

            let entry = config
                .providers
                .entry(provider.clone())
                .or_insert_with(|| ProviderConfig::new(Kind::DuckDuckGo));
            entry.kind = Kind::DuckDuckGo;
            entry.set_token(Secret::new(token));
            entry.username = Some(user);
            if config.default.is_none() && config.providers.len() == 1 {
                config.default = Some(provider.clone());
            }
            config.save(path)?;
            eprintln!(
                "logged in; access token saved for '{provider}' → {}",
                path.display()
            );
            Ok(())
        }

        AuthCmd::Show { provider, reveal } => {
            let config = Config::load(path)?;
            let name = config.resolve_name(Some(&provider))?;
            let p = &config.providers[&name];
            println!("name:        {name}");
            println!("kind:        {}", p.kind);
            println!("enabled:     {}", if p.enabled { "yes" } else { "no" });
            println!(
                "default:     {}",
                if config.default.as_deref() == Some(&name) {
                    "yes"
                } else {
                    "no"
                }
            );
            match &p.token {
                Some(t) if reveal => println!("token:       {}", t.expose()),
                Some(t) => println!("token:       {}  (use --reveal to print it)", t.redacted()),
                None => println!("token:       none"),
            }
            if let Some(a) = &p.account_id {
                println!("account_id:  {a}");
            }
            if let Some(u) = &p.username {
                println!("username:    {u}");
            }
            if let Some(d) = &p.description {
                println!("description: {d}");
            }
            Ok(())
        }

        AuthCmd::Remove { provider } => {
            let mut config = Config::load(path)?;
            let name = config.resolve_name(Some(&provider))?;
            config.providers.remove(&name);
            if config.default.as_deref() == Some(&name) {
                config.default = None;
            }
            config.save(path)?;
            eprintln!("removed '{name}'");
            Ok(())
        }
    }
}

fn check(path: &Path, provider: Option<&str>) -> Result<()> {
    let mut config = Config::load(path)?;
    let names: Vec<String> = match provider {
        Some(p) => vec![config.resolve_name(Some(p))?],
        None => config.providers.keys().cloned().collect(),
    };
    if names.is_empty() {
        bail!("no providers configured");
    }
    let snapshot = config.clone();
    let mut failures = 0;
    for name in &names {
        let cfg = config.providers.get_mut(name).expect("listed");
        match providers::check(name, cfg) {
            Ok(msg) => println!("{name}: {msg}"),
            Err(e) => {
                failures += 1;
                println!("{name}: FAILED: {e:#}");
            }
        }
    }
    if config.providers != snapshot.providers {
        config.save(path)?;
    }
    if failures > 0 {
        bail!("{failures} provider(s) failed verification");
    }
    Ok(())
}

// ------------------------------------------------------------------ config

fn config_cmd(path: &Path, cmd: ConfigCmd) -> Result<()> {
    match cmd {
        ConfigCmd::Path => {
            println!("{}", path.display());
            Ok(())
        }
        ConfigCmd::Show { reveal } => {
            let config = Config::load(path)?;
            let shown = if reveal { config } else { config.redacted() };
            println!("# {}", path.display());
            print!("{}", shown.to_toml()?);
            Ok(())
        }
        ConfigCmd::Export { file } => {
            let config = Config::load(path)?;
            let text = config.to_toml()?;
            match file {
                Some(f) => {
                    write_private(&f, &text).with_context(|| format!("writing {}", f.display()))?;
                    eprintln!("exported {} provider(s) to {} (mode 0600). Import elsewhere with: emask config import {}", config.providers.len(), f.display(), f.display());
                }
                None => {
                    if io::stdout().is_terminal() {
                        eprintln!("emask: note: this output contains your tokens");
                    }
                    print!("{text}");
                }
            }
            Ok(())
        }
        ConfigCmd::Import { file, merge } => {
            let text = if file.as_os_str() == "-" {
                let mut s = String::new();
                io::stdin().read_to_string(&mut s)?;
                s
            } else {
                std::fs::read_to_string(&file)
                    .with_context(|| format!("reading {}", file.display()))?
            };
            let incoming =
                Config::parse(&text).with_context(|| format!("parsing {}", file.display()))?;
            if incoming.providers.is_empty() {
                bail!("{} has no [providers.*] entries", file.display());
            }
            let mut config = if merge {
                Config::load(path)?
            } else {
                Config::default()
            };
            let count = incoming.providers.len();
            config.providers.extend(incoming.providers);
            if incoming.default.is_some() || config.default.is_none() {
                config.default = incoming.default.or(config.default);
            }
            config.save(path)?;
            eprintln!(
                "imported {count} provider(s) into {} ({})",
                path.display(),
                if merge { "merged" } else { "replaced" }
            );
            Ok(())
        }
        ConfigCmd::Edit => {
            if !path.exists() {
                Config::default().save(path)?;
            }
            let editor = std::env::var("VISUAL")
                .or_else(|_| std::env::var("EDITOR"))
                .unwrap_or_else(|_| "vi".into());
            let status = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("{editor} \"$0\""))
                .arg(path)
                .status()
                .with_context(|| format!("launching {editor}"))?;
            if !status.success() {
                bail!("editor exited with {status}");
            }
            let config = Config::load(path)?;
            eprintln!("config OK: {} provider(s)", config.providers.len());
            Ok(())
        }
    }
}

// ------------------------------------------------------------------- input

/// Hidden prompt when attached to a terminal; a single stdin line otherwise.
fn read_secret(prompt: &str) -> Result<Secret> {
    let mut raw = if io::stdin().is_terminal() {
        rpassword::prompt_password(prompt).context("reading token")?
    } else {
        let mut s = String::new();
        io::stdin()
            .read_line(&mut s)
            .context("reading token from stdin")?;
        s
    };
    let secret = Secret::new(raw.trim());
    raw.zeroize();
    Ok(secret)
}

fn read_secret_file(file: &Path) -> Result<Secret> {
    let mut raw = String::new();
    if file.as_os_str() == "-" {
        io::stdin()
            .read_to_string(&mut raw)
            .context("reading token from stdin")?;
    } else {
        raw =
            std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    }
    let secret = Secret::new(raw.lines().next().unwrap_or("").trim());
    raw.zeroize();
    Ok(secret)
}

fn read_line(prompt: &str) -> Result<String> {
    eprint!("{prompt}");
    io::stderr().flush()?;
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    Ok(s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_definition_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn bare_provider_parses_as_positional() {
        let cli = Cli::try_parse_from(["emask", "fm", "--for", "example.com"]).unwrap();
        assert_eq!(cli.provider.as_deref(), Some("fm"));
        assert_eq!(cli.create.site.as_deref(), Some("example.com"));
        assert!(cli.command.is_none());
    }

    #[test]
    fn subcommands_still_win() {
        let cli = Cli::try_parse_from(["emask", "providers"]).unwrap();
        assert!(matches!(cli.command, Some(Command::Providers)));
        let cli = Cli::try_parse_from(["emask", "auth", "login", "--user", "alice"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Auth {
                cmd: AuthCmd::Login { .. }
            })
        ));
    }
}
