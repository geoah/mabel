# Stories

End-to-end user stories, one scenario each, executable by hand and implemented
as Playwright specs under `tests/e2e/` (milestone 10 of
[../proposals/001-architecture.md](../proposals/001-architecture.md)). All
seven are implemented; each story names its spec, and a "Deviations" section
lists where that spec departs from the story text. Template:
[../templates/story.md](../templates/story.md).

The UI is three primitives and nothing else (proposal 004): the identity card
list, the witness card list, and one identity page at `/identities/<id>` for
every identity, local or foreign. Nav is `nav-wallet` and `nav-witnesses`.
There is no verify screen, no lookup screen and no identity selector, so a
story that verified in the UI verifies on the CLI instead.

Every story starts from the compose topology of
[../../docker/compose.yaml](../../docker/compose.yaml): one witness on
`http://127.0.0.1:9080` and two wallets on `http://127.0.0.1:9081` and
`http://127.0.0.1:9082`. Three stories add to it: 004 and 005 hand-start a
second witness and a second machine for alice, and 007 brings the topology up
again with the test resolver overlay. Each node serves its own UI and its own API from that
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
  a fresh verifier with the flag-R wording, then re-attested. Verification is a
  CLI concern (proposal 004), so every report here is read on the CLI.
- [004-fork-on-two-witnesses.md](004-fork-on-two-witnesses.md): two divergent
  branches on two witnesses, the fork record in the witness UI, and a
  multi-source verify that exits 20 naming both sources.
- [005-witness-operator.md](005-witness-operator.md): the witness debug route
  as the card list and the identity page, declared kinds, fork counts,
  read-only enforcement, and the paging the route still answers after the
  controls left the screen.
- [006-stale-append.md](006-stale-append.md): a shared-ledger append that lost
  the race, the exit-50 recovery, and the retry that lands.
- [007-profile-and-verification.md](007-profile-and-verification.md): display
  names, the five DNS verification states, private contact notes, degrees of
  separation on a foreign identity page, opening an identity by hostname, and
  browsing what a witness holds. The one story that also needs
  [../../docker/compose.dns.yaml](../../docker/compose.dns.yaml), the test
  resolver overlay; its spec brings the topology up with that overlay itself.
