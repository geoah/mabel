#!/usr/bin/env bash
# The container entrypoint: prepare the node home, publish or wait for the
# witness ticket, then exec `mabel <command>` (ticket 015).
#
# Environment, all optional:
#   MABEL_HOME             node home, /data in the image
#   MABEL_ROLE             wallet (default) or witness. Read here and written
#                          nowhere: a witness home mints a witness identity,
#                          publishes a name and this container's endpoint on
#                          it, and lists it in node.json.witness_for. What a
#                          node can do is read from what its home holds
#                          (proposal 006 section 8), so node.json carries no
#                          role line at all.
#   MABEL_HTTP_BIND        node.json http_bind, default 0.0.0.0:9080
#   MABEL_RELAY            node.json relay, disabled (default) or n0
#   MABEL_STORAGE_CAPACITY node.json storage_capacity in bytes
#   MABEL_WITNESSES        node-wide witnesses, one entry per witness identity,
#                          entries separated by spaces:
#                            <mabel id>=<endpoint id>[,<endpoint id>...]
#                          Each entry is one `mabel witness set-default` call,
#                          so node.json names an identity and the machines that
#                          answer for it (proposal 006 section 5.4). An entry
#                          for an identity a waited-for prefix already
#                          published adds its endpoints to that one.
#   MABEL_WITNESS_ALIAS    local alias of the identity a witness home witnesses
#                          for, minted on first start, default witness
#   MABEL_IROH_PORT        UDP port this node's Iroh endpoint binds, 9070
#   MABEL_ADVERTISE_IP     IP the published ticket names, default this
#                          container's address on the compose network
#   MABEL_PUBLISH_TICKET   path prefix to write <prefix>.ticket, <prefix>.id
#                          and <prefix>.identity to, for the witness. Its last
#                          path segment is also the name the witness identity
#                          publishes: witness reads as Witness one.
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
# restart, and a volume carrying the pre-proposal-006 file (a `role` line and
# 64-character hex ids under `witnesses`) is rewritten into the shape the node
# loads before anything reads it.
#
# No `role` and no `accept_legacy_witness_config`: one node serves one API, and
# no ledger in this topology was written before witnesses were identities, so
# the legacy admission clause has nothing to admit.
write_node_json() {
    local witness_for="$1"
    cat >"$home/node.json" <<JSON
{
  "http_bind": "$http_bind",
  "witnesses": [],
  "witness_for": $witness_for,
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

# The name a witness identity publishes, taken from the path prefix this
# container publishes its ticket under so that two witnesses in one compose file
# do not read as the same person. `witness` is the first one, `witness-two` the
# second, and any other prefix is used as it is written.
witness_display_name() {
    local label="${1##*/}"
    label="${label//[-_]/ }"
    if [ "$label" = "witness" ]; then
        label="witness one"
    fi
    printf '%s' "${label^}"
}

# `node id` opens the home, or creates it with node.key when the volume is
# empty. It reads node.key and not node.json, so it runs first even on a volume
# whose node.json is the pre-proposal-006 file: `write_node_json` replaces that
# file on the next line, and every later command loads the new one.
mabel node id >/dev/null
write_node_json "[]"

# A witness is an identity, not an endpoint (proposal 006 section 1). A witness
# home mints one, publishes a name and the machine that answers for it on that
# identity's own ledger, and names it in `node.json.witness_for`: that admits a
# push whose witness set names the identity (section 4), and the advertisement
# is what section 4.1 requires before this home takes a ledger it does not
# already store. The alias is stable, so a restart reuses the identity on the
# volume rather than minting a second one.
witness_identity=""
if [ "$role" = "witness" ]; then
    witness_alias="${MABEL_WITNESS_ALIAS:-witness}"
    witness_identity="$(identity_id_of "$witness_alias")"
    minted=0
    if [ -z "$witness_identity" ]; then
        mabel identity create --alias "$witness_alias" --kind service >/dev/null
        witness_identity="$(identity_id_of "$witness_alias")"
        if [ -z "$witness_identity" ]; then
            log "the witness identity could not be created"
            exit 1
        fi
        minted=1
        log "minted witness identity $witness_identity as $witness_alias"
    fi
    # An identity that publishes no profile shows in a wallet as a bare id, so
    # this one publishes a name, and it publishes it before the advertisement
    # below so that a freshly minted witness reads like a seeded one: inception,
    # then name, then endpoint.
    #
    # Only on the boot that mints it. A profile replacement is the whole
    # document, so replaying it on every start would append an event clearing
    # any hostname or email an operator published on this identity in the
    # meantime. The name is worth one event at the beginning and is not worth
    # overwriting an operator's later work on every restart. The cost is that a
    # witness whose volume predates this line keeps its bare id until someone
    # runs `mabel profile replace` against it once.
    #
    # `--yes` because nobody is at a terminal to confirm, and `--no-sync`
    # because this is the first boot: the ledger names no witness yet, so there
    # is nobody to ask where its chain ends and nothing for the network step to
    # do.
    if [ "$minted" -eq 1 ]; then
        witness_name="$(witness_display_name "${publish:-$witness_alias}")"
        if named="$(mabel profile replace --identity "$witness_alias" \
            --display-name "$witness_name" --yes --no-sync --json 2>&1)"; then
            log "$witness_alias publishes the name $witness_name"
        else
            log "publishing a name for $witness_alias failed: $named"
            exit 1
        fi
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
    write_node_json "[\"$witness_identity\"]"
    log "witnessing for $witness_identity"
fi

endpoint_id="$(mabel node id)"
log "$role $endpoint_id, http $http_bind, iroh udp $iroh_port, relay $relay"

if [ -n "$publish" ]; then
    address="$(container_ip)"
    ticket="$(mabel node ticket --addr "$address:$iroh_port")"
    write_atomically "$publish.id" "$endpoint_id"
    write_atomically "$publish.ticket" "$ticket"
    # The witness identity goes with the machine that answers for it: a wallet
    # configures `{identity, endpoints}` and cannot invent the identity half
    # from an endpoint id (proposal 006 section 5.4).
    if [ -n "$witness_identity" ]; then
        write_atomically "$publish.identity" "$witness_identity"
    fi
    log "published $publish.ticket for $address:$iroh_port"
fi

# The node-wide witnesses this home configures: one identity and the machines
# that answer for it (proposal 006 section 5.4). A push reads `node.json`, so a
# wallet that only has a ticket would have nothing to dial.
#
# Two sources feed the same list. A waited-for prefix publishes both halves, its
# `.identity` and its `.id`. `MABEL_WITNESSES` names them by hand, which is what
# the DNS overlay uses, and an entry for an identity a prefix already published
# adds its endpoints to that entry rather than starting a second one.
witness_identities=()
witness_machines=()

record_witness() {
    local identity="$1"
    shift
    local index
    for index in "${!witness_identities[@]}"; do
        if [ "${witness_identities[$index]}" = "$identity" ]; then
            witness_machines[index]="${witness_machines[index]} $*"
            return
        fi
    done
    witness_identities+=("$identity")
    witness_machines+=("$*")
}

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
    if [ -f "$prefix.id" ] && [ -f "$prefix.identity" ]; then
        record_witness "$(cat "$prefix.identity")" "$(cat "$prefix.id")"
    elif [ -f "$prefix.id" ]; then
        log "$prefix publishes no .identity, so it cannot be a configured witness"
    fi
done

# `<mabel id>=<endpoint id>[,<endpoint id>...]`, entries separated by spaces. An
# entry with no `=` names an identity and no machine, which `witness set-default`
# refuses unless this home can already reach it, so it is a config error here.
for entry in ${MABEL_WITNESSES:-}; do
    if [ "${entry%%=*}" = "$entry" ]; then
        log "MABEL_WITNESSES entry $entry names no endpoints: use <mabel id>=<endpoint id>"
        exit 1
    fi
    listed="${entry#*=}"
    record_witness "${entry%%=*}" "${listed//,/ }"
done

# `witness set-default` validates every id and rewrites this identity's entry in
# node.json, so a typo fails the container instead of being stored. Each call
# names one identity and the machines that answer for it; other entries are left
# alone, which is how a wallet ends up configured for two witnesses.
for index in "${!witness_identities[@]}"; do
    identity="${witness_identities[$index]}"
    endpoints=""
    for machine in ${witness_machines[$index]}; do
        case ",$endpoints," in
        *",$machine,"*) continue ;;
        esac
        endpoints="${endpoints:+$endpoints,}$machine"
    done
    mabel witness set-default --witness "$identity" --endpoints "$endpoints" >/dev/null
    log "pushing to $identity through ${endpoints//,/ }"
done

log "running mabel $*"
exec mabel "$@"
