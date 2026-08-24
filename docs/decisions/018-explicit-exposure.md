# 018: exposing a node is an explicit operator act

- Date: 2026-08-24
- Status: accepted
- Source: product owner

A node answers loopback and nothing else unless an operator names another
host. `mabel wallet serve --allow-host <host[:port]>` and `mabel witness run
--allow-host <host[:port]>` add that value to the `Host` header set the
loopback middleware accepts, and add the matching `http` and `https` origins
to the `Origin` set. Both flags are repeatable.

The default is unchanged: no flag and no `allowed_hosts` in `node.json` means
`127.0.0.1:<port>` and `localhost:<port>` alone, and every other `Host` is
refused with 403 and reason `host_not_loopback`. The rejection names every
value that would have been accepted, the allowed hosts included.

`node.json` carries the same set as `allowed_hosts`, and the two merge: the
flag adds to the file rather than replacing it, so running once with
`--allow-host` cannot silently drop the name an operator recorded. Comparison
is by whole string after the trim-and-lowercase the `Host` rule already
applies, so `wallet.example` does not accept `wallet.example:8443`.

The wallet has no authentication. Anyone who can reach an allowed host can use
the keys of that node, and the flag is where the operator says they accept
that: the network boundary is theirs to draw, with a tailnet, a firewall or a
reverse proxy that authenticates. The node states what it accepts on startup
and refuses everything else.

Rules:

- No flag, no exposure. A node that was never told a host answers loopback.
- One flag per name. There is no wildcard and no "any host" value.
- The flag widens the `Host` and `Origin` sets and nothing else. It does not
  change what the API binds to, which is `--http` and `http_bind`, and it does
  not add authentication.
