//! `mabel contact set|show`.
//!
//! The note is private: it lives in `contacts/<identity_id>.json`, is never
//! signed and never leaves this node (proposal 003 section 1). It is valid for
//! a foreign identity, which is the point: the public affordance is the
//! hostname, and everything else about a person goes here.

use mabel_node::api::documents::ContactView;
use mabel_node::contacts::{ContactTextError, MAX_NICKNAME_BYTES, MAX_NOTE_BYTES, normalize};

use crate::context::Context;
use crate::error::{CliError, Result};
use crate::ids;
use crate::render::Outcome;

/// `mabel contact set <identity> [--nickname N] [--note T]`.
///
/// Replacement, like the profile: an omitted flag clears that field, and
/// clearing both removes the file.
pub fn set(
    ctx: &Context,
    name: &str,
    nickname: Option<&str>,
    note: Option<&str>,
) -> Result<Outcome> {
    let identity = ctx.resolve(name)?;
    let nickname = field("nickname", nickname, MAX_NICKNAME_BYTES)?;
    let note = field("note", note, MAX_NOTE_BYTES)?;
    let contact = ctx.contacts().replace(identity, nickname, note)?;
    let text = match &contact {
        Some(contact) => format!(
            "{identity}\n  nickname: {}\n  note:     {}",
            shown(contact.nickname.as_deref()),
            shown(contact.note.as_deref())
        ),
        None => format!("{identity}\nno contact note recorded here"),
    };
    let document = ContactView {
        identity_id: ids::identity(identity),
        contact: contact.as_ref().map(mabel_node::wallet::contact_document),
    };
    Outcome::new(&document, text)
}

/// `mabel contact show <identity>`.
pub fn show(ctx: &Context, name: &str) -> Result<Outcome> {
    let identity = ctx.resolve(name)?;
    let contact = ctx.contacts().read(identity)?;
    let text = match &contact {
        Some(contact) => format!(
            "{identity}\n  nickname: {}\n  note:     {}",
            shown(contact.nickname.as_deref()),
            shown(contact.note.as_deref())
        ),
        None => format!("{identity}\nno contact note recorded here"),
    };
    let document = ContactView {
        identity_id: ids::identity(identity),
        contact: contact.as_ref().map(mabel_node::wallet::contact_document),
    };
    Outcome::new(&document, text)
}

fn shown(value: Option<&str>) -> String {
    value.map_or_else(|| "(unset)".to_owned(), ToOwned::to_owned)
}

/// One field against the caps and the codepoint policy of proposal 003
/// section 1, reported with the same reasons the HTTP route uses.
fn field(field: &'static str, value: Option<&str>, cap: usize) -> Result<Option<String>> {
    normalize(field, value, cap).map_err(|error| match error {
        ContactTextError::TooLong { len, cap, .. } => {
            CliError::schema("contact_field_too_long", error.to_string())
                .with_detail("field", field)
                .with_detail("len", len)
                .with_detail("cap", cap)
        }
        ContactTextError::Invalid { detail, .. } => {
            CliError::schema("invalid_contact_text", error.to_string())
                .with_detail("field", field)
                .with_detail("detail", detail)
        }
    })
}
