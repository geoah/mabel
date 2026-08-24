# 040: compose topology and story rewrites

- Status: open
- Depends on: 039

## Goal

The compose topology runs on witness identities: the entrypoint creates one,
advertises the container's endpoint on it and lists it in `witness_for`,
`MABEL_WITNESSES` holds Mabel ids with their endpoints, and the stories cover
the unified surface, a link with no witness at all and an endpoint rotation
(proposal 006 section 10).

## Scope

- `docker/entrypoint.sh` grows one step: create the witness identity, append an
  `EndpointAdvertisement` naming this container's endpoint, and list the identity
  in `node.json.witness_for`. `MABEL_PUBLISH_TICKET` writes a third file beside
  `<prefix>.ticket` and `<prefix>.id`, the witness's Mabel id.
  `MABEL_WITNESSES` holds `identity` and `endpoints` pairs where it held
  endpoint ids, feeding `mabel witness set-default`. The `role` line stays and is
  ignored.
- The ticket stays load-bearing and so does the two-phase bring-up in
  `tests/e2e/lib/docker.ts`: a witness identity exists no earlier than its
  endpoint did. `witnessId()` reads the new file.
- `docker/compose.yaml`, `docker/compose.two-witnesses.yaml`,
  `docker/compose.dns.yaml`, `docker/smoke.sh` and the zone files under
  `docker/dns/` all change. `smoke.sh` names the witness identity rather than the
  witness endpoint id and reads the ledger through `GET
  /api/identities/:identity_id`.
- The zone files gain `mabel-endpoints=` records beside their `mabel=` records,
  including one label whose endpoints are split across two character-strings.
- Story rewrites: 005 to the unified surface, losing every `witness-detail-*`
  assertion; 001 exchanging a link where it exchanges a descriptor; 004's two
  witnesses becoming two witness identities; 007 gaining a `mabel-endpoints=`
  record and a resolve-by-link case.
- Two new specs under `tests/e2e/specs/`: reaching an identity by link with no
  witness in the topology at all, and rotating a witness's endpoint through
  section 5.5 with a client that holds only the stale advertisement, which
  reaches nothing until it is handed a new record.
- `mabel serve` in the compose commands, with the hidden aliases unused.

## Acceptance criteria

- [ ] `docker/smoke.sh` is green against `docker/compose.yaml` from a clean
      volume: alice creates an identity, names the witness identity, pushes, and
      the witness and bob report the same head.
- [ ] The two-witnesses and DNS overlays bring up without `docker run` hand
      wiring, as ticket 032 left them.
- [ ] A container started from an old volume whose `node.json` carries `role`
      and hex `witnesses` fails to load with the message naming what to run, and
      a volume the entrypoint rewrites starts clean.
- [ ] The link story passes with every witness container stopped, proving an
      identity is reachable with no witness in the topology.
- [ ] The rotation story shows the stale client reaching nothing after step 4 of
      section 5.5 and reaching the new machine once given a fresh record.
- [ ] tests: push path unbroken and proved end to end here. The compose smoke,
      the seven rewritten and two new Playwright stories, the cargo suites,
      `cargo fmt` and `clippy` are all green.
- [ ] The demo script of ticket 017 still runs against the topology.
