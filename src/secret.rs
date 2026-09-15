// SPDX-License-Identifier: AGPL-3.0-or-later
//! A string that refuses to leak: redacted `Debug`, no `Display`, zeroed on drop.
//!
//! Every token in emask lives inside a [`Secret`]. Code that genuinely needs
//! the raw value calls [`Secret::expose`], which makes each use greppable.

use serde::{Deserialize, Serialize};
use std::fmt;
use zeroize::Zeroize;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Secret(value.into())
    }

    /// The raw value. Keep the borrow short and never format it into output.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }

    /// First and last four characters, enough to tell tokens apart.
    pub fn redacted(&self) -> String {
        let chars: Vec<char> = self.0.chars().collect();
        if chars.len() <= 8 {
            "••••••••".to_string()
        } else {
            let head: String = chars[..4].iter().collect();
            let tail: String = chars[chars.len() - 4..].iter().collect();
            format!("{head}…{tail}")
        }
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Secret({})", self.redacted())
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_the_value() {
        let s = Secret::new("fmu1-abcdefghijklmnop");
        let dbg = format!("{s:?}");
        assert!(!dbg.contains("abcdefghijkl"), "{dbg}");
        assert_eq!(s.redacted(), "fmu1…mnop");
        assert_eq!(Secret::new("short").redacted(), "••••••••");
    }

    #[test]
    fn serde_is_transparent() {
        let s: Secret = serde_json::from_str("\"tok\"").unwrap();
        assert_eq!(s.expose(), "tok");
        assert_eq!(serde_json::to_string(&s).unwrap(), "\"tok\"");
    }
}
