//! Where a document's money went, or the explicit claim that it did not move.
//!
//! One definition, shared by `POST /api/sales` and `POST /api/purchases`. Two hand-written
//! copies of a serde decision this subtle are how two spellings of one contract diverge
//! (`odd/tasks/v4-payables.md` S3), so the grammar lives here and both routes call it.

use serde::Deserialize;
use uuid::Uuid;

/// Where a document's money went, or the explicit claim that it did not move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destination {
    /// Money moved, through this method.
    Method(Uuid),
    /// Money did not move. The document owes.
    OnAccount,
}

/// The one refusal a destination can earn. Both routes map it to `400 BAD_REQUEST`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refused;

/// Keeps an explicit `null` distinct from an absent key.
///
/// Used as `#[serde(default, deserialize_with = "…keep_present")]`. `serde` calls
/// `default` only when the key is absent, so the field is `None` for an omitted key and
/// `Some(Value::Null)` for an explicit null. A plain `Option<Uuid>` cannot make that
/// distinction: serde maps both to `None`.
pub fn keep_present<'de, D>(deserializer: D) -> Result<Option<serde_json::Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    serde_json::Value::deserialize(deserializer).map(Some)
}

impl Destination {
    /// Reads the raw `payment_method_id` field.
    ///
    /// - `None` (omitted): `Ok(None)`. The caller decides what omission means; both routes
    ///   read it as the seeded `cash` method, which is what every document meant before v3,
    ///   so an older client keeps its exact behaviour.
    /// - a uuid string: `Ok(Some(Method(id)))`. Money moved, through that method.
    /// - `"on_account"`: `Ok(Some(OnAccount))`. Money did not move; the document owes.
    /// - anything else, **including an explicit `null`**: `Err(Refused)`.
    ///
    /// **Why a sentinel and not `null`.** `null` cannot mean "the till" without silently
    /// turning a debt into a cash document, and it cannot mean "on account" without
    /// silently turning every older client's omitted field into a debt. A sentinel is the
    /// only spelling where all three inputs stay distinguishable, and an explicit null is
    /// refused so the caller has to say which one it means.
    ///
    /// **Why this is a 400 and not a serde error.** axum's `Json` extractor answers any
    /// deserialization failure with 422. Keeping the field raw and judging it here is what
    /// makes the refusal a 400, the status the field's contract states.
    pub fn from_field(raw: Option<serde_json::Value>) -> Result<Option<Self>, Refused> {
        match raw {
            None => Ok(None),
            Some(serde_json::Value::String(text)) if text == "on_account" => {
                Ok(Some(Self::OnAccount))
            }
            Some(serde_json::Value::String(text)) => Uuid::parse_str(&text)
                .map(|id| Some(Self::Method(id)))
                .map_err(|_| Refused),
            // An explicit null, a number, an object, a list: none of them is a destination.
            Some(_) => Err(Refused),
        }
    }
}

/// Only the field's own contract is tested here; the routes' status codes are tested in
/// `backend/tests/integration.rs` through `POST /api/sales` and `POST /api/purchases`.
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_omitted_field_is_none_and_a_null_is_refused() {
        assert_eq!(Destination::from_field(None), Ok(None));
        assert_eq!(Destination::from_field(Some(json!(null))), Err(Refused));
    }

    #[test]
    fn on_account_and_a_uuid_are_the_only_two_spellings() {
        let id = Uuid::from_u128(7);
        assert_eq!(
            Destination::from_field(Some(json!("on_account"))),
            Ok(Some(Destination::OnAccount))
        );
        assert_eq!(
            Destination::from_field(Some(json!(id.to_string()))),
            Ok(Some(Destination::Method(id)))
        );
        assert_eq!(
            Destination::from_field(Some(json!("garbage"))),
            Err(Refused)
        );
        assert_eq!(Destination::from_field(Some(json!(42))), Err(Refused));
    }
}
