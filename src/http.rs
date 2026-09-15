// SPDX-License-Identifier: AGPL-3.0-or-later
//! Thin helpers around `ureq` so provider code reads as plain HTTP calls.

use anyhow::anyhow;
use std::time::Duration;

/// Sent on every request. DuckDuckGo rejects curl's default User-Agent with an
/// empty 403, but is happy with an honest tool identifier like this one.
pub const USER_AGENT: &str = concat!(
    "emask/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/dan-hart/emask)"
);

pub fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(20))
        .user_agent(USER_AGENT)
        .build()
}

/// Turn a `ureq` failure into a readable error carrying the HTTP status, a
/// trimmed response body, and a hint for the common auth failures.
pub fn describe(err: ureq::Error, what: &str) -> anyhow::Error {
    match err {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            let body = body.trim();
            let hint = match code {
                401 => "\nhint: the token was rejected — refresh it with `emask auth set <PROVIDER>`",
                403 => "\nhint: forbidden — the token may be revoked or the service blocked the request",
                429 => "\nhint: rate limited — wait a moment and try again",
                _ => "",
            };
            if body.is_empty() {
                anyhow!("{what}: HTTP {code}{hint}")
            } else {
                anyhow!("{what}: HTTP {code}: {}{hint}", truncate(body, 300))
            }
        }
        ureq::Error::Transport(t) => anyhow!("{what}: network error: {t}"),
    }
}

pub fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max_chars).collect::<String>())
    }
}
