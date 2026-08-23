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
#                          spaces or commas: the witnesses this node queries
#                          for any ledger, whatever a ledger's own chain names
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

# `node id` opens the home, or creates it with node.key when the volume is
# empty. Nothing in mabel rewrites node.json afterwards, so compose owns it and
# it is written on every start: an edited compose file takes effect on restart.
mabel node id >/dev/null
cat >"$home/node.json" <<JSON
{
  "role": "$role",
  "http_bind": "$http_bind",
  "witnesses": [],
  "storage_capacity": $capacity,
  "relay": "$relay"
}
JSON

# `witness set-default` validates every id and rewrites node.json's witness
# set, so a typo here fails the container instead of being stored.
if [ -n "${MABEL_WITNESSES:-}" ]; then
    # shellcheck disable=SC2086
    mabel witness set-default ${MABEL_WITNESSES//,/ } >/dev/null
    log "node-wide witnesses: ${MABEL_WITNESSES//,/ }"
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
done

log "running mabel $*"
exec mabel "$@"
