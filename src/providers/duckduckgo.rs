// SPDX-License-Identifier: AGPL-3.0-or-later
//! DuckDuckGo Email Protection (private @duck.com addresses).
//!
//! The API is undocumented but stable; it is what the DuckDuckGo browser and
//! extension use. Login is a one-time passphrase emailed to the inbox behind
//! your Duck Address, exchanged for a long-lived access token.

use super::{require_token, CreateOptions, Kind, Mask};
use crate::config::ProviderConfig;
use crate::http;
use anyhow::{anyhow, Context, Result};
use serde_json::Value;

pub const BASE: &str = "https://quack.duckduckgo.com/api";
pub const DOMAIN: &str = "duck.com";

pub fn create(name: &str, cfg: &mut ProviderConfig, opts: &CreateOptions) -> Result<Mask> {
    let token = require_token(name, cfg)?;
    let resp = http::agent()
        .post(&format!("{BASE}/email/addresses"))
        .set("Authorization", &format!("Bearer {token}"))
        .set("Content-Type", "application/json")
        .call()
        .map_err(|e| http::describe(e, "DuckDuckGo address request failed"))?;
    let value: Value = resp
        .into_json()
        .context("DuckDuckGo returned a non-JSON response")?;
    let email = parse_address(&value)?;
    let note = opts
        .site
        .as_ref()
        .map(|s| format!("DuckDuckGo does not store labels — this one is for {s}"));
    Ok(Mask {
        email,
        provider: name.to_string(),
        kind: Kind::DuckDuckGo,
        note,
    })
}

pub fn check(name: &str, cfg: &mut ProviderConfig) -> Result<String> {
    require_token(name, cfg)?;
    Ok(format!(
        "DuckDuckGo token present for '{name}' (it can only be verified by minting: `emask {name}`)"
    ))
}

pub fn parse_address(v: &Value) -> Result<String> {
    let local = v["address"]
        .as_str()
        .filter(|a| !a.is_empty())
        .ok_or_else(|| {
            anyhow!(
                "unexpected DuckDuckGo response: {}",
                http::truncate(&v.to_string(), 300)
            )
        })?;
    Ok(format!("{local}@{DOMAIN}"))
}

/// Step 1 of login: DuckDuckGo emails a one-time passphrase to the inbox
/// behind `<username>@duck.com`.
pub fn request_passphrase(agent: &ureq::Agent, username: &str) -> Result<()> {
    agent
        .get(&format!("{BASE}/auth/loginlink"))
        .query("user", username)
        .call()
        .map_err(|e| http::describe(e, "DuckDuckGo login-link request failed"))?;
    Ok(())
}

/// Step 2 of login: exchange the passphrase for a login token, then the login
/// token for the long-lived access token used to mint addresses.
pub fn exchange_passphrase(
    agent: &ureq::Agent,
    username: &str,
    passphrase: &str,
) -> Result<String> {
    let otp = passphrase.split_whitespace().collect::<Vec<_>>().join("+");
    let url = format!("{BASE}/auth/login?user={}&otp={otp}", urlencode(username));
    let login: Value = agent
        .get(&url)
        .call()
        .map_err(|e| http::describe(e, "DuckDuckGo login failed (wrong or expired passphrase?)"))?
        .into_json()
        .context("DuckDuckGo login response was not JSON")?;
    let token = login["token"].as_str().ok_or_else(|| {
        anyhow!(
            "DuckDuckGo login response had no token: {}",
            http::truncate(&login.to_string(), 200)
        )
    })?;

    let dashboard: Value = agent
        .get(&format!("{BASE}/email/dashboard"))
        .set("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|e| http::describe(e, "DuckDuckGo dashboard request failed"))?
        .into_json()
        .context("DuckDuckGo dashboard response was not JSON")?;
    dashboard
        .pointer("/user/access_token")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("DuckDuckGo dashboard response had no access token"))
}

/// Minimal percent-encoding for a query value (RFC 3986 unreserved set kept).
pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_address_into_full_email() {
        let v: Value = serde_json::from_str(r#"{"address":"brave-otter-quiet"}"#).unwrap();
        assert_eq!(parse_address(&v).unwrap(), "brave-otter-quiet@duck.com");
    }

    #[test]
    fn rejects_missing_address() {
        let v: Value = serde_json::from_str(r#"{"error":"invalid_token"}"#).unwrap();
        assert!(parse_address(&v)
            .unwrap_err()
            .to_string()
            .contains("invalid_token"));
    }

    #[test]
    fn urlencode_keeps_unreserved() {
        assert_eq!(urlencode("dan.hart-1_~"), "dan.hart-1_~");
        assert_eq!(urlencode("a b@c"), "a%20b%40c");
    }
}
