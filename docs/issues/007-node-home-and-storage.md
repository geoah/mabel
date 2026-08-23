# 007: node home, storage layout, keys and permissions

- Status: done
- Depends on: 002

## Goal

`mabel-node` owns a node home on disk with the exact layout of proposal 001
section 8, a typed `node.json`, atomic writes, the permission rules, key
generation and a rebuildable head cache.

## Scope

- Home resolution: `$MABEL_HOME`, then `~/.mabel`, overridable with `--home`
  (section 8).
- Every path in the section 8 tree: `node.json`, `node.key`,
  `identities/<id>/meta.json`, `identities/<id>/{active,reserve}.key`,
  `ledgers/<id>/<zero-padded-seq>.ev`, `ledgers/<id>/head.json`,
  `ledgers/<id>/meta.json`, `forks/<id>/<seq>-<event_id>.fork`, `peers.json`.
  The fork file name carries the conflicting event's id, not the kept one.
- Typed `node.json`: `role` (`wallet` or `witness`), `http_bind`, `witnesses`,
  `storage_cap` defaulting to 2 GiB (section 5), and `relay` with values `n0`
  (default) or `disabled`; an unknown field or value is a load error.
- Key generation: 32 bytes from `getrandom` into `SecretKey::from_bytes` for
  the node key and for each identity's active and reserve key; the node key is
  distinct from every identity key (sections 4 and 12).
- Atomic writes (temp file, fsync, rename); a multi-event append renames
  `head.json` last (section 8).
- Permissions: directories 0700, key files 0600; a group- or world-readable key
  file is an error carrying exit code 60 unless
  `--allow-insecure-permissions` is passed (section 8).
- Read paths "read all" and "read from seq N" served by the sorted directory
  listing; events are served to peers as the bytes on disk, unmodified
  (section 8).
- `head.json` and the ledger `meta.json` provenance record (source endpoint,
  first seen).

## Acceptance criteria

- [x] Files land at the exact paths section 8 names, with sequence-ordered
      event file names so directory order is chain order.
- [x] A crash between event writes and the `head.json` rename leaves a shorter
      but valid ledger (section 8).
- [x] Deleting `head.json` and reopening rebuilds it from the event files.
- [x] Reading an event returns the stored bytes with no decode-then-encode
      round trip (section 3.1, byte authority).
- [x] No database and no index beyond the sorted listing (section 8).
- [x] tests: `node.json` round-trips with defaults applied, and `relay:
      "sometimes"` is rejected with a load error.
- [x] tests: unit tests over a `tempfile` home cover atomic append and crash
      truncation, head rebuild, fork file naming by conflicting event id, 0700
      and 0600 enforcement on create, the exit-60 condition for a 0644 key file
      and the `--allow-insecure-permissions` bypass (section 11).
