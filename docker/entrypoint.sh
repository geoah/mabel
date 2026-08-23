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
#   MABEL_IROH_PORT        UDP port this node's Iroh endpoint binds, 9070
#   MABEL_ADVERTISE_IP     IP the published ticket names, default this
#                          container's address on the compose network
#   MABEL_PUBLISH_TICKET   path prefix to write <prefix>.ticket and
#                          <prefix>.id to, for the witness
#   MABEL_WAIT_FOR_TICKET  path prefix to read <prefix>.ticket from; the
#                          ticket is appended to the command as --peer
#   MABEL_WAIT_SECONDS     how long to wait for that file, default 60

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

# A postcard varint, which is how the ticket encodes a UDP port.
varint_hex() {
    local value="$1" out=""
    while [ "$value" -ge 128 ]; do
        out+="$(printf '%02x' "$(((value & 127) | 128))")"
        value="$((value >> 7))"
    done
    printf '%s%02x' "$out" "$value"
}

# The `endpoint...` string `mabel --peer` takes, built from an endpoint id and
# one IPv4 socket address.
#
# mabel has no command that prints a ticket (see docker/README.md, "The ticket
# gap"), so the bytes are assembled here. The format is iroh-tickets 1.0.0's
# `EndpointTicket`: the lowercase kind prefix `endpoint`, then unpadded base32
# of postcard-encoded bytes, which are
#
#   00                  ticket wire format, Variant1
#   <32 bytes>          the endpoint id
#   01                  one transport address
#   01 00               TransportAddr::Ip, IPv4
#   <4 bytes> <varint>  the address and the port
#
# A wrong layout is loud, not silent: `mabel wallet serve --peer` exits 2 with
# reason `malformed_peer_ticket` and the service never becomes healthy.
endpoint_ticket() {
    local endpoint_id="$1" address="$2" port="$3"
    local id_hex address_hex hex escaped
    id_hex="$(printf '%s====' "$endpoint_id" | tr 'a-z' 'A-Z' | base32 -d |
        od -An -v -tx1 | tr -d ' \n')"
    if [ "${#id_hex}" -ne 64 ]; then
        log "$endpoint_id is not a 32-byte endpoint id"
        exit 1
    fi
    address_hex="$(printf '%s' "$address" |
        awk -F. '{ printf "%02x%02x%02x%02x", $1, $2, $3, $4 }')"
    hex="00${id_hex}010100${address_hex}$(varint_hex "$port")"
    escaped="$(printf '%s' "$hex" | sed 's/../\\x&/g')"
    # shellcheck disable=SC2059
    printf 'endpoint%s' \
        "$(printf "$escaped" | base32 -w0 | tr -d '=' | tr 'A-Z' 'a-z')"
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

endpoint_id="$(mabel node id)"
log "$role $endpoint_id, http $http_bind, iroh udp $iroh_port, relay $relay"

if [ -n "$publish" ]; then
    address="$(container_ip)"
    ticket="$(endpoint_ticket "$endpoint_id" "$address" "$iroh_port")"
    write_atomically "$publish.id" "$endpoint_id"
    write_atomically "$publish.ticket" "$ticket"
    log "published $publish.ticket for $address:$iroh_port"
fi

if [ -n "$wait_for" ]; then
    waited=0
    until [ -f "$wait_for.ticket" ]; do
        if [ "$waited" -ge "$wait_seconds" ]; then
            log "$wait_for.ticket did not appear within ${wait_seconds}s"
            exit 1
        fi
        sleep 1
        waited="$((waited + 1))"
    done
    ticket="$(cat "$wait_for.ticket")"
    log "seeding peer ticket from $wait_for.ticket"
    set -- "$@" --peer "$ticket"
fi

log "running mabel $*"
exec mabel "$@"
