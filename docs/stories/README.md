# Stories

End-to-end user stories, one scenario each, executable by hand and implemented
as Playwright specs under `tests/e2e/` (milestone 10 of
[../proposals/001-architecture.md](../proposals/001-architecture.md)). All
seven are implemented; each story names its spec, and a "Deviations" section
lists where that spec departs from the story text. Template:
[../templates/story.md](../templates/story.md).

The UI is three primitives and nothing else (proposal 004): the identity card
list, the witness card list, and one identity page at `/identities/<id>` for
every identity, local or foreign. Nav is `nav-wallet`, `nav-witnesses` and
`nav-node` on a wallet, and `nav-witness` and `nav-node` on a witness, which
serves no wallet. `/node` is six short rows of what `GET /api/node` answers,
`node-role` (the document's own word, `wallet` or `witness`),
`node-endpoint-id` (labelled `Iroh ID`), `node-relay` (`public relays` or
`direct connections only`), `node-storage`, `node-version`, and either
`node-identity-count` on a wallet or `node-ledger-count` and `node-fork-count`
on a witness, each a bare number under the row's own label. Where the API
listens is not a fact about the node's place in the network, so round 5 of
proposal 005 dropped `node-http-bind` from the page. `node-witnesses` is the
card list of the witnesses this node uses by default, `node-witnesses-empty`
reading `none` when it uses none. There is no verify screen, no lookup screen
and no identity selector, so a story that verified in the UI verifies on the CLI
instead.

The wallet home is three flat sections under three headings, divided by a rule
and never nested in cards (round 6 of proposal 005): "Open an identity"
(`wallet-search`, whose box is labelled `Mabel ID or handle`), "Your
identities" (`identity-list`, holding `identity-cards` and the folded
`identity-create`), and "Known identities" (`known-identities`, holding
`known-identity-cards` from `GET /api/identities/known` and the
`known-trusted-only` switch, which narrows the list to direct trust and crawl
distances). A known row is an identity this home has a record of and does not
control, so `known-identities-empty` reads `Your wallet knows of no other
identity yet.` on a wallet that has fetched, crawled and noted nobody.

Three facts of the UI every story after decision 017 depends on. Every action on
the identity page starts closed, so a step that uses a form clicks that
action's summary first: `action-trust`, `action-revoke`, `action-witnesses`,
`action-push`, `action-profile`, `action-handle`, `action-keys`,
`action-contact`, the four membership actions and `lookup-contact` on a foreign
page. `action-contact` and `lookup-contact` are both named `Update local info`
and hold one `contact-save` that writes the nickname and the note together. And
the header carries the app name and the nav and nothing else: the one control
that starts a graph sync is the `graph-sync` card on `/witnesses`, because a
sync reads what witnesses hold. There is no developer mode and no demo mode, so
a value the screen does not explain is read from the HTTP route instead.

An action is the shared collapsible, not a `details` element: the block carries
`data-state` reading `open` or `closed`, its `<action>-summary` is a `button`,
and a closed block holds none of its content, so a form inside it cannot be
filled before it is opened. One collapsible and one chevron cover every
expander in the app, and the chevron only ever turns over, never sideways: it
carries `data-slot="collapsible-chevron"` with `data-state` `open` or `closed`.
The expanders are `identity-create`, an identity card's
`identity-card-expand-<id>`, a ledger line's `event-expand-<seq>`, and the two
lists on a foreign page, `lookup-trust-toggle` and `lookup-reverse-toggle`.
`identity-card-expand-<id>` is a small icon button in the card's corner, so
`Show the record` and `Hide the record` are its `aria-label` rather than text on
the screen, and a card with nothing more to show draws no button at all.

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
`page.getByTestId('identity-detail-resolved').locator('[data-value]')` and
its `data-value` attribute. `textContent` on the testid element is also the
whole value, because the hidden middle characters stay in the DOM in an
`sr-only` span; what a reader sees truncated is drawn by CSS.

Every identity on every screen is drawn by one of two components (proposal
005), which is what makes the testids above predictable. The inline identity is
one line, `<testid>` with `<testid>-name`, `<testid>-nickname`,
`<testid>-verification` and the id inside it; the identity page's heading is
`identity-detail-resolved`, and a card in a list is `identity-card-name-<id>`.
A name reads `Alice Ashworth (alice)`: the name the identity publishes in
`<testid>-name`, then the nickname only this device keeps in `<testid>-nickname`,
in parentheses, and no element at all when there is no second name to draw. A
card has the width for a whole Mabel ID and a Mabel ID is the only thing that
tells two identities apart, so no card truncates one: the `Identifier` span
inside a card reads `data-truncated="false"`.

The pill is `your identity`, green `trusted` or amber `trusted (Nd)`, and its
`data-pill` attribute is `own`, `trusted` or `degree`; a screen with nothing to
say draws no pill. The pill keeps its testid, `<testid>-pill`, and sits in the
card's top right corner rather than inside the name, so a story reads it by its
own testid and never as text inside `identity-detail-resolved`. Beside it, a
card whose record this home does not store carries `<card>-unheld` reading `not
stored here`. The kind an identity declares is a badge on the card's first small
line, `<card>-declared-kind` with `data-declared-kind`. Proposal 005 also
removed five elements outright, so nothing in these stories reads them: the back
link, the declared-kind advisory sentence, the DNS advisory sentence, the
key-facts sentence and the name-provenance row.

An identity page draws its sections in this order: `identity-detail`, who this
identity trusts (`trust-panel`), the record (`ledger-panel`),
`principals-panel`, then the sections a foreign identity carries and
`identity-actions` last. `identity-detail` is the same card a list draws, opened
and without its toggle, and its rows are labelled in lowercase: `email`,
`nickname`, `note`, `created`, `handle`, `ledger`, `trusts`, `who can act for
it`, `invitations`. The email row is drawn only on an opened card, and `who can
act for it` only when the answer differs from the identity itself, which is what
an identity-rooted ledger is; an identity holding its own key draws no such row.
`trust-panel` is headed `Who <name> trusts` and described `Everyone it has said
it trusts and has not taken back.` `trust-list` holds one collapsed identity
card per subject, keyed by the subject's id, and an attestation taken back is
absent from it entirely: it stays on the record forever, and the record is where
it is read.

The ledger is compact `li` rows under `ledger-events`, eight to a page, and
nobody tunes that from the screen: round 5 removed the since box, the limit box
and the Load button. A record that fits on one page draws no footer at all, and
one that does not draws `ledger-footer` holding `ledger-previous`,
`ledger-page-<n>` and `ledger-next` and nothing else. No story builds a record
longer than eight entries, so the bar itself is pinned by
`ui/src/test/ledger-and-push.test.tsx` rather than here. How much of the record
this home holds is a sentence rather than a range: `ledger-event-count` counts
the entries it has, `ledger-not-fetched` reads `Your wallet holds none of this
record's N entries yet.` and `ledger-partial` says how many of how many it holds.
No screen names the position a record's newest entry sits at, so a story that
read `identity-detail-head-seq` or `identity-card-head-seq-<id>` reads `head_seq`
on `GET /api/identities/<id>` instead.

How you know them, on a foreign page, is `lookup-result`: a verdict line
(`lookup-degrees` reading `Connected through N steps` or `You trust them
directly`, or `lookup-degrees-none` reading `No connection found yet.` with
`lookup-degrees` inside it), a vertical chain of the same identity cards
(`lookup-path-<i>`, whose cards are `lookup-path-<i>-root` and
`lookup-hop-<i>-<j>`), and the two collapsed lists `lookup-trust` and
`lookup-reverse`. The reverse list is headed `Who your wallet has seen trusting
them`, and the caveat it used to carry is the sentence its info tip holds:
`lookup-reverse-note` has `aria-label` `Best effort: who your wallet has seen
trusting them, not everyone who does`.

The website is a handle everywhere a reader sees it. The identity page's row is
labelled `handle`, `action-handle` is where one is set (`handle-current`,
`handle-input`, `handle-submit`, the `handle-consent` panel whose confirm reads
`Publish the handle`, `handle-result`, and the TXT line to publish), and the
check lives in the same action as `verification-panel` with
`verification-status`, `verification-mark`, `verification-check`,
`verification-checked-at-ms` and `verification-detail`. `action-profile` changes
the public name and email only. The `hostname` in a testid is deliberate: the
document field is still `hostname`, so `identity-detail-hostname` and
`identity-detail-hostname-verification` keep their names.

- [001-two-people-meet.md](001-two-people-meet.md): two identities created in
  two wallet UIs, descriptors exchanged, one witness, mutual attestations, and
  a stranger verifying from an empty home.
- [002-shared-ledger.md](002-shared-ledger.md): an organization-declared ledger
  with a founder, an invitation admitted across two homes, and a verifier told
  which principal signed for the ledger.
- [003-revocation.md](003-revocation.md): trust taken back in the UI by naming
  the identity, read back by a fresh verifier with the flag-R wording, then
  re-attested. Verification is a CLI concern (proposal 004), so every report
  here is read on the CLI.
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
  names, the five DNS verification states, private contact notes, how you know a
  stranger on a foreign identity page, the identities this wallet knows of,
  opening an identity by hostname, and browsing what a witness holds. The one
  story that also needs
  [../../docker/compose.dns.yaml](../../docker/compose.dns.yaml), the test
  resolver overlay; its spec brings the topology up with that overlay itself.
