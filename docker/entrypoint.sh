#!/usr/bin/env bash
# The container entrypoint: prepare the node home, publish or wait for the
# witness ticket, then exec `mabel <command>` (ticket 015).
#
# Environment, all optional:
#   MABEL_HOME             node home, /data in the image
#   MABEL_ROLE             wallet (default) or witness, written to node.json
#   MABEL_HTTP_BIND        node.json http_bind, default 0.0.0.0:9080
#   MABEL_RELAY            node.json relay, disabled (default) or n0
#   MABEL_STORAGE_CAPACITY node.json storage_capacity in bytes
#   MABEL_WITNESSES        node.json witnesses, endpoint ids separated by
#                          spaces or commas: the machines this node pushes to
#                          for any ledger, whatever a ledger's own chain names
#   MABEL_WITNESS_ALIAS    local alias of the identity a witness home witnesses
#                          for, minted on first start, default witness
#   MABEL_IROH_PORT        UDP port this node's Iroh endpoint binds, 9070
#   MABEL_ADVERTISE_IP     IP the published ticket names, default this
#                          container's address on the compose network
#   MABEL_PUBLISH_TICKET   path prefix to write <prefix>.ticket and
#                          <prefix>.id to, for the witness
#   MABEL_WAIT_FOR_TICKET  path prefixes to read <prefix>.ticket from,
#                          separated by spaces or commas; each ticket is
#                          appended to the command as --peer
#   MABEL_WAIT_SECONDS     how long to wait for each of those files, default 60

set -euo pipefail

home="${MABEL_HOME:-/data}"
role="${MABEL_ROLE:-wallet}"
http_bind="${MABEL_HTTP_BIND:-0.0.0.0:9080}"
relay="${MABEL_RELAY:-disabled}"
capacity="${MABEL_STORAGE_CAPACITY:-2147483648}"
iroh_port="${MABEL_IROH_PORT:-9070}"
publish="${MABEL_PUBLISH_TICKET:-}"
wait_for="${MABEL_WAIT_FOR_TICKET:-}"
wait_seconds="${MABEL_WAIT_SECONDS:-60}"

log() { printf 'entrypoint: %s\n' "$*" >&2; }

# This container's address on the compose network, which is the address the
# ticket names. /etc/hosts carries it under the container hostname.
container_ip() {
    if [ -n "${MABEL_ADVERTISE_IP:-}" ]; then
        printf '%s' "$MABEL_ADVERTISE_IP"
        return
    fi
    local address
    address="$(getent ahostsv4 "$(hostname)" | awk 'NR == 1 { print $1 }')"
    if [ -z "$address" ]; then
        log "this container has no IPv4 address, set MABEL_ADVERTISE_IP"
        exit 1
    fi
    printf '%s' "$address"
}

write_atomically() {
    local path="$1" content="$2"
    printf '%s\n' "$content" >"$path.writing"
    mv "$path.writing" "$path"
}

# node.json, with the identities this home witnesses for. Compose owns the file
# and it is written on every start, so an edited compose file takes effect on
# restart.
write_node_json() {
    local witness_for="$1" legacy="$2"
    cat >"$home/node.json" <<JSON
{
  "role": "$role",
  "http_bind": "$http_bind",
  "witnesses": [],
  "witness_for": $witness_for,
  "accept_legacy_witness_config": $legacy,
  "storage_capacity": $capacity,
  "relay": "$relay"
}
JSON
}

# The identity id this home records under `alias`, empty when it holds none.
# `identity show` prints one `identity_id` at the top level; every other id in
# that document has another key. An unknown alias is not an error here, so the
# refusal is swallowed rather than failing the container.
identity_id_of() {
    local shown
    shown="$(mabel identity show "$1" --json 2>/dev/null)" || return 0
    printf '%s\n' "$shown" |
        sed -n 's/^[[:space:]]*"identity_id": "\([^"]*\)".*/\1/p' |
        tail -1
}

# `node id` opens the home, or creates it with node.key when the volume is
# empty. It runs first, because minting the witness identity below needs a home.
mabel node id >/dev/null
write_node_json "[]" false

# A witness is an identity, not an endpoint (proposal 006 section 1). A witness
# home mints one, publishes the machine that answers for it on that identity's
# own ledger, and names it in `node.json.witness_for`: that is what admits a
# push whose witness set names the identity (section 4), and the advertisement
# is what section 4.1 requires before this home takes a ledger it does not
# already store. The alias is stable, so a restart reuses the identity on the
# volume rather than minting a second one.
#
# `accept_legacy_witness_config` goes on with it: a ledger written before
# witnesses were identities carries a tag-11 list of endpoint ids, and this home
# is the machine those lists name. The switch is a migration switch and goes
# with the last such ledger.
witness_identity=""
if [ "$role" = "witness" ]; then
    witness_alias="${MABEL_WITNESS_ALIAS:-witness}"
    witness_identity="$(identity_id_of "$witness_alias")"
    if [ -z "$witness_identity" ]; then
        mabel identity create --alias "$witness_alias" --kind service >/dev/null
        witness_identity="$(identity_id_of "$witness_alias")"
        if [ -z "$witness_identity" ]; then
            log "the witness identity could not be created"
            exit 1
        fi
        log "minted witness identity $witness_identity as $witness_alias"
    fi
    # `auto` is this container's own endpoint id, which is the machine a pusher
    # must dial for this identity. It is replayed on every start, so a
    # regenerated node.key is advertised rather than leaving the old one on the
    # ledger; an unchanged list is refused as a no-op, which is not an error
    # here, and anything else is.
    if advertised="$(mabel identity endpoints replace --identity "$witness_alias" \
        --endpoints auto --json 2>&1)"; then
        log "advertised this machine for $witness_alias"
    elif printf '%s' "$advertised" | grep -q no_op_endpoint_advertisement; then
        log "$witness_alias already advertises this machine"
    else
        log "advertising this machine for $witness_alias failed: $advertised"
        exit 1
    fi
    write_node_json "[\"$witness_identity\"]" true
    log "witnessing for $witness_identity"
fi

endpoint_id="$(mabel node id)"
log "$role $endpoint_id, http $http_bind, iroh udp $iroh_port, relay $relay"

if [ -n "$publish" ]; then
    address="$(container_ip)"
    ticket="$(mabel node ticket --addr "$address:$iroh_port")"
    write_atomically "$publish.id" "$endpoint_id"
    write_atomically "$publish.ticket" "$ticket"
    log "published $publish.ticket for $address:$iroh_port"
fi

# The endpoints this node pushes to: what `MABEL_WITNESSES` names, plus the
# machine behind each ticket waited for below. A push reads them from
# `node.json`, so a wallet that only has a ticket would have nothing to dial.
push_to="${MABEL_WITNESSES:-}"
push_to="${push_to//,/ }"

# Each prefix is waited for in turn and seeded as one --peer, so a wallet in
# the two-witnesses overlay starts knowing where both witnesses are.
for prefix in ${wait_for//,/ }; do
    waited=0
    until [ -f "$prefix.ticket" ]; do
        if [ "$waited" -ge "$wait_seconds" ]; then
            log "$prefix.ticket did not appear within ${wait_seconds}s"
            exit 1
        fi
        sleep 1
        waited="$((waited + 1))"
    done
    log "seeding peer ticket from $prefix.ticket"
    set -- "$@" --peer "$(cat "$prefix.ticket")"
    if [ -f "$prefix.id" ]; then
        push_to="$push_to $(cat "$prefix.id")"
    fi
done

# `witness set-default` validates every id and rewrites node.json's witness
# set, so a typo fails the container instead of being stored.
if [ -n "${push_to// /}" ]; then
    # shellcheck disable=SC2086
    mabel witness set-default $push_to >/dev/null
    log "pushing to $push_to"
fi

log "running mabel $*"
exec mabel "$@"
