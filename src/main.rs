// SPDX-License-Identifier: AGPL-3.0-or-later
//! emask — mint a masked email address and copy it to your clipboard.
//!
//! Providers (Fastmail, DuckDuckGo) live in [`providers`]; everything else is
//! plumbing: [`config`] for the portable TOML file, [`cli`] for the command
//! surface, [`clipboard`] and [`http`] for the two side effects we perform.

mod cli;
mod clipboard;
mod config;
mod http;
mod providers;
mod secret;

fn main() {
    if let Err(err) = cli::run() {
        eprintln!("emask: error: {err:#}");
        std::process::exit(1);
    }
}
