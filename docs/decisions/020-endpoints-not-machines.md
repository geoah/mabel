# 020: the machines an identity advertises are called endpoints

- Date: 2026-08-25
- Status: accepted
- Source: product owner

One noun for the machines an identity advertises: **endpoints**. Not "machines",
not "nodes". The wire already said so, and only the prose drifted: a link carries
`?endpoints=`, DNS carries `mabel-endpoints=`, the CLI takes `--endpoints` and
`mabel identity endpoints replace`, and every API document names the field
`endpoints`. None of those change.

What changes is what people read. UI copy that said "machines", and any gloss,
CLI line or doc sentence that said "nodes" or "machines" while meaning advertised
endpoints, says "endpoints".

Rules:

- "Endpoint" is the noun for one machine an identity advertises, and "endpoints"
  for the list. It is the same word on the wire, on a screen and in a doc.
- "Node" keeps exactly one meaning: the running program and its home. The Node
  tab, `node.json`, the `mabel node` commands and the phrase "this node" are
  about that and about nothing else.
- The label "Iroh ID" stays. It names the kind of id an endpoint has, which is a
  different question from what the thing is called.
