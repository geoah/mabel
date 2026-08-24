# Proposals

Design proposals, numbered in the order they were opened. Template:
[../templates/proposal.md](../templates/proposal.md). A proposal's status
moves from proposed to accepted or rejected after review; accepted proposals
that establish product-level rules get a decision record citing them. Files
never move.

- [001-architecture.md](001-architecture.md): overall mabel architecture
  (ledger, keys, Iroh sync, crates, storage, CLI, UIs, testing). Accepted.
- [002-unified-ledger.md](002-unified-ledger.md): replaces the person and
  organization ledger types with one ledger whose folded state is a principal
  set, rooted in either a raw key or one founding identity. Accepted;
  supersedes parts of 001 sections 3.1 to 3.6, 9 and 10.
- [003-wallet-ux-dns-and-trust-graph.md](003-wallet-ux-dns-and-trust-graph.md):
  on-ledger profiles (payload tag 17), DNS hostname verification, the local
  trust graph, and the wallet information architecture. Accepted; implements
  decisions 014, 015 and 016.
- [004-three-primitive-ui.md](004-three-primitive-ui.md): the UI collapses to
  three primitives (identity list, witness list, one identity page for local
  and foreign identities), a search box replaces the lookup and verify tabs,
  and a witnesses tab browses what a witness holds. Accepted.
- [005-public-profile-email-and-identity-components.md](005-public-profile-email-and-identity-components.md):
  the public profile gains an email, identity creation writes the profile as
  the second event, the alias becomes the private nickname, and the UI renders
  every identity through one inline component and one expandable card on a
  mobile-first single column. Accepted.
