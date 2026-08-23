//! `mabel node id`.

use crate::context::Context;
use crate::documents::NodeId;
use crate::error::Result;
use crate::render::Outcome;

/// `mabel node id`: this node's Iroh endpoint id, base32 as every document
/// spells it.
pub fn id(ctx: &Context) -> Result<Outcome> {
    let endpoint_id = ctx.source()?;
    let text = endpoint_id.to_string();
    Outcome::new(&NodeId { endpoint_id }, text)
}
