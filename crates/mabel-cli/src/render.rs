//! What a command hands back: one `--json` document and the lines a person
//! reads.
//!
//! Success documents carry `ok: true` with the payload flat beside it, the
//! shape `contracts/README.md` freezes for both surfaces. The two are built
//! together so no command can answer one way in text and another in JSON.

use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::{CliError, Result};

/// A command's answer.
#[derive(Debug)]
pub struct Outcome {
    /// The `--json` document, carrying `ok: true`.
    pub document: Value,
    /// The text rendering, without a trailing newline.
    pub text: String,
}

impl Outcome {
    /// Builds both renderings from one payload.
    ///
    /// # Errors
    ///
    /// Returns code 1 if the payload does not serialize as a JSON object,
    /// which the document types do not permit.
    pub fn new(payload: &impl Serialize, text: impl Into<String>) -> Result<Self> {
        Ok(Self {
            document: success(payload)?,
            text: text.into(),
        })
    }
}

/// `{"ok": true, ...payload}`.
fn success(payload: &impl Serialize) -> Result<Value> {
    let value = serde_json::to_value(payload)
        .map_err(|error| CliError::internal("unserializable_document", error.to_string()))?;
    let Value::Object(fields) = value else {
        return Err(CliError::internal(
            "unserializable_document",
            "a success payload must be a JSON object",
        ));
    };
    let mut document = Map::with_capacity(fields.len() + 1);
    document.insert("ok".to_owned(), Value::Bool(true));
    for (key, value) in fields {
        document.insert(key, value);
    }
    Ok(Value::Object(document))
}

/// Unix milliseconds as RFC 3339 UTC, the one human time in the output
/// (`contracts/README.md`, "Timestamps").
///
/// The node renders it, so the statement in a report the wallet runtime built
/// and the statement this command built carry the same string.
#[must_use]
pub fn rfc3339_utc(timestamp_ms: u64) -> String {
    mabel_node::rfc3339_utc(timestamp_ms)
}

#[cfg(test)]
mod tests {
    use super::{Outcome, rfc3339_utc};
    use serde_json::json;

    #[test]
    fn the_fixture_timestamps_render_as_the_fixture_statements_spell_them() {
        // contracts/cli/verify-ledger.json and verify-trust.json.
        assert_eq!(rfc3339_utc(1_700_000_500_000), "2023-11-14T22:21:40Z");
        assert_eq!(rfc3339_utc(1_700_000_560_000), "2023-11-14T22:22:40Z");
        assert_eq!(rfc3339_utc(1_700_000_620_000), "2023-11-14T22:23:40Z");
    }

    #[test]
    fn the_epoch_a_leap_day_and_the_timestamp_cap_render() {
        assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(1_709_164_800_000), "2024-02-29T00:00:00Z");
        assert_eq!(
            rfc3339_utc(mabel_core::MAX_TIMESTAMP_MS),
            "2100-01-01T00:00:00Z"
        );
    }

    #[test]
    fn a_success_document_carries_ok_beside_a_flat_payload() {
        let outcome = Outcome::new(&json!({"alias": "alice"}), "created").expect("builds");
        assert_eq!(outcome.document, json!({"ok": true, "alias": "alice"}));
        assert_eq!(outcome.text, "created");
    }

    #[test]
    fn a_payload_that_is_not_an_object_is_refused() {
        let error = Outcome::new(&json!("alice"), "created").expect_err("not an object");
        assert_eq!(error.exit_code(), 1);
    }
}
