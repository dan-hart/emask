// SPDX-License-Identifier: AGPL-3.0-or-later
//! Guided credential setup. UI and service operations are injected in tests;
//! the wizard never creates masked addresses.

use crate::config::{Config, ProviderConfig};
use crate::providers::{self, Kind};
use crate::secret::Secret;
use anyhow::{bail, Context, Result};
use std::io::{self, IsTerminal, Write};
use std::path::Path;
use zeroize::Zeroizing;

trait Ui {
    fn message(&mut self, text: &str);
    fn line(&mut self, prompt: &str) -> Result<String>;
    fn secret(&mut self, prompt: &str) -> Result<Secret>;
}

trait Services {
    fn verify(&mut self, name: &str, candidate: &mut ProviderConfig) -> Result<()>;
    fn request_passphrase(&mut self, user: &str) -> Result<()>;
    fn exchange_passphrase(&mut self, user: &str, passphrase: &Secret) -> Result<Secret>;
}

struct Terminal;

impl Ui for Terminal {
    fn message(&mut self, text: &str) {
        eprintln!("{text}");
    }

    fn line(&mut self, prompt: &str) -> Result<String> {
        eprint!("{prompt}");
        io::stderr().flush()?;
        let mut answer = String::new();
        if io::stdin().read_line(&mut answer)? == 0 {
            bail!(
                "input ended; the account in progress was not saved. Run `emask setup` to continue"
            );
        }
        Ok(answer.trim().to_string())
    }

    fn secret(&mut self, prompt: &str) -> Result<Secret> {
        let raw = Zeroizing::new(
            rpassword::prompt_password(prompt).context("reading hidden credential")?,
        );
        Ok(Secret::new(raw.trim()))
    }
}

struct LiveServices;

impl Services for LiveServices {
    fn verify(&mut self, name: &str, candidate: &mut ProviderConfig) -> Result<()> {
        providers::check(name, candidate)?;
        Ok(())
    }

    fn request_passphrase(&mut self, user: &str) -> Result<()> {
        providers::duckduckgo::request_passphrase(&crate::http::agent(), user)
    }

    fn exchange_passphrase(&mut self, user: &str, passphrase: &Secret) -> Result<Secret> {
        providers::duckduckgo::exchange_passphrase(&crate::http::agent(), user, passphrase.expose())
            .map(Secret::new)
    }
}

pub fn run(path: &Path, wanted: Option<&str>) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        bail!("setup requires an interactive terminal. Run `emask setup` in your terminal, or use `emask auth set <PROVIDER> --token-file FILE` for scripts");
    }
    wizard(path, wanted, &mut Terminal, &mut LiveServices)
}

fn wizard(
    path: &Path,
    wanted: Option<&str>,
    ui: &mut impl Ui,
    services: &mut impl Services,
) -> Result<()> {
    let mut config = Config::load(path)?;
    let targets = targets(&config, wanted)?;
    ui.message("\nemask setup — connect your email services");
    ui.message("Tokens and passphrases are hidden while you paste. You can skip any service.");
    ui.message(&format!(
        "Credentials are saved privately to {}",
        path.display()
    ));

    for (name, kind) in targets {
        ui.message(&format!("\n{kind} — '{name}'"));
        let original = config.providers.get(&name);
        let has_token = original.is_some_and(ProviderConfig::has_token);
        let prompt = if has_token {
            "Saved credentials found. Replace them? [y/N]: "
        } else {
            "Set up this service? [Y/n]: "
        };
        if !confirm(ui, prompt, !has_token)? {
            ui.message(if has_token {
                "Kept saved credentials."
            } else {
                "Skipped."
            });
            continue;
        }
        let original = original
            .cloned()
            .unwrap_or_else(|| ProviderConfig::new(kind));
        loop {
            let mut candidate = original.clone();
            match credentials(&name, &mut candidate, ui, services)? {
                Credentials::Skipped => {
                    ui.message("Skipped; saved credentials were kept.");
                    break;
                }
                Credentials::Ready => {
                    if !candidate.enabled {
                        candidate.enabled =
                            confirm(ui, "This account is disabled. Enable it? [Y/n]: ", true)?;
                    }
                    // Persist only a completed candidate. A failed check or EOF never
                    // replaces the old account, and later cancellation keeps earlier saves.
                    config.providers.insert(name.clone(), candidate);
                    config.save(path)?;
                    ui.message(&format!("Saved '{name}'."));
                    break;
                }
                Credentials::ServiceFailed => {
                    // Upstream errors may contain response bodies or passphrase URLs.
                    // Never echo those in an interactive credential flow.
                    ui.message("Could not connect or verify credentials. Check your connection and credentials (Fastmail needs the Masked Email scope). Saved credentials were kept.");
                    if choice(ui, "[r] Retry or [s] skip? [s]: ", &["r", "s"], "s")? == "s" {
                        break;
                    }
                }
            }
        }
    }
    choose_default(&mut config, path, ui)?;
    Ok(())
}

fn targets(config: &Config, wanted: Option<&str>) -> Result<Vec<(String, Kind)>> {
    if let Some(wanted) = wanted {
        if let Some(account) = config.providers.get(wanted) {
            return Ok(vec![(wanted.to_string(), account.kind)]);
        }
        if let Some(kind) = Kind::from_alias(wanted) {
            if config.providers.values().any(|p| p.kind == kind) {
                let name = config.resolve_name(Some(wanted))?;
                return Ok(vec![(name, kind)]);
            }
            return Ok(vec![(available_name(config, kind), kind)]);
        }
        bail!("unknown provider '{wanted}'. Use `emask setup fm`, `emask setup ddg`, or an existing name from `emask providers`");
    }
    let mut targets = Vec::new();
    for kind in Kind::ALL {
        let existing: Vec<_> = config
            .providers
            .iter()
            .filter(|(_, p)| p.kind == kind)
            .map(|(name, _)| (name.clone(), kind))
            .collect();
        if existing.is_empty() {
            targets.push((available_name(config, kind), kind));
        } else {
            targets.extend(existing);
        }
    }
    Ok(targets)
}

fn available_name(config: &Config, kind: Kind) -> String {
    let base = kind.aliases()[1];
    let mut name = base.to_string();
    let mut suffix = 2;
    while config.providers.contains_key(&name) {
        name = format!("{base}{suffix}");
        suffix += 1;
    }
    name
}

enum Credentials {
    Skipped,
    Ready,
    ServiceFailed,
}

/// Network errors are handled by the retry loop; input errors propagate
/// separately so cancellation never retries or saves.
fn credentials(
    name: &str,
    candidate: &mut ProviderConfig,
    ui: &mut impl Ui,
    services: &mut impl Services,
) -> Result<Credentials> {
    ui.message(candidate.kind.token_help());
    if candidate.kind == Kind::DuckDuckGo {
        let method = choice(
            ui,
            "[1] Paste an access token  [2] Email passphrase login [1]: ",
            &["1", "2"],
            "1",
        )?;
        if method == "2" {
            let user = loop {
                let raw = ui.line("Your Duck Address (name or name@duck.com; Enter to skip): ")?;
                let user = raw.trim().to_ascii_lowercase();
                let user = user.strip_suffix("@duck.com").unwrap_or(&user);
                if user.is_empty() {
                    return Ok(Credentials::Skipped);
                }
                if !user.contains('@') && !user.chars().any(char::is_whitespace) {
                    break user.to_string();
                }
                ui.message("Enter a Duck Address, for example alice@duck.com.");
            };
            ui.message("Requesting your email passphrase…");
            if services.request_passphrase(&user).is_err() {
                return Ok(Credentials::ServiceFailed);
            }
            ui.message("Check the inbox behind your Duck Address for the one-time passphrase.");
            let phrase = ui.secret("Paste the passphrase (hidden; Enter to skip): ")?;
            if phrase.is_empty() {
                return Ok(Credentials::Skipped);
            }
            let token = match services.exchange_passphrase(&user, &phrase) {
                Ok(token) => token,
                Err(_) => return Ok(Credentials::ServiceFailed),
            };
            if token.is_empty() {
                return Ok(Credentials::ServiceFailed);
            }
            candidate.set_token(token);
            candidate.username = Some(user);
            ui.message("DuckDuckGo login succeeded.");
            return Ok(Credentials::Ready);
        }
    }
    let token = ui.secret("Paste token (hidden; Enter to skip): ")?;
    if token.is_empty() {
        return Ok(Credentials::Skipped);
    }
    candidate.set_token(token);
    if candidate.kind == Kind::Fastmail {
        ui.message("Checking Fastmail Masked Email access…");
        if services.verify(name, candidate).is_err() {
            return Ok(Credentials::ServiceFailed);
        }
        ui.message("Fastmail credentials verified.");
    } else {
        // A pasted token may belong to a different Duck Address.
        candidate.username = None;
        ui.message("DuckDuckGo token accepted for saving; not verified. Creating an address is required to verify it.");
    }
    Ok(Credentials::Ready)
}

fn choice(ui: &mut impl Ui, prompt: &str, options: &[&str], default: &str) -> Result<String> {
    loop {
        let answer = ui.line(prompt)?.trim().to_ascii_lowercase();
        if answer.is_empty() {
            return Ok(default.to_string());
        }
        if options.contains(&answer.as_str()) {
            return Ok(answer);
        }
        ui.message(&format!(
            "Choose {} or press Enter for {default}.",
            options.join("/")
        ));
    }
}

fn confirm(ui: &mut impl Ui, prompt: &str, default: bool) -> Result<bool> {
    Ok(matches!(
        choice(
            ui,
            prompt,
            &["y", "yes", "n", "no"],
            if default { "y" } else { "n" }
        )?
        .as_str(),
        "y" | "yes"
    ))
}

fn choose_default(config: &mut Config, path: &Path, ui: &mut impl Ui) -> Result<()> {
    let ready: Vec<_> = config
        .providers
        .iter()
        .filter(|(_, p)| p.enabled && p.has_token())
        .map(|(name, _)| name.clone())
        .collect();
    if ready.is_empty() {
        ui.message(
            "\nNo enabled services with saved tokens. Run `emask setup` when you are ready.",
        );
        return Ok(());
    }
    let current = ready
        .iter()
        .position(|name| config.default.as_ref() == Some(name))
        .unwrap_or(0);
    let selected = if ready.len() == 1 {
        0
    } else {
        ui.message("\nChoose the service used by plain `emask`:");
        for (index, name) in ready.iter().enumerate() {
            ui.message(&format!(
                "  {}. {name} ({})",
                index + 1,
                config.providers[name].kind
            ));
        }
        loop {
            let answer = ui.line(&format!("Default service [{}]: ", current + 1))?;
            if answer.is_empty() {
                break current;
            }
            if let Ok(index) = answer.parse::<usize>() {
                if (1..=ready.len()).contains(&index) {
                    break index - 1;
                }
            }
            ui.message(
                "Enter a number from the list, or press Enter to keep the suggested default.",
            );
        }
    };
    let name = &ready[selected];
    if config.default.as_ref() != Some(name) {
        config.default = Some(name.clone());
        config.save(path)?;
    }
    let command = if Config::path(None)? == path {
        "emask".to_string()
    } else {
        format!(
            "emask --config '{}'",
            path.display().to_string().replace('\'', "'\\''")
        )
    };
    ui.message(&format!("\nSetup complete. Default: '{name}'.\nConfig: {}\nCreate your first address: {command}\nReview saved services: {command} providers", path.display()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestFile(std::path::PathBuf);
    impl TestFile {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            Self(std::env::temp_dir().join(format!(
                "emask-setup-{}-{}/config.toml",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )))
        }
    }
    impl Drop for TestFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.0.parent().unwrap());
        }
    }

    #[derive(Default)]
    struct Script {
        input: VecDeque<(bool, &'static str)>,
        output: String,
    }
    impl Script {
        fn new(input: &[(bool, &'static str)]) -> Self {
            Self {
                input: input.iter().copied().collect(),
                output: String::new(),
            }
        }
        fn read(&mut self, prompt: &str, hidden: bool) -> Result<String> {
            self.output.push_str(prompt);
            let Some((expected_hidden, answer)) = self.input.pop_front() else {
                bail!("input ended; current account was not saved");
            };
            assert_eq!(hidden, expected_hidden, "prompt: {prompt}");
            Ok(answer.to_string())
        }
    }
    impl Ui for Script {
        fn message(&mut self, text: &str) {
            self.output.push_str(text);
            self.output.push('\n');
        }
        fn line(&mut self, prompt: &str) -> Result<String> {
            self.read(prompt, false)
        }
        fn secret(&mut self, prompt: &str) -> Result<Secret> {
            Ok(Secret::new(self.read(prompt, true)?.trim()))
        }
    }

    #[derive(Default)]
    struct TestServices {
        checks: usize,
        requests: usize,
        exchanges: usize,
        failures: usize,
    }
    impl Services for TestServices {
        fn verify(&mut self, _: &str, candidate: &mut ProviderConfig) -> Result<()> {
            self.checks += 1;
            if self.failures > 0 {
                self.failures -= 1;
                bail!("synthetic-secret-in-upstream-error");
            }
            candidate.account_id = Some("u12345678".into());
            Ok(())
        }
        fn request_passphrase(&mut self, user: &str) -> Result<()> {
            assert_eq!(user, "alice");
            self.requests += 1;
            Ok(())
        }
        fn exchange_passphrase(&mut self, user: &str, phrase: &Secret) -> Result<Secret> {
            assert_eq!(user, "alice");
            assert_eq!(phrase.expose(), "example words");
            self.exchanges += 1;
            Ok(Secret::new("synthetic-login-token"))
        }
    }

    fn existing(kind: Kind) -> Config {
        let mut config = Config::default();
        let mut account = ProviderConfig::new(kind);
        account.set_token(Secret::new("synthetic-old-token"));
        account.account_id = Some("uold".into());
        account.description = Some("Work signups".into());
        config.providers.insert("work".into(), account);
        config.default = Some("work".into());
        config
    }

    #[test]
    fn skipping_everything_does_not_create_a_config() {
        let file = TestFile::new();
        let mut ui = Script::new(&[(false, "n"), (false, "n")]);
        wizard(&file.0, None, &mut ui, &mut TestServices::default()).unwrap();
        assert!(!file.0.exists());
        assert!(ui.input.is_empty());
    }

    #[test]
    fn pasted_duck_token_is_saved_privately_and_not_claimed_verified() {
        let file = TestFile::new();
        let mut ui = Script::new(&[(false, ""), (false, ""), (true, "synthetic-duck-token")]);
        let mut services = TestServices::default();
        wizard(&file.0, Some("ddg"), &mut ui, &mut services).unwrap();
        let config = Config::load(&file.0).unwrap();
        assert_eq!(config.default.as_deref(), Some("ddg"));
        assert!(config.providers["ddg"].has_token());
        assert!(ui.output.contains("not verified"));
        assert!(!ui.output.contains("synthetic-duck-token"));
        assert_eq!(services.checks, 0);
        assert!(
            ui.output.contains("emask --config"),
            "custom configs need a working next command"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&file.0).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                std::fs::metadata(file.0.parent().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
    }

    #[test]
    fn keeping_existing_credentials_does_not_rewrite_file() {
        let file = TestFile::new();
        existing(Kind::Fastmail).save(&file.0).unwrap();
        let before = std::fs::read(&file.0).unwrap();
        let mut ui = Script::new(&[(false, "")]);
        wizard(&file.0, Some("work"), &mut ui, &mut TestServices::default()).unwrap();
        assert_eq!(before, std::fs::read(&file.0).unwrap());
    }

    #[test]
    fn failed_fastmail_replacement_preserves_old_credentials_and_hides_error() {
        let file = TestFile::new();
        existing(Kind::Fastmail).save(&file.0).unwrap();
        let before = std::fs::read(&file.0).unwrap();
        let mut ui = Script::new(&[(false, "y"), (true, "synthetic-new-token"), (false, "s")]);
        let mut services = TestServices {
            failures: 1,
            ..Default::default()
        };
        wizard(&file.0, Some("work"), &mut ui, &mut services).unwrap();
        assert_eq!(before, std::fs::read(&file.0).unwrap());
        assert!(!ui.output.contains("synthetic-secret-in-upstream-error"));
    }

    #[test]
    fn fastmail_retry_saves_verified_candidate_and_preserves_metadata() {
        let file = TestFile::new();
        existing(Kind::Fastmail).save(&file.0).unwrap();
        let mut ui = Script::new(&[
            (false, "y"),
            (true, "synthetic-bad"),
            (false, "r"),
            (true, "synthetic-new"),
        ]);
        let mut services = TestServices {
            failures: 1,
            ..Default::default()
        };
        wizard(&file.0, Some("work"), &mut ui, &mut services).unwrap();
        let config = Config::load(&file.0).unwrap();
        assert_eq!(
            config.providers["work"].account_id.as_deref(),
            Some("u12345678")
        );
        assert_eq!(
            config.providers["work"].description.as_deref(),
            Some("Work signups")
        );
        assert_eq!(
            config.providers["work"].token.as_ref().unwrap().expose(),
            "synthetic-new"
        );
        assert_eq!(services.checks, 2);
    }

    #[test]
    fn duck_login_accepts_full_address_and_hides_passphrase() {
        let file = TestFile::new();
        let mut ui = Script::new(&[
            (false, ""),
            (false, "2"),
            (false, "alice@duck.com"),
            (true, "example words"),
        ]);
        let mut services = TestServices::default();
        wizard(&file.0, Some("duckduckgo"), &mut ui, &mut services).unwrap();
        let config = Config::load(&file.0).unwrap();
        assert_eq!(config.providers["ddg"].username.as_deref(), Some("alice"));
        assert_eq!(services.requests, 1);
        assert_eq!(services.exchanges, 1);
        assert!(!ui.output.contains("example words"));
    }

    #[test]
    fn eof_keeps_previously_completed_account() {
        let file = TestFile::new();
        let mut ui = Script::new(&[(false, ""), (true, "synthetic-fastmail")]);
        assert!(wizard(&file.0, None, &mut ui, &mut TestServices::default()).is_err());
        let config = Config::load(&file.0).unwrap();
        assert!(config.providers["fm"].has_token());
        assert!(!config.providers.contains_key("ddg"));
    }

    #[test]
    fn eof_during_token_input_exits_without_offering_service_retry() {
        let file = TestFile::new();
        let mut ui = Script::new(&[(false, "")]);
        assert!(wizard(&file.0, Some("fm"), &mut ui, &mut TestServices::default()).is_err());
        assert!(!file.0.exists());
        assert!(!ui.output.contains("Retry"));
    }

    #[test]
    fn blank_token_keeps_old_credentials() {
        let file = TestFile::new();
        existing(Kind::Fastmail).save(&file.0).unwrap();
        let before = std::fs::read(&file.0).unwrap();
        let mut ui = Script::new(&[(false, "y"), (true, "")]);
        wizard(&file.0, Some("work"), &mut ui, &mut TestServices::default()).unwrap();
        assert_eq!(before, std::fs::read(&file.0).unwrap());
    }

    #[test]
    fn disabled_account_stays_disabled_when_enable_is_declined() {
        let file = TestFile::new();
        let mut original = existing(Kind::DuckDuckGo);
        original.providers.get_mut("work").unwrap().enabled = false;
        original.save(&file.0).unwrap();
        let mut ui = Script::new(&[
            (false, "y"),
            (false, ""),
            (true, "synthetic-new"),
            (false, "n"),
        ]);
        wizard(&file.0, Some("work"), &mut ui, &mut TestServices::default()).unwrap();
        assert!(!Config::load(&file.0).unwrap().providers["work"].enabled);
    }

    #[test]
    fn default_selection_lists_only_enabled_accounts_with_tokens() {
        let file = TestFile::new();
        let mut config = existing(Kind::Fastmail);
        let mut duck = ProviderConfig::new(Kind::DuckDuckGo);
        duck.set_token(Secret::new("synthetic-duck"));
        config.providers.insert("duck".into(), duck);
        config
            .providers
            .insert("empty".into(), ProviderConfig::new(Kind::Fastmail));
        config.save(&file.0).unwrap();
        // Configure just 'work', keep it, then select it as the default (sorted second).
        let mut ui = Script::new(&[(false, ""), (false, "2")]);
        wizard(&file.0, Some("work"), &mut ui, &mut TestServices::default()).unwrap();
        assert_eq!(
            Config::load(&file.0).unwrap().default.as_deref(),
            Some("work")
        );
        assert!(!ui.output.contains("3. empty"));
    }

    #[test]
    fn ambiguous_alias_fails_before_prompting() {
        let file = TestFile::new();
        let mut config = existing(Kind::Fastmail);
        config
            .providers
            .insert("personal".into(), config.providers["work"].clone());
        config.save(&file.0).unwrap();
        let mut ui = Script::default();
        let error = wizard(
            &file.0,
            Some("fastmail"),
            &mut ui,
            &mut TestServices::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("ambiguous"));
    }
}
