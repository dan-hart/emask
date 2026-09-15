// SPDX-License-Identifier: AGPL-3.0-or-later
//! Fastmail Masked Email via JMAP.
//!
//! Token: https://app.fastmail.com/settings/security/tokens/new (scope "Masked Email").
//! The account id is discovered from the session endpoint once and cached in
//! the config so subsequent calls are a single request.

use super::{require_token, CreateOptions, Kind, Mask};
use crate::config::ProviderConfig;
use crate::http;
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

pub const SESSION_URL: &str = "https://api.fastmail.com/jmap/session";
pub const API_URL: &str = "https://api.fastmail.com/jmap/api/";
pub const CAPABILITY: &str = "https://www.fastmail.com/dev/maskedemail";

pub fn create(name: &str, cfg: &mut ProviderConfig, opts: &CreateOptions) -> Result<Mask> {
    let token = require_token(name, cfg)?.to_string();
    let agent = http::agent();

    if cfg
        .account_id
        .as_deref()
        .is_none_or(|id| id.trim().is_empty())
    {
        cfg.account_id = Some(fetch_account_id(&agent, &token)?);
    }
    let account_id = cfg.account_id.clone().expect("account id set above");

    let description = opts
        .description
        .clone()
        .or_else(|| opts.site.clone())
        .or_else(|| cfg.description.clone())
        .unwrap_or_else(|| "emask".to_string());

    let mut new = json!({ "state": "enabled", "description": description });
    if let Some(site) = &opts.site {
        new["forDomain"] = json!(site);
    }
    if let Some(prefix) = &opts.prefix {
        new["emailPrefix"] = json!(prefix);
    }
    let body = json!({
        "using": ["urn:ietf:params:jmap:core", CAPABILITY],
        "methodCalls": [["MaskedEmail/set", { "accountId": account_id, "create": { "new": new } }, "0"]]
    });

    let resp = agent
        .post(API_URL)
        .set("Authorization", &format!("Bearer {token}"))
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|e| http::describe(e, "Fastmail MaskedEmail/set failed"))?;
    let value: Value = resp
        .into_json()
        .context("Fastmail returned a non-JSON response")?;
    let email = parse_created(&value)?;
    Ok(Mask {
        email,
        provider: name.to_string(),
        kind: Kind::Fastmail,
        note: None,
    })
}

pub fn check(name: &str, cfg: &mut ProviderConfig) -> Result<String> {
    let token = require_token(name, cfg)?.to_string();
    let session = fetch_session(&http::agent(), &token)?;
    let account = account_from_session(&session)?;
    let username = session["username"].as_str().unwrap_or("?").to_string();
    cfg.account_id = Some(account.clone());
    Ok(format!(
        "Fastmail OK — signed in as {username}, masked-email account {account}"
    ))
}

fn fetch_session(agent: &ureq::Agent, token: &str) -> Result<Value> {
    agent
        .get(SESSION_URL)
        .set("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|e| http::describe(e, "Fastmail session request failed"))?
        .into_json()
        .context("Fastmail session was not JSON")
}

pub fn fetch_account_id(agent: &ureq::Agent, token: &str) -> Result<String> {
    account_from_session(&fetch_session(agent, token)?)
}

/// The `primaryAccounts` entry for the masked-email capability. Missing means
/// the token was created without the "Masked Email" scope.
pub fn account_from_session(session: &Value) -> Result<String> {
    if let Some(id) = session["primaryAccounts"][CAPABILITY].as_str() {
        return Ok(id.to_string());
    }
    let present = session["primaryAccounts"]
        .as_object()
        .map(|m| m.keys().cloned().collect::<Vec<_>>().join(", "))
        .unwrap_or_default();
    bail!(
        "this Fastmail token has no Masked Email capability (capabilities present: {present}).\n\
         Create a token with the \"Masked Email\" scope at https://app.fastmail.com/settings/security/tokens/new"
    )
}

/// Pull the created address out of a `MaskedEmail/set` response.
pub fn parse_created(v: &Value) -> Result<String> {
    let response = v
        .get("methodResponses")
        .and_then(|m| m.get(0))
        .ok_or_else(|| {
            anyhow!(
                "unexpected Fastmail response: {}",
                http::truncate(&v.to_string(), 300)
            )
        })?;
    let method = response.get(0).and_then(Value::as_str).unwrap_or("");
    let payload = response.get(1).cloned().unwrap_or(Value::Null);

    if method == "error" {
        bail!(
            "Fastmail error: {} {}",
            payload["type"].as_str().unwrap_or("unknown"),
            payload["description"].as_str().unwrap_or("")
        );
    }
    if let Some(email) = payload
        .pointer("/created/new/email")
        .and_then(Value::as_str)
    {
        return Ok(email.to_string());
    }
    if let Some(err) = payload.pointer("/notCreated/new") {
        bail!(
            "Fastmail refused to create the address: {} {}",
            err["type"].as_str().unwrap_or("unknown"),
            err["description"].as_str().unwrap_or("")
        );
    }
    bail!(
        "unexpected Fastmail response: {}",
        http::truncate(&v.to_string(), 300)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_created_email() {
        let v: Value = serde_json::from_str(r#"{"methodResponses":[["MaskedEmail/set",{"accountId":"u1","created":{"new":{"id":"m1","email":"sample.mask1234@fastmail.com"}}},"0"]]}"#).unwrap();
        assert_eq!(parse_created(&v).unwrap(), "sample.mask1234@fastmail.com");
    }

    #[test]
    fn surfaces_not_created() {
        let v: Value = serde_json::from_str(r#"{"methodResponses":[["MaskedEmail/set",{"notCreated":{"new":{"type":"invalidProperties","description":"bad prefix"}}},"0"]]}"#).unwrap();
        let err = parse_created(&v).unwrap_err().to_string();
        assert!(
            err.contains("invalidProperties") && err.contains("bad prefix"),
            "{err}"
        );
    }

    #[test]
    fn surfaces_method_error() {
        let v: Value = serde_json::from_str(r#"{"methodResponses":[["error",{"type":"accountNotFound","description":"nope"},"0"]]}"#).unwrap();
        assert!(parse_created(&v)
            .unwrap_err()
            .to_string()
            .contains("accountNotFound"));
    }

    #[test]
    fn account_id_requires_capability() {
        let ok: Value = serde_json::from_str(
            r#"{"primaryAccounts":{"https://www.fastmail.com/dev/maskedemail":"uabc"}}"#,
        )
        .unwrap();
        assert_eq!(account_from_session(&ok).unwrap(), "uabc");
        let missing: Value =
            serde_json::from_str(r#"{"primaryAccounts":{"urn:ietf:params:jmap:mail":"uabc"}}"#)
                .unwrap();
        assert!(account_from_session(&missing)
            .unwrap_err()
            .to_string()
            .contains("Masked Email"));
    }
}
