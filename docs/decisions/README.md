# Decisions

Decision records from the product owner. They override anything a research
doc or the hearsay reference implies. Hearsay (github.com/sirodoht/hearsay,
cloned at /tmp/hearsay) is the reference project; mabel rebuilds its ideas
without KERI.

- [001-scope.md](001-scope.md): POC scope and deliverables.
- [002-ledger.md](002-ledger.md): own minimal ledger, no KERI; event types;
  rotation deferred.
- [003-trust.md](003-trust.md): trust is one-way, recorded in the truster's
  ledger only.
- [004-organizations.md](004-organizations.md): invite plus signed
  acceptance; single-controller signing.
- [005-witnesses.md](005-witnesses.md): witnesses are passive replicas.
- [006-networking.md](006-networking.md): everything p2p over Iroh; default
  n0 relays acceptable.
- [007-encoding-and-protocols.md](007-encoding-and-protocols.md): protobuf
  allowed and suggested for events; gRPC optional, architect decides.
- [008-out-of-scope.md](008-out-of-scope.md): what this POC explicitly does
  not do.
- [009-doc-conventions.md](009-doc-conventions.md): numbered docs files with
  README indexes.
- [010-delivery-process.md](010-delivery-process.md): the seven phases and
  the dual (Opus plus Codex) review rule.
- [011-git-conventions.md](011-git-conventions.md): conventional commits,
  semver, frequent pushes.
- [012-naming-full-words.md](012-naming-full-words.md): full words in
  identifiers, `organization` not `org`.
- [014-wallet-ux.md](014-wallet-ux.md): address-book wallet, clean primary
  view, developer mode for the rest.
- [015-dns-verification.md](015-dns-verification.md): hostname linking via
  TXT records, cached daily verification, advisory.
- [016-trust-graph.md](016-trust-graph.md): crawled trust graph, degrees of
  separation, manual sync with staleness, configurable depth.

Number 013 was used briefly and retired.
- [017-plain-language-ui.md](017-plain-language-ui.md): UI copy is plain
  language, no middle dots or dashes as separators, no developer mode, and
  identity creation offers the secret keys for saving. Accepted.
- [018-explicit-exposure.md](018-explicit-exposure.md): a node answers
  loopback unless an operator names a host with `--allow-host`; the wallet has
  no authentication, so the operator owns the network boundary. Accepted.
- [019-mabel-id-prefix.md](019-mabel-id-prefix.md): an identity id shown to a
  person reads `mabel://<id>`; machine surfaces keep the bare id, and every
  field that takes an id takes the prefixed form too. Accepted.
- [020-endpoints-not-machines.md](020-endpoints-not-machines.md): the machines
  an identity advertises are called endpoints in prose as well as on the wire,
  and "node" means the running program alone. Accepted.
