# Stories

End-to-end user stories, one scenario each, written to be executed by hand and
then implemented as Playwright specs under `tests/e2e/` (milestone 10 of
[../proposals/001-architecture.md](../proposals/001-architecture.md)).
Template: [../templates/story.md](../templates/story.md).

Every story runs against the compose topology of
[../../docker/compose.yaml](../../docker/compose.yaml): one witness on
`http://127.0.0.1:9080` and two wallets on `http://127.0.0.1:9081` and
`http://127.0.0.1:9082`. Each node serves its own UI and its own API from that
origin, and the host port equals the container port because the API refuses any
`Host` that is not `127.0.0.1` or `localhost` on the port it bound. Assertion
strings come from [../../contracts/](../../contracts/README.md) and the
`data-testid` values from the components in `ui/src/`.

Reading an identifier in a spec: the `data-value` attribute holding the whole
52-character value sits on the `Identifier` span *inside* the element carrying
the testid, so a spec reads
`page.getByTestId('identity-detail-identity-id').locator('[data-value]')` and
its `data-value` attribute. `textContent` on the testid element is also the
whole value, because the hidden middle characters stay in the DOM in an
`sr-only` span; what a reader sees truncated is drawn by CSS.

- [001-two-people-meet.md](001-two-people-meet.md): two identities created in
  two wallet UIs, descriptors exchanged, one witness, mutual attestations, and
  a stranger verifying from an empty home.
- [002-shared-ledger.md](002-shared-ledger.md): an organization-declared ledger
  with a founder, an invitation admitted across two homes, and a verifier told
  which principal signed for the ledger.
- [003-revocation.md](003-revocation.md): trust revoked in the UI, read back by
  a fresh verifier with the flag-R wording, then re-attested.
- [004-fork-on-two-witnesses.md](004-fork-on-two-witnesses.md): two divergent
  branches on two witnesses, the fork record in the witness UI, and a
  multi-source verify that exits 20 naming both sources.
- [005-witness-operator.md](005-witness-operator.md): the witness debug route,
  paging, declared kinds, fork counts and read-only enforcement.
- [006-stale-append.md](006-stale-append.md): a shared-ledger append that lost
  the race, the exit-50 recovery, and the retry that lands.
- [007-profile-and-verification.md](007-profile-and-verification.md): display
  names, DNS verification states and lookup with degrees of separation. Draft,
  blocked on tickets 023-029.
