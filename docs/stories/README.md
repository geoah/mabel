# Stories

End-to-end user stories, one scenario each, executable by hand and implemented
as Playwright specs under `tests/e2e/` (milestone 10 of
[../proposals/001-architecture.md](../proposals/001-architecture.md)). All nine
are implemented; each story names its spec, and a "Deviations" section lists
where that spec departs from the story text. Template:
[../templates/story.md](../templates/story.md).

A witness is an identity, not an endpoint (proposal 006 section 1).
Every story reads two ids where it used to read one: `witness_identity`, the
Mabel id a `WitnessSet` records and `witness add --witness` takes, and
`witness_id`, the Iroh endpoint id of the container answering for it, which is
what a push dials and what `--from` pins. The compose entrypoint publishes both
beside the ticket, as `/shared/witness.identity` and `/shared/witness.id`. It
also publishes a display name on the witness identity's own record before the
advertisement, `Witness one` for the `witness` container and `Witness two` for
`witness-two`, so a witness reads as somebody rather than as an id. That record
is therefore inception, then the name, then the endpoints, and its head sits at
seq 2 on a container that has just started.

The UI is three primitives and nothing else (proposal 004): the identity card
list, the witness card list, and one identity page at `/identities/<id>` for
every identity, local or foreign. Every node serves the same nav and the same
home: `nav-wallet`, `nav-witnesses` and `nav-node`, three entries and no fourth,
on a node that signs for nothing as much as on one that signs for ten. There is
no `nav-witness`, no `/witness` route, and `/witnesses/<id>` redirects to
`/identities/<id>`, because a witness is an identity and its page is the
identity page (proposal 006 section 8). `/node` is short rows of what `GET
/api/node` answers: `node-endpoint-id` (labelled `Iroh ID`), `node-relay`
(`public relays` or `direct connections only`), `node-identity-count`
(`identities`), `node-witness-for` (`keeps records for`, holding one inline
identity per entry or the word `none`), `node-ledger-count` (`records`),
`node-fork-count` (`conflicts`), `node-storage` and `node-version`. No document
names a role and no screen draws one: what a node can do is read from what it
holds. A home with no key of its own says so in one sentence, `node-no-keys`.
Where the API listens is not a fact about the node's place in the network, so
round 5 of proposal 005 dropped `node-http-bind` from the page.
`node-witnesses` is the card list of the witnesses this node uses by default,
`node-witnesses-empty` reading `none` when it uses none. There is no verify
screen, no lookup screen and no identity selector, so a story that verified in
the UI verifies on the CLI instead.

A section is a heading, an optional one-line description and its content, and it
draws no border: only the leaf inside it does, which is a card, a form input or a
notice, so no screen draws a border inside a border. A section's description
carries a testid where a story reads it, `trust-panel-description`,
`known-identities-note` and `principals-description`.

The wallet home is three flat sections under three headings, divided by a rule
and never nested in cards (round 6 of proposal 005): "Open an identity"
(`wallet-search`, whose box is labelled `Mabel ID, handle or link` and whose
placeholder reads `alice.example, or paste a Mabel ID or a link`; it takes a
Mabel ID, a handle or a `mabel://` link, and the browser parses none of them:
the box hands the string to the node, which owns the grammar), "Your
identities" (`identity-list`, holding `identity-cards` and the folded
`identity-create`), and "Known identities" (`known-identities`, holding
`known-identity-cards` from `GET /api/identities/known` under the tab row
`known-identities-filter`, whose two tabs are `known-identities-all` and
`known-identities-trusted`; the second narrows the list to direct trust and
crawl distances). A known row is an identity this home has a record of and
does not control, so `known-identities-empty` reads `Your wallet knows of no other
identity yet.` on a wallet that has fetched, crawled and noted nobody, and
`known-identities-note` reads `This is what this home holds. A record missing
here may still be on another witness.`, the sentence that came off the witness
route when that route went away. A home that keeps other people's records lists
them here: that is the whole of what a witness operator reads.

Three facts of the UI every story after decision 017 depends on. Every action on
the identity page starts closed, so a step that uses a form clicks that
action's summary first: `action-trust`, `action-revoke`, `action-witnesses`,
`action-push`, `action-profile`, `action-handle`, `action-keys`,
`action-contact`, `action-endpoints`, `action-share`, the four membership
actions and `lookup-contact` on a foreign page. `action-contact` and `lookup-contact` are both named `Update local info`
and hold one `contact-save` that writes the nickname and the note together. And
the header carries the app name and the nav and nothing else: the one control
that starts a graph sync is the `graph-sync` card on `/witnesses`, because a
sync reads what witnesses hold. There is no developer mode and no demo mode, so
a value the screen does not explain is read from the HTTP route instead.

The actions sit under five group headings rather than one "What you can do",
which no story reads any more: `action-group-profile` (`Profile`),
`action-group-trust` (`Trust`), `action-group-witnesses` (`Witnesses and sync`),
`action-group-reach` (`Reaching this identity`, holding `action-endpoints` and
`action-share`) and `action-group-control` (`Control and keys`). Every `action-<name>` and
`<action>-summary` kept its testid, so only the heading a story reads changed.

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
`http://127.0.0.1:9082`. Four stories add to it: 004 and 005 bring the second
witness up with the
[compose.two-witnesses.yaml](../../docker/compose.two-witnesses.yaml) overlay
and hand-start a second machine for alice, 007 brings the topology up again
with the test resolver overlay, and 008 and 009 hand-start one borrowed home
each. Each node serves its own UI and its own API from that
origin, and the host port equals the container port because the API refuses any
`Host` that is not `127.0.0.1` or `localhost` on the port it bound. Assertion
strings come from [../../contracts/](../../contracts/README.md) and the
`data-testid` values from the components in `ui/src/`.

Where a list is narrowed by choosing between two or three of its own shapes, the
control is a row of tabs and not a row of toggles: the row is a `tablist` with a
testid of its own, each tab is a `role="tab"` carrying `aria-selected` `true` or
`false`, and the panel that is not selected is not in the DOM at all, so only
the chosen list can be read. There are two such rows, `known-identities-filter`
on the wallet home and `witness-holdings-filter` on a witness's page.

A Mabel identity id put in front of a person reads `mabel://<id>` (decision
019): the identity page heading, every card, every inline identity, the entry
contents a reader opens, and every CLI line outside `--json`. An endpoint id
names a machine rather than an identity and stays bare under its own label, and
so do public keys and entry ids. The prefix is display only, so `data-value`,
`--json` documents, HTTP bodies, `node.json` and the ids inside a DNS record
value all carry the bare 52 characters, and a spec reading an id through
`[data-value]` reads it exactly as it did before.

Reading an identifier in a spec: the `data-value` attribute holding the whole
52-character value sits on the `Identifier` span *inside* the element carrying
the testid, so a spec reads
`page.getByTestId('identity-detail-resolved').locator('[data-value]')` and
its `data-value` attribute. `textContent` on the testid element is also the
whole value, because the hidden middle characters stay in the DOM in an
`sr-only` span; what a reader sees truncated is drawn by CSS. The visible text
of a Mabel ID carries the prefix and the `data-value` does not, so a spec that
compares `textContent` to an id compares it to `mabel://<id>`.

Every identity on every screen is drawn by one of two components (proposal
005), which is what makes the testids above predictable. The inline identity is
one line, `<testid>` with `<testid>-name`, `<testid>-nickname`,
`<testid>-verification` and the id inside it; the identity page's heading is
`identity-detail-resolved`, and a card in a list is `identity-card-name-<id>`.
A name reads `Alice Ashworth (alice)`: the name the identity publishes in
`<testid>-name`, then the nickname only this device keeps in `<testid>-nickname`,
in parentheses, and no element at all when there is no second name to draw.
`<testid>-name` is always drawn: an identity that publishes no name and that
this device has never named is titled with the first eight characters of its id
and an ellipsis, and that element carries `data-placeholder-name="true"` where
a real name carries `data-placeholder-name="false"`. The stand-in title is not
an id being shown, so it takes no `mabel://` prefix; the whole prefixed id is
under it as on every other card. A
card has the width for a whole Mabel ID and a Mabel ID is the only thing that
tells two identities apart, so no card truncates one: the `Identifier` span
inside a card reads `data-truncated="false"`.

The pill is `your identity`, green `trusted` or amber `trusted (Nd)`, and its
`data-pill` attribute is `own`, `trusted` or `degree`; a screen with nothing to
say draws no pill. The pill keeps its testid, `<testid>-pill`, and sits in the
card's top right corner rather than inside the name, so a story reads it by its
own testid and never as text inside `identity-detail-resolved`. Beside it, a
card whose record this home does not store carries `<card>-unheld` reading `not
stored here`. The kind an identity declares is a badge that leads that same row,
`<card>-declared-kind` and `identity-detail-declared-kind`, both with
`data-declared-kind`: what an identity says it is sorts one card from another,
so it comes before the pills about trust rather than beside the name. Under the
name line, `<card>-kind-line` holds whatever the
listing that drew the card carries, which is how many entries a witness holds of
a record and how many conflicts it recorded: a plain wallet card passes no such
markers and draws no `identity-card-kind-line-<id>` at all, and no identity page
draws `identity-detail-kind-line`. Proposal 005 also removed five elements
outright, so nothing in these stories reads them: the back link, the
declared-kind advisory sentence, the DNS advisory sentence, the key-facts
sentence and the name-provenance row. Every witness screen went with them: there
is no witness detail page and no back link anywhere.

A card that routes somewhere is one stretched anchor: `<testid>-link` and
`identity-card-link-<id>` sit on the card's title, with the href they had
before. That is the published name, the stand-in title when there is none, and
never the id.
Clicking anywhere on the card navigates, and the keyboard reaches the same page,
so no story needs a forced click or a click on a `div`. Every control on a card
sits above that anchor and keeps its own click, including the button beside an
id, which names what it copies: `Copy Mabel ID` or `Copy Iroh ID`, swapped for
`<label>: copied` for two seconds after a copy.

An identity page draws its sections in this order: `identity-detail`, who this
identity trusts (`trust-panel`), the record (`ledger-panel`),
`principals-panel`, then the sections a foreign identity carries and
`identity-actions` last. `identity-detail` is the same card a list draws, opened
and without its toggle, and its rows are labelled in lowercase: `email`,
`nickname`, `note`, `created`, `handle`, `ledger`, `trusts`, `who can act for
it`, `invitations`. The email row is drawn only on an opened card, and `who can
act for it` only when the answer differs from the identity itself, which is what
an identity-rooted ledger is; an identity holding its own key draws no such row.
`trust-panel` is headed `Who <name> trusts`, `trust-panel-description` reads
`People this identity currently trusts.` and `trust-list-empty` reads `This
identity does not trust anyone yet.` `trust-list` holds one collapsed identity
card per subject, keyed by the subject's id, and an attestation taken back is
absent from it entirely: it stays on the record forever, and the record is where
it is read.

A closed ledger line carries two things and no more, `event-seq-<seq>` and
`event-gloss-<seq>`, the plain clause naming what the entry did (`created this
identity`, `chose who keeps a copy`, `said it trusts someone`, and `did something
this version does not know about` for a kind this build has no gloss for). The
raw kind string, the entry id, the entry before it, the signing time and the
payload all live inside the opened entry, so a story reading
`event-payload-kind-<seq>`, `event-id-<seq>` or `event-payload-<seq>` clicks
`event-expand-<seq>` first; a closed line has no such element in the DOM at all.

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
trusting them, not everyone who does`. That info tip sits inside the same row as
the toggle and stops the click there, so a list is opened by clicking
`lookup-trust-label` or `lookup-reverse-label`, the heading a reader aims at,
rather than the middle of the row, which can land on the icon.

`/witnesses` is a card list of the witness identities this home knows,
`witness-cards`, each drawn by the same identity card every other screen draws.
A witness this node uses by default carries
`witness-default-<identity id>` reading `this node uses it by default`. The
endpoints that answer for one are rows of its record, labelled `endpoint`, so a
list card shows them once it is opened:
`identity-card-machine-<endpoint>-<identity id>` holds the endpoint's Iroh ID
and `identity-card-machine-<endpoint>-note-<identity id>` holds one of two
sentences, `This endpoint is listed on this identity's own record.` or `No
record we have confirms that this endpoint answers for it.` The identity page
draws the same rows as `identity-detail-machine-<endpoint>` and
`identity-detail-machine-<endpoint>-note`. The `machine` inside those testids is
the older spelling of the same row and did not change with the label.
`binding`, `verified` and `hinted` are API words and stop at the API.

A witness's page is its identity page, and what it keeps for other people is a
section of it, `witness-holdings`, asked live over the sync protocol when the
page loads. Above the list, `witness-chosen-by` reads `N of your identities` and
`witness-node-default` reads `yes, for the identities that chose no witness of
their own` or `no`. The list sits under the tab row `witness-holdings-filter`,
whose three tabs are `witness-holdings-all`, `witness-holdings-trusted` and
`witness-holdings-ours` in that order, labelled `All`, `Trusted` and `Yours`,
with `All` chosen when the page opens; the chosen tab's own sentence is the
section's description, and
`witness-holdings-empty` reads `This witness holds no record.` under `All` and
`No record it holds matches this.` under the other two. `witness-holdings-error`
and `witness-unreachable` are what it says instead when the endpoints answering
for that witness cannot be reached.

A conflict is a fact about a stored record, and `GET /api/forks` is the one
route that reports one, on every node. No screen draws a fork record: the
witness detail page and its Forks card went with the witness routes, and what a
list still says is `identity-card-fork-count-<id>` on a witness's holdings.

The website is a handle everywhere a reader sees it. The identity page's row is
labelled `handle`, `action-handle` is where one is set (`handle-current`,
`handle-input`, `handle-submit`, the `handle-consent` panel whose confirm reads
`Publish the handle`, `handle-result`, and the TXT lines to publish:
`handle-txt-record` always, and `handle-txt-endpoints-record` beside it only
when the identity advertises an endpoint. The ids inside a record value stay
bare, because `mabel=` and `mabel-endpoints=` are defined over bare ids), and
the check lives in the same action as `verification-panel` with
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
  branches on two witness identities, the fork record on `GET /api/forks`, and
  a multi-source verify that exits 20 naming both sources.
- [005-witness-operator.md](005-witness-operator.md): what a witness holds,
  read on the one home every node draws, declared kinds, the conflict it
  recorded, the routes the witness screens used answering 404, and an old
  `node.json` refused rather than misread.
- [006-stale-append.md](006-stale-append.md): a shared-ledger append that lost
  the race, the exit-50 recovery, and the retry that lands.
- [007-profile-and-verification.md](007-profile-and-verification.md): display
  names, the five DNS verification states, private contact notes, how you know a
  stranger on a foreign identity page, the identities this wallet knows of,
  opening an identity by hostname, and browsing what a witness holds. The one
  story that also needs
  [../../docker/compose.dns.yaml](../../docker/compose.dns.yaml), the test
  resolver overlay; its spec brings the topology up with that overlay itself.
- [008-link-with-no-witness.md](008-link-with-no-witness.md): an endpoint
  published on an identity's own record, a `mabel://` link handed over with a
  ticket beside it, and a home that knows nobody reading that record with every
  witness container stopped.
- [009-endpoint-rotation.md](009-endpoint-rotation.md): a witness identity
  moved to a second endpoint through proposal 006 section 5.5, the client that
  was never handed the out-of-band update reaching nothing, and the fresh record
  that recovers it.
