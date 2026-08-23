# 012: Full words in names

- Date: 2026-08-24
- Status: accepted
- Source: product owner

- Identifiers use full words: `organization`, not `org`, in enums, protobuf
  messages and fields, Rust types, JSON fields, and route paths.
- Applies to everything user-visible or schema-visible. CLI subcommands use
  the full word; short aliases are allowed as hidden conveniences.
- Existing `Org*` names are renamed as part of the ledger unification work
  rather than as a separate pass.
