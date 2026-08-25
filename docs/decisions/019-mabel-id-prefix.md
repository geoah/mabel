# 019: a shown identity id carries its `mabel://` prefix

- Date: 2026-08-25
- Status: accepted
- Source: product owner

A 52-character base32 string on its own tells a person nothing. It could be an
identity, a key, an entry id or an endpoint, and every one of those is spelled
the same way. So wherever a Mabel identity id is put in front of a person it
reads `mabel://<id>`: the prefix names the system the string belongs to, and it
makes every id on a screen or in a terminal something that can be pasted
straight into the search box, a chat message or another wallet.

The prefix is display only. An identity id is the 52 characters; `mabel://` is
how those characters are shown.

Rules:

- Every human-facing rendering of an identity id carries the prefix: identity
  cards in all their states, inline identities, the identity page, witness
  cards, the node page, entry contents, search results, the share panel, and
  every CLI line that prints an identity id outside `--json`.
- Machine surfaces carry the bare id: protobuf bytes, HTTP API documents,
  `--json` documents, `node.json`, `peers.json`, `bindings/`, and the
  `data-value` attributes the end-to-end suite reads. A `.mabel` file and a QR
  square carry the whole link, as they already did. Inside a DNS record value
  the ids stay bare, because `mabel=` and `mabel-endpoints=` are defined over
  bare ids (proposal 006 section 6).
- An endpoint id names a machine, not an identity, and never takes the prefix.
  It stays bare under its own label. Both render as 52 base32 characters, so
  which one a value is comes from where it sits, never from its shape.
- A copy control for an identity id copies the prefixed form. A copy control for
  an endpoint id copies what it shows.
- Every field and flag that takes an identity id takes the prefixed form too,
  and the two are one input. A link that also names endpoints is accepted only
  where something dials them; anywhere else it is refused whole with the reason
  `invalid_mabel_link` and a sentence saying so. Endpoints are never silently
  dropped.
- A card for an identity with no display name titles itself with the first eight
  characters of its id and an ellipsis. That title is a stand-in name, not an id
  being shown, so it carries no prefix and the no-truncation rule does not reach
  it. The whole prefixed id is on the card as usual.
