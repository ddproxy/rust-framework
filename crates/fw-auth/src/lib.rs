//! Bearer-token checks. Only SHA-256 hashes of tokens are stored. A presented token is hashed and
//! compared against every configured hash, and attempts are throttled per address.
//!
//! # Examples
//!
//! ```
//! use fw_auth::{bearer_token, sha256_hex, TokenSet};
//!
//! let hash = sha256_hex("example value");
//! let tokens = TokenSet::new([("peer-a".to_owned(), hash)]).unwrap();
//! assert_eq!(tokens.verify("example value"), Some("peer-a"));
//! assert_eq!(bearer_token("Bearer abc123"), Some("abc123"));
//! assert!(bearer_token("Basic abc123").is_none());
//! ```

mod throttle;

use std::fmt;

use sha2::{Digest, Sha256};

pub use throttle::{LoginThrottle, Retry, ThrottleSettings};

/// Lowercase hex SHA-256 of `input`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     fw_auth::sha256_hex("abc"),
///     "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
/// );
/// ```
pub fn sha256_hex(input: &str) -> String {
    Sha256::digest(input.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Compares two byte strings without an early exit on the first difference. A length mismatch
/// returns early; that leaks only the length, which is public for hashes.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The token in an `Authorization` value: `Bearer <token>`, with a case-insensitive scheme.
/// Returns `None` for any other scheme and for a token that is empty or holds anything but
/// visible ASCII, so a header with control characters or spaces never reaches the hash.
///
/// # Examples
///
/// ```
/// assert_eq!(fw_auth::bearer_token("bearer  abc123  "), Some("abc123"));
/// assert_eq!(fw_auth::bearer_token("Bearer"), None);
/// ```
pub fn bearer_token(value: &str) -> Option<&str> {
    let (scheme, rest) = value
        .trim_ascii()
        .split_once(|c: char| c.is_ascii_whitespace())?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = rest.trim_ascii();
    let visible = !token.is_empty() && token.bytes().all(|b| b.is_ascii_graphic());
    visible.then_some(token)
}

#[derive(Debug, PartialEq, Eq)]
pub enum TokenSetError {
    /// The named entry's hash is not 64 lowercase hex characters.
    BadHash(String),
}

impl fmt::Display for TokenSetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadHash(name) => write!(f, "token {name:?} has a malformed sha256"),
        }
    }
}

impl std::error::Error for TokenSetError {}

/// Named token hashes. [`TokenSet::verify`] reports which name matched.
#[derive(Debug, Clone, Default)]
pub struct TokenSet {
    entries: Vec<(String, String)>,
}

impl TokenSet {
    /// Builds a set from `(name, sha256 hex)` pairs. Every hash must be 64 lowercase hex
    /// characters.
    ///
    /// # Examples
    ///
    /// ```
    /// use fw_auth::{sha256_hex, TokenSet};
    ///
    /// let hash = sha256_hex("example value");
    /// let tokens = TokenSet::new([("peer-a".to_owned(), hash)]).unwrap();
    /// assert_eq!(tokens.verify("example value"), Some("peer-a"));
    /// assert!(TokenSet::new([("bad".to_owned(), "abc".to_owned())]).is_err());
    /// ```
    pub fn new<I>(entries: I) -> Result<Self, TokenSetError>
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let mut out = Vec::new();
        for (name, hash) in entries {
            let well_formed = hash.len() == 64
                && hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
            if !well_formed {
                return Err(TokenSetError::BadHash(name));
            }
            out.push((name, hash));
        }
        Ok(Self { entries: out })
    }

    /// The name of the token whose hash equals `token`'s, or `None`. Every entry is compared, so
    /// the time taken does not reveal which entry matched.
    pub fn verify(&self, token: &str) -> Option<&str> {
        let presented = sha256_hex(token);
        let mut found = None;
        for (name, hash) in &self.entries {
            let matched = constant_time_eq(presented.as_bytes(), hash.as_bytes());
            if matched && found.is_none() {
                found = Some(name.as_str());
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "correct horse battery staple";

    fn set() -> TokenSet {
        TokenSet::new([
            ("peer-a".to_owned(), sha256_hex(TOKEN)),
            ("peer-b".to_owned(), sha256_hex("another long token value")),
        ])
        .unwrap()
    }

    #[test]
    fn the_hash_of_a_known_input_matches_the_standard_vector() {
        assert_eq!(
            sha256_hex("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn verify_returns_the_matching_name_and_nothing_else() {
        let tokens = set();
        assert_eq!(tokens.verify(TOKEN), Some("peer-a"));
        assert_eq!(tokens.verify("another long token value"), Some("peer-b"));
        assert_eq!(tokens.verify("wrong"), None);
        assert_eq!(tokens.verify(""), None);
        assert_eq!(
            tokens.verify(&sha256_hex(TOKEN)),
            None,
            "the hash is not a token"
        );
    }

    #[test]
    fn a_malformed_stored_hash_is_refused() {
        let upper = sha256_hex("x").to_uppercase();
        assert_eq!(
            TokenSet::new([("a".to_owned(), upper)]).unwrap_err(),
            TokenSetError::BadHash("a".to_owned())
        );
        assert!(TokenSet::new([("b".to_owned(), "abc".to_owned())]).is_err());
    }

    #[test]
    fn bearer_extraction_accepts_scheme_case_and_surrounding_space() {
        assert_eq!(bearer_token("Bearer abc123"), Some("abc123"));
        assert_eq!(bearer_token("bearer   abc123  "), Some("abc123"));
        assert_eq!(bearer_token("  BEARER abc"), Some("abc"));
        assert_eq!(
            bearer_token("Basic dXNlcjpwYXNz"),
            None,
            "other schemes are not bearer"
        );
    }

    #[test]
    fn hostile_authorization_values_are_rejected() {
        let hostile = [
            "",
            "Bearer",
            "Bearer ",
            "Bearer    ",
            "Bearer a b",
            "Bearer a\tb",
            "Bearer a\u{0}b",
            "Bearer \u{1b}[31mred",
            "Bearer tok\r\nX-Injected: 1",
            "Bearer t\u{e9}ken",
            "Bearerabc",
            "Token abc",
            "Bearer\u{0}abc",
        ];
        for value in hostile {
            assert_eq!(
                bearer_token(value),
                None,
                "{value:?} must not yield a token"
            );
        }
    }

    #[test]
    fn a_very_long_token_is_hashed_not_rejected() {
        let long = "a".repeat(64 * 1024);
        let value = format!("Bearer {long}");
        assert_eq!(bearer_token(&value).map(str::len), Some(long.len()));
        assert_eq!(sha256_hex(&long).len(), 64);
    }

    #[test]
    fn constant_time_eq_matches_plain_equality() {
        assert!(constant_time_eq(b"abcd", b"abcd"));
        assert!(!constant_time_eq(b"abcd", b"abce"));
        assert!(!constant_time_eq(b"abcd", b"abc"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn empty_and_very_long_inputs_hash_without_panicking() {
        assert_eq!(
            sha256_hex(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let huge = "x".repeat(1024 * 1024);
        let hash = sha256_hex(&huge);
        assert_eq!(hash.len(), 64);
        assert_ne!(hash, sha256_hex("x"));
        assert_eq!(
            bearer_token(&format!("Bearer {huge}")).map(str::len),
            Some(huge.len())
        );
    }

    #[test]
    fn token_set_rejects_every_malformed_hash_shape() {
        let empty = TokenSet::new([]).unwrap();
        assert_eq!(empty.verify("anything"), None);

        let good = sha256_hex("example value");
        for bad in [
            String::new(),
            "abc".to_owned(),
            good.to_uppercase(),
            format!("{good}0"),
            format!("z{}", &good[1..]),
            format!("{good}\u{e9}"),
        ] {
            assert_eq!(
                TokenSet::new([("bad".to_owned(), bad.clone())]).unwrap_err(),
                TokenSetError::BadHash("bad".to_owned()),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn constant_time_eq_handles_large_and_empty_inputs() {
        let large = vec![0x5a; 16 * 1024];
        let mut different = large.clone();
        different[large.len() - 1] = 0x5b;
        assert!(constant_time_eq(&large, &large));
        assert!(!constant_time_eq(&large, &different));
        assert!(!constant_time_eq(&large, &large[..large.len() - 1]));
        assert!(constant_time_eq(b"", b""));
    }
}
