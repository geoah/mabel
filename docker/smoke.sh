#!/usr/bin/env bash
# The scripted check of ticket 015, run from the host against the compose
# topology: alice creates an identity, names the witness, pushes, and the
# witness and bob both report the same head.
#
#   docker compose -f docker/compose.yaml up --build -d
#   docker/smoke.sh
#
# Needs curl and jq on the host. Every request goes to 127.0.0.1 on the
# published port, which is also the container port, so the `Host` header curl
# sends is the one the API's loopback rules accept.

set -euo pipefail

witness_port="${MABEL_WITNESS_PORT:-9080}"
alice_port="${MABEL_ALICE_PORT:-9081}"
bob_port="${MABEL_BOB_PORT:-9082}"

step() { printf '\n== %s\n' "$*"; }

get() {
    curl -fsS "http://127.0.0.1:$1$2"
}

post() {
    curl -fsS -X POST \
        -H "Origin: http://127.0.0.1:$1" \
        -H 'Content-Type: application/json' \
        --data "$3" \
        "http://127.0.0.1:$1$2"
}

wait_for_api() {
    local port="$1" waited=0
    until get "$port" /api/node >/dev/null 2>&1; do
        if [ "$waited" -ge 60 ]; then
            echo "no answer from 127.0.0.1:$port/api/node after ${waited}s" >&2
            exit 1
        fi
        sleep 1
        waited="$((waited + 1))"
    done
}

for port in "$witness_port" "$alice_port" "$bob_port"; do
    wait_for_api "$port"
done

step "the three nodes"
for port in "$witness_port" "$alice_port" "$bob_port"; do
    get "$port" /api/node | jq -c '{role, endpoint_id, http_bind, relay}'
done

witness_id="$(get "$witness_port" /api/node | jq -r .endpoint_id)"

step "alice creates an identity"
created="$(post "$alice_port" /api/identities '{"alias":"alice","declared_kind":"person"}')"
echo "$created" | jq -c '{identity_id: .identity.identity_id, head_seq: .identity.head_seq}'
identity_id="$(echo "$created" | jq -r .identity.identity_id)"

step "alice names the witness"
post "$alice_port" "/api/identities/$identity_id/witnesses" \
    "{\"witnesses\":[\"$witness_id\"]}" | jq -c '{head_seq, head_event}'

step "alice pushes to the witness"
post "$alice_port" /api/sync/push \
    "{\"identity_id\":\"$identity_id\",\"to\":null}" |
    jq -c '{head_seq, results}'

step "the witness holds the ledger"
entry="$(get "$witness_port" "/api/ledgers/$identity_id")"
echo "$entry" | jq -c '{ledger_id: .entry.ledger_id, head_seq: .entry.head_seq, event_count: .entry.event_count, witnesses}'

step "bob verifies the ledger through the witness"
report="$(post "$bob_port" /api/verify \
    "{\"kind\":\"ledger\",\"ledger_id\":\"$identity_id\",\"from\":\"$witness_id\"}")"
echo "$report" | jq -c 'del(.events?)'

witness_head="$(echo "$entry" | jq -r .entry.head_seq)"
alice_head="$(get "$alice_port" "/api/identities/$identity_id" | jq -r .identity.head_seq)"
if [ "$witness_head" != "$alice_head" ]; then
    echo "the witness reports head $witness_head, alice holds $alice_head" >&2
    exit 1
fi

printf '\nok: head seq %s on alice and on the witness\n' "$witness_head"
