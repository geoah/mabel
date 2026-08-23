# 024: DNS hostname verifier and its cache

- Status: done
- Depends on: 023

## Goal

The wallet node checks a claimed hostname against a `_mabel.<hostname>` TXT
record and caches an advisory verdict, per proposal 003 section 2. Witnesses
never verify.

## Scope

- A `Resolver` trait with one method resolving TXT records for an absolute
  name, implemented over `hickory-resolver` 0.26.1 (tokio) and stubbed in
  tests, so no test reaches the public internet.
- Query construction and matching exactly as section 2 makes normative:
  absolute name with the root label, search list disabled, per-record
  concatenation of character-strings, case-insensitive `mabel=` prefix, id
  parsed by the existing codec, CNAME to at most four links.
- The five statuses of section 2, all advisory and never gating ledger
  validity (decision 015).
- Cache at `verification/<identity_id>.json` with the fields section 2 lists,
  written atomically through the ticket 007 storage path, rebuildable.
- Hostname binding, decisive-result retention and the 24-hour `stale` rule of
  section 2.
- Re-check policy of section 2: the single-identity GET answers from cache and
  may start one background refresh; the list route is cache-only; the forced
  check waits. No background timer.
- The verifier service trait in `crates/mabel-node/src/api/`, so ticket 026
  wires routes to it. The witness surface labels a hostname as claimed only.

## Acceptance criteria

- [ ] The witness runtime performs no DNS lookup and exposes no verification
      status beyond the claim itself (section 2).
- [ ] tests, with a stub resolver, one case per status: `verified`,
      `mismatched`, `unverified`, `unreachable` and `unclaimed`.
- [ ] tests: character-strings within one record are concatenated and strings
      across two records are not; a `MABEL=` prefix and a mixed-case id both
      match; a non-`mabel=` record at the label yields `unverified`.
- [ ] tests: a five-link CNAME chain, a CNAME loop, a timeout and a resolver
      error each yield `unreachable`.
- [ ] tests: a cached entry whose `hostname` differs from the ledger's current
      `profile.hostname` is treated as absent.
- [ ] tests: an `unreachable` re-check leaves a `verified` or `mismatched`
      result in place and is recorded beside it with its own timestamp.
- [ ] tests: a `verified` result older than 24 hours is served with `stale:
      true`.
- [ ] tests: the query name is absolute and no search suffix is appended.
