use std::collections::HashSet;

use super::{
    model::{MemoryEntry, MemoryKind, MemoryScope, MemoryWriteProposal},
    provider::{MemoryError, MemoryResult},
    session_store::now_unix_ms,
};

pub struct MemoryPolicy {
    max_content_characters: usize,
    minimum_importance: f32,
}

impl Default for MemoryPolicy {
    fn default() -> Self {
        Self {
            max_content_characters: 2_000,
            minimum_importance: 0.2,
        }
    }
}

impl MemoryPolicy {
    pub fn approve(
        &self,
        scope: &MemoryScope,
        proposal: MemoryWriteProposal,
    ) -> MemoryResult<MemoryEntry> {
        scope.validate().map_err(MemoryError::PolicyRejected)?;
        let content = proposal.content.trim().to_string();
        let character_count = content.chars().count();
        if character_count == 0 || character_count > self.max_content_characters {
            return Err(MemoryError::PolicyRejected(format!(
                "content must contain 1 to {} characters",
                self.max_content_characters
            )));
        }
        if contains_sensitive_material(&content) {
            return Err(MemoryError::PolicyRejected(
                "potential credential, payment, or private key content cannot enter long-term memory"
                    .into(),
            ));
        }

        let importance = proposal.importance.unwrap_or(0.5);
        if !importance.is_finite() {
            return Err(MemoryError::PolicyRejected(
                "importance must be a finite number".into(),
            ));
        }
        let importance = importance.clamp(0.0, 1.0);
        if importance < self.minimum_importance {
            return Err(MemoryError::PolicyRejected(
                "proposal importance is below the persistence threshold".into(),
            ));
        }

        let source_event_ids = normalize_values(proposal.source_event_ids, 64, 128, "source id")?;
        let tags = normalize_values(proposal.tags, 16, 48, "tag")?;
        let now = now_unix_ms();
        if proposal
            .occurred_at_unix_ms
            .is_some_and(|occurred| occurred > now.saturating_add(300_000))
        {
            return Err(MemoryError::PolicyRejected(
                "occurredAt cannot be more than five minutes in the future".into(),
            ));
        }

        let kind = proposal.suggested_kind.unwrap_or_else(|| {
            if proposal.occurred_at_unix_ms.is_some() {
                MemoryKind::Episodic
            } else {
                MemoryKind::Semantic
            }
        });
        Ok(MemoryEntry {
            scope: scope.clone(),
            kind,
            content,
            importance,
            tags,
            occurred_at_unix_ms: proposal.occurred_at_unix_ms,
            created_at_unix_ms: now,
            source_event_ids,
            metadata: serde_json::json!({
                "approvedBy": "MemoryPolicy",
                "localOnly": true
            }),
        })
    }
}

fn normalize_values(
    values: Vec<String>,
    max_count: usize,
    max_length: usize,
    label: &str,
) -> MemoryResult<Vec<String>> {
    if values.len() > max_count {
        return Err(MemoryError::PolicyRejected(format!(
            "too many {label} values"
        )));
    }
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for value in values {
        let value = value.trim();
        if value.is_empty() || value.chars().count() > max_length {
            return Err(MemoryError::PolicyRejected(format!(
                "{label} is empty or too long"
            )));
        }
        if label == "tag" && contains_sensitive_material(value) {
            return Err(MemoryError::PolicyRejected(
                "sensitive material cannot be stored in memory tags".into(),
            ));
        }
        let deduplication_key = value.to_lowercase();
        if seen.insert(deduplication_key) {
            normalized.push(value.to_string());
        }
    }
    Ok(normalized)
}

fn contains_sensitive_material(content: &str) -> bool {
    let lower = content.to_lowercase();
    const SENSITIVE_MARKERS: &[&str] = &[
        "password",
        "passphrase",
        "api key",
        "api_key",
        "access token",
        "refresh token",
        "private key",
        "credit card",
        "cvv",
        "密码",
        "口令",
        "验证码",
        "访问令牌",
        "刷新令牌",
        "私钥",
        "信用卡",
        "银行卡号",
        "支付密码",
    ];
    SENSITIVE_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> MemoryScope {
        MemoryScope {
            user_id: "user".into(),
            character_id: "character".into(),
            session_id: Some("session".into()),
        }
    }

    fn proposal(content: &str) -> MemoryWriteProposal {
        MemoryWriteProposal {
            content: content.into(),
            suggested_kind: None,
            importance: Some(0.8),
            tags: Vec::new(),
            occurred_at_unix_ms: None,
            source_event_ids: vec!["event-1".into()],
        }
    }

    #[test]
    fn rejects_empty_and_sensitive_content() {
        let policy = MemoryPolicy::default();
        assert!(policy.approve(&scope(), proposal("  ")).is_err());
        assert!(policy
            .approve(&scope(), proposal("My API key is secret-value"))
            .is_err());
        assert!(policy
            .approve(&scope(), proposal("支付密码是 123456"))
            .is_err());
    }

    #[test]
    fn policy_has_final_say_over_kind_and_importance() {
        let policy = MemoryPolicy::default();
        let mut candidate = proposal("The user prefers dark themes");
        candidate.importance = Some(4.0);
        let approved = policy.approve(&scope(), candidate).unwrap();
        assert_eq!(approved.kind, MemoryKind::Semantic);
        assert_eq!(approved.importance, 1.0);
    }
}
