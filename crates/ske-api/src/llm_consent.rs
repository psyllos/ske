//! Durable consent bound to the exact destination and disclosed content.
use serde::{Deserialize, Serialize};
use ske_common::{Error, Result};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Consent {
    pub endpoint: String,
    pub snapshot: i64,
    pub repository: Option<String>,
    pub content_sha256: String,
}

impl Consent {
    pub fn scope(
        endpoint: &str,
        snapshot: i64,
        repository: Option<&str>,
        source: &[String],
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            snapshot,
            repository: repository.map(str::to_owned),
            content_sha256: ske_common::sha256_hex(serde_json::to_vec(source).unwrap()),
        }
    }

    /// Call only after explicit user approval. No source content is stored.
    pub fn record(&self, path: &Path) -> Result<()> {
        let bytes = serde_json::to_vec(self).map_err(err)?;
        std::fs::write(path, bytes).map_err(err)
    }

    pub fn matches_record(&self, path: &Path) -> bool {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .is_some_and(|record| record == *self)
    }
}

fn err(e: impl std::fmt::Display) -> Error {
    Error::new("E_USAGE", format!("consent: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consent_scope_binds_endpoint_snapshot_repo_and_content() {
        let first = Consent::scope(
            "https://api.example.com/v1/chat/completions",
            7,
            Some("lab"),
            &["decide.go source".to_string()],
        );
        // Same fields, different content: a distinct scope.
        let other = Consent::scope(
            "https://api.example.com/v1/chat/completions",
            7,
            Some("lab"),
            &["different source".to_string()],
        );
        assert_ne!(first, other);
        // Different endpoint: also distinct.
        let other_endpoint = Consent::scope(
            "https://other.example.com/v1/chat/completions",
            7,
            Some("lab"),
            &["decide.go source".to_string()],
        );
        assert_ne!(first, other_endpoint);
    }

    #[test]
    fn consent_record_and_match_round_trip_in_tmp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("consent.json");
        let consent = Consent::scope("https://api.example.com", 3, None, &["source".to_string()]);
        consent.record(&path).unwrap();
        assert!(consent.matches_record(&path));
        // Tampering with the file (different content hash) must not match.
        let tampered = Consent::scope("https://api.example.com", 3, None, &["other".to_string()]);
        // Same endpoint/snapshot but different content => file was for different scope.
        assert!(!tampered.matches_record(&path));
        std::fs::write(&path, b"not json").unwrap();
        assert!(!tampered.matches_record(&path));
    }
}
