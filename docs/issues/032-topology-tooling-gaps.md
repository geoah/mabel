# 032: topology tooling gaps

- Status: open
- Depends on: none

## Goal

Close the small infrastructure gaps the story work surfaced (stories 004,
005, 007): tickets, node-wide witnesses, and a second witness in compose.

## Scope

- `mabel node ticket [--addr <ip:port>]` prints this node's
  `EndpointTicket`, replacing the entrypoint's hand-assembled bytes;
  `docker/entrypoint.sh` uses it.
- `node.json.witnesses` becomes settable: a `MABEL_WITNESSES` env var in
  the entrypoint and a `mabel witness set-default <endpoint-id>...`
  command (or config edit path), feeding the crawler's source order
  (proposal 003 section 3 step 3).
- `docker/compose.two-witnesses.yaml` overlay adding a second witness
  publishing its ticket to the shared volume (stories 004 and 005).
- A DNS resolver container for story 007's e2e verification: compose
  overlay with a configured test resolver (image, zone with the
  `_mabel.<hostname>` TXT record, stable address, healthcheck), wallets
  started with their resolver pointed at it (ticket 024's Resolver seam
  covers unit tests only).
- `sync push` writes successful sources into `peers.json` hints (proposal
  003 section 3), retiring the dead `Peers.tickets` note from ticket 015.

## Acceptance criteria

- [ ] `mabel node ticket` output parses with `parse_peer_ticket` and the
      entrypoint uses it (demo still green).
- [ ] Stories 004 and 005 run against the overlay without `docker run`
      hand-wiring.
- [ ] tests: unit test for the ticket command; compose overlay verified by
      a real bring-up; cargo suites green.
