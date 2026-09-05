//! Where the token comes from.
//!
//! Discovered on every launch and never written: `gh` already keeps it in the
//! keyring, and a second copy on disk is a second thing to leak (`AGENTS.md`
//! rule 8). The order of authority is the environment first — an explicit
//! `E1_GITHUB_TOKEN`, then the two names the ecosystem already uses — and
//! `gh auth token` last, because it is the slowest and the one most people
//! have.

use std::fmt;
use std::process::Command;

/// A bearer token. Its `Debug` output does not contain it.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl Token {
    /// Wrap a token, trimmed. `None` for an empty string, so a variable set
    /// to nothing is the same as one not set.
    pub fn new(raw: impl AsRef<str>) -> Option<Self> {
        let trimmed = raw.as_ref().trim();
        (!trimmed.is_empty()).then(|| Self(trimmed.to_string()))
    }

    /// The token itself, for the `Authorization` header and nothing else.
    pub fn secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(…)")
    }
}

/// The environment variables consulted, in order.
pub const ENVIRONMENT: [&str; 3] = ["E1_GITHUB_TOKEN", "GITHUB_TOKEN", "GH_TOKEN"];

/// Pick a token from the environment, then from `gh`.
///
/// Both sources are injected so the order can be tested without setting
/// process-wide state: `env` answers a variable name, `gh` is only asked when
/// the environment had nothing.
pub fn resolve(
    env: impl Fn(&str) -> Option<String>,
    gh: impl FnOnce() -> Option<String>,
) -> Option<Token> {
    ENVIRONMENT
        .iter()
        .find_map(|name| env(name).and_then(Token::new))
        .or_else(|| gh().and_then(Token::new))
}

/// What `gh auth token` prints, if `gh` is installed and signed in.
pub fn from_gh() -> Option<String> {
    let output = Command::new("gh").args(["auth", "token"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The token this process should use, from the real environment and `gh`.
pub fn discover() -> Option<Token> {
    resolve(|name| std::env::var(name).ok(), from_gh)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_explicit_variable_wins_over_the_shared_ones() {
        let env = |name: &str| match name {
            "E1_GITHUB_TOKEN" => Some("mine".to_string()),
            "GITHUB_TOKEN" => Some("theirs".to_string()),
            _ => None,
        };
        let token = resolve(env, || panic!("gh must not be asked")).unwrap();
        assert_eq!(token.secret(), "mine");
    }

    #[test]
    fn gh_is_only_asked_when_the_environment_had_nothing() {
        let token = resolve(|_| None, || Some("from-gh\n".to_string())).unwrap();
        assert_eq!(token.secret(), "from-gh", "trimmed");
    }

    #[test]
    fn an_empty_variable_is_the_same_as_an_unset_one() {
        let env = |name: &str| (name == "GITHUB_TOKEN").then(|| "   ".to_string());
        assert!(resolve(env, || None).is_none());
    }

    #[test]
    fn the_token_does_not_appear_in_debug_output() {
        let token = Token::new("ghp_secret").unwrap();
        assert!(!format!("{token:?}").contains("secret"));
    }
}
