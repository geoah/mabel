# 031: admitted controller acts from their own home

- Status: open
- Depends on: none

## Goal

After bob accepts an invitation and alice admits him, bob can fetch the
shared ledger into his own home and append to it (trust, membership,
witness config) signing with his own active key. Today only the founder's
home can act: `controlled_by` is written solely by `identity create
--founder`, and command resolution requires a local `identities/<id>/`
directory (story 002 asserts this as a known limit).

## Scope

- `mabel sync fetch` (and the wallet service fetch path) recognizes a
  fetched ledger whose folded CONTROLLER set includes a local identity's
  key, and records the `controlled_by` link so the ledger becomes
  actionable.
- Command resolution accepts such ledgers everywhere `--ledger`/issuer
  aliases resolve, without requiring identity key files for the ledger
  itself.
- The append discipline treats these ledgers as shared (never solely
  controlled).

## Acceptance criteria

- [ ] The story 002 flow continues on bob's home: fetch, then bob appends
      a trust attestation to the shared ledger, and a fresh verifier sees
      bob as the signing principal.
- [ ] A fetched ledger with no local controller key stays read-only, with
      a clear error naming why.
- [ ] tests: an integration test covering the two-home controller flow;
      cargo test -p mabel-cli and -p mabel-node green.
