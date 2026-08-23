#!/usr/bin/env bash
# The whole mabel story over the compose topology of docker/compose.yaml, run
# through the CLI inside the containers (ticket 017, proposal 001 section 11).
#
#   demo/run-demo.sh          # up, the story, down -v
#   demo/run-demo.sh --keep   # leaves the topology and the homes running
#
# Needs docker, curl and jq on the host, and nothing else: every node.json
# sets relay "disabled" and the witness address travels as an EndpointTicket
# on a shared volume, so no step reaches the internet (docker/README.md).
#
# Each wallet's home lives in its own container volume, so every command runs
# with `docker compose exec` and the three membership artifacts travel between
# alice and bob as files, by `docker cp` through a directory on the host. That
# is the point of the artifacts: two homes that share no disk still admit one
# member with two signatures.

set -euo pipefail

keep=0
for argument in "$@"; do
    case "$argument" in
    --keep) keep=1 ;;
    -h | --help)
        sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    *)
        echo "run-demo.sh: unknown argument $argument (try --keep or --help)" >&2
        exit 2
        ;;
    esac
done

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
compose_file="$root/docker/compose.yaml"
started_at="$(date +%s)"
work=""
verifier_runs=0

fail() {
    printf '\nrun-demo.sh failed: %s\n' "$*" >&2
    printf 'the topology is still up; `docker compose -f %s logs` says what the nodes did,\n' \
        "$compose_file" >&2
    printf 'and `docker compose -f %s down -v` clears it.\n' "$compose_file" >&2
    keep=1
    exit 1
}

cleanup() {
    local status="$?"
    [ -n "$work" ] && rm -rf "$work"
    if [ "$keep" -eq 0 ]; then
        printf '\n== tearing the topology down (--keep leaves it up)\n'
        docker compose -f "$compose_file" down -v >/dev/null 2>&1 || true
    fi
    exit "$status"
}
trap cleanup EXIT

for tool in docker curl jq; do
    command -v "$tool" >/dev/null 2>&1 || fail "$tool is not on PATH"
done

just_titled=0

phase() {
    printf '\n\n=== %s\n' "$*"
    just_titled=1
}

blank() {
    printf '\n'
    just_titled=0
}

note() {
    if [ "$just_titled" -eq 1 ]; then blank; fi
    printf '  %s\n' "$*"
}

emit() {
    printf '%s\n' "$RUN_OUT" | sed 's/^/      /'
}

dc() {
    docker compose -f "$compose_file" "$@"
}

# `mabel <args>` in one container. Prints the command, the output, and leaves
# both in RUN_OUT and RUN_STATUS.
run() {
    local service="$1"
    shift
    just_titled=0
    printf '\n  [%s] $ mabel %s\n' "$service" "$*"
    RUN_STATUS=0
    RUN_OUT="$(dc exec -T "$service" mabel "$@" 2>&1)" || RUN_STATUS=$?
    emit
    [ "$RUN_STATUS" -eq 0 ] || fail "mabel $* exited $RUN_STATUS in $service"
}

# `mabel sync push <args> --peer <the witness ticket this container holds>`.
# The ticket is an address hint and never authorization (section 4), which is
# why it can sit on a world-readable volume. A rejected push is the caller's
# to read: phase 8 expects one.
push() {
    local service="$1" arguments=""
    shift
    just_titled=0
    for argument in "$@"; do arguments+=" $argument"; done
    printf '\n  [%s] $ mabel sync push%s --peer "$(cat /shared/witness.ticket)"\n' \
        "$service" "$arguments"
    RUN_STATUS=0
    RUN_OUT="$(dc exec -T "$service" sh -c \
        "mabel sync push$arguments --peer \"\$(cat /shared/witness.ticket)\"" 2>&1)" ||
        RUN_STATUS=$?
    emit
}

# One throwaway container on the compose network with an empty home: no
# identities, no keys but the node key it makes on the spot, and the witness
# ticket as its only address hint.
verify_from_a_fresh_home() {
    just_titled=0
    verifier_runs="$((verifier_runs + 1))"
    printf '\n  [a fresh home, container %s] $ mabel %s\n' "$verifier_runs" "$*"
    RUN_STATUS=0
    RUN_OUT="$(docker run --rm --network "$network" \
        --volume "$ticket_volume:/shared:ro" \
        --env MABEL_WAIT_FOR_TICKET=/shared/witness \
        --name "mabel-demo-verifier-$verifier_runs" \
        "$image" "$@" 2>&1 | sed '/^entrypoint: running mabel /d')" || RUN_STATUS=$?
    emit
    [ "$RUN_STATUS" -eq 0 ] || fail "the fresh verifier exited $RUN_STATUS"
}

# Carries one file from one container's /tmp to another's, through the host.
hand_over() {
    just_titled=0
    local from="$1" to="$2" name="$3"
    printf '\n  $ docker cp mabel-%s:/tmp/%s - | docker cp - mabel-%s:/tmp/%s\n' \
        "$from" "$name" "$to" "$name"
    docker cp "mabel-$from:/tmp/$name" "$work/$name" >/dev/null ||
        fail "could not copy /tmp/$name out of mabel-$from"
    docker cp "$work/$name" "mabel-$to:/tmp/$name" >/dev/null ||
        fail "could not copy $name into mabel-$to"
    printf '      %s bytes now in both homes\n' "$(wc -c <"$work/$name" | tr -d ' ')"
}

work="$(mktemp -d)"

phase "1. one witness and two wallets, on one bridge network"
note 'a witness stores and serves ledgers and signs nothing of its own'
note '(decision 001, passive witnesses): it cannot admit, attest or revoke.'
printf '\n  $ docker compose -f docker/compose.yaml down -v && up -d --wait\n'
dc down -v >/dev/null 2>&1 || true
dc up -d --wait >/dev/null 2>&1 || fail "the topology did not come up healthy"
dc ps --format '{{.Name}}  {{.Service}}  {{.Status}}' | sed 's/^/      /' ||
    fail "docker compose ps reported nothing"

image="$(docker inspect -f '{{.Config.Image}}' mabel-witness)"
network="$(docker inspect \
    -f '{{range $name, $_ := .NetworkSettings.Networks}}{{$name}}{{end}}' mabel-witness)"
ticket_volume="$(docker inspect \
    -f '{{range .Mounts}}{{if eq .Destination "/shared"}}{{.Name}}{{end}}{{end}}' mabel-witness)"
witness_id="$(dc exec -T witness cat /shared/witness.id)"
[ -n "$witness_id" ] || fail "the witness published no endpoint id to /shared/witness.id"
blank
note "witness endpoint $witness_id"

phase "2. alice and bob create person identities"
note 'an identity is the digest of its own inception event, so the id and the'
note 'first key are one fact (proposal 001 section 3.3). The alias is local.'
run alice identity create --alias alice --kind person
alice_id="$(printf '%s' "$RUN_OUT" | sed -n 's/^created identity //p')"
run bob identity create --alias bob --kind person
bob_id="$(printf '%s' "$RUN_OUT" | sed -n 's/^created identity //p')"
[ -n "$alice_id" ] && [ -n "$bob_id" ] || fail "could not read the new identity ids"

phase "3. both name the witness in their ledger and push"
note 'naming a witness is an event in the ledger, so who was asked to hold a'
note 'copy is part of the record a verifier reads.'
run alice witness add --identity alice --endpoint "$witness_id"
run bob witness add --identity bob --endpoint "$witness_id"
push alice --identity alice
[ "$RUN_STATUS" -eq 0 ] || fail "alice could not push to the witness"
push bob --identity bob
[ "$RUN_STATUS" -eq 0 ] || fail "bob could not push to the witness"

phase "4. bob exports the descriptor an invitation embeds"
note "the descriptor carries bob's inception byte for byte, which is what"
note "proves his id and his key belong together (proposal 002 section 8)."
run bob identity export bob --out /tmp/bob.descriptor
hand_over bob alice bob.descriptor

phase "5. alice founds a shared ledger"
note 'an organization is a ledger with an identity root: it holds no key of'
note 'its own and its controllers sign for it (decision 002). One ledger type'
note 'covers a person and an organization (unified ledgers, decision 003).'
run alice identity create --alias mabel-demo-co --kind organization --founder alice
org_id="$(printf '%s' "$RUN_OUT" | sed -n 's/^created identity //p')"
[ -n "$org_id" ] || fail "could not read the shared ledger id"

phase "6. alice invites bob, bob accepts, alice admits"
note 'three commands, three files, two signatures. Nobody is added to a'
note 'ledger without their own signature (membership by invitation,'
note 'decision 004), and the two homes share no disk.'
run alice membership invite --ledger mabel-demo-co --by alice \
    --invitee /tmp/bob.descriptor --role controller --out /tmp/invitation.bundle
hand_over alice bob invitation.bundle
blank
note 'bob sees what accepting admits to before his key is used: the surface'
note 'below is the fold of the bundle, not a claim the file makes.'
run bob membership accept /tmp/invitation.bundle --as bob \
    --out /tmp/acceptance.file --yes
hand_over bob alice acceptance.file
run alice membership admit --ledger mabel-demo-co --by alice /tmp/acceptance.file
run alice membership list --ledger mabel-demo-co

phase "7. alice and the shared ledger both attest trust in bob"
note 'trust is one-way and it is a statement, not a permission (decision'
note '005): an attestation names a subject and grants nothing.'
run alice trust add --issuer alice --subject "$bob_id"
run alice trust add --issuer mabel-demo-co --subject "$bob_id"
blank
note "the shared ledger holds no key, so alice's key signed for it. The"
note "report names the principal that signed, so a delegate's signature is"
note "not read as the ledger's own (proposal 002 section 5)."
run alice verify trust --issuer mabel-demo-co --subject "$bob_id"

phase "8. push what the witness will take"
push alice --identity alice
[ "$RUN_STATUS" -eq 0 ] || fail "alice could not push her attestation"
blank
note "now the shared ledger, to the same witness by endpoint id. The reply is"
note "shown as JSON because the text line does not name the rejection."
push alice --identity mabel-demo-co --to "$witness_id" --json
org_pushed="$RUN_STATUS"
if [ "$org_pushed" -eq 0 ]; then
    blank
    note "the witness took the identity-rooted ledger."
else
    blank
    note "KNOWN GAP: the witness rejects every identity-rooted ledger as"
    note "MALFORMED, \"a message nests more than 8 levels deep\". A pushed"
    note "event is scanned inside Frame -> PushReq -> SignedEvent, two levels"
    note "below where the same bytes are scanned locally, so the founder"
    note "inception an identity root embeds lands at depth 9 against the cap"
    note "of 8 in mabel_core::validate::MAX_NESTING. Locally it reaches depth"
    note "7, which is why \`identity create --founder\` succeeds and the push"
    note "does not. The shared ledger stays local for the rest of this demo,"
    note "and the verification below reads alice's own ledger, which pushes."
fi

phase "9. a stranger verifies alice-trusts-bob from an empty home"
note "a fresh container: no identities, no aliases, no keys but the node key"
note "it makes on the spot, and one address hint. It reads the witness's"
note "copy. The report names its source and how far it read, and claims"
note "nothing about a chain it did not read (flag R, proposal 001 section 6)."
verify_from_a_fresh_home verify trust --issuer "$alice_id" --subject "$bob_id" \
    --from "$witness_id"
printf '%s\n' "$RUN_OUT" | grep -q '^trusted: true' ||
    fail "the fresh home did not report trusted: true"
printf '%s\n' "$RUN_OUT" | grep -q '^signed by principal' ||
    fail "the report named no signing principal"

phase "10. alice revokes, and the stranger reads the revocation"
run alice trust list --issuer alice
attestation="$(printf '%s' "$RUN_OUT" | awk 'NR == 1 { print $1 }')"
[ -n "$attestation" ] || fail "alice has issued no attestation to revoke"
run alice trust revoke --issuer alice --attestation "$attestation"
push alice --identity alice
[ "$RUN_STATUS" -eq 0 ] || fail "alice could not push her revocation"
verify_from_a_fresh_home verify trust --issuer "$alice_id" --subject "$bob_id" \
    --from "$witness_id"
printf '%s\n' "$RUN_OUT" | grep -q '^trusted: false' ||
    fail "the fresh home still reports trust after the revocation"
printf '%s\n' "$RUN_OUT" | grep -q 'revoked at seq' ||
    fail "the report does not name the revocation"

phase "11. what the witness holds"
note "the witness's read-only debug API, on the host at 127.0.0.1:9080. Host"
note "port equals container port because the API refuses any Host that is"
note "not 127.0.0.1 or localhost on the port it bound (section 10)."
printf '\n  $ curl -fsS http://127.0.0.1:9080/api/ledgers\n'
ledgers="$(curl -fsS http://127.0.0.1:9080/api/ledgers)" ||
    fail "the witness debug API did not answer on 127.0.0.1:9080"
printf '%s' "$ledgers" |
    jq -r '.entries[] | "      \(.ledger_id)  \(.declared_kind)  head seq \(.head_seq) at \(.head_event)  \(.event_count) events"' ||
    fail "the witness ledger list did not parse"
count="$(printf '%s' "$ledgers" | jq '.entries | length')"
[ "$count" -ge 2 ] || fail "the witness holds $count ledgers, expected at least 2"

printf '\n\n=== the demo ran green in %s seconds\n' "$(($(date +%s) - started_at))"
printf '  alice        %s\n' "$alice_id"
printf '  bob          %s\n' "$bob_id"
if [ "$org_pushed" -eq 0 ]; then
    printf '  shared       %s\n' "$org_id"
else
    printf '  shared       %s (local only, see phase 8)\n' "$org_id"
fi
printf '  witness      %s, holding %s ledgers\n' "$witness_id" "$count"
if [ "$keep" -eq 1 ]; then
    printf '\n  --keep: the topology is still up. The UI is on http://127.0.0.1:9081\n'
    printf '  for alice and http://127.0.0.1:9080 for the witness.\n'
    printf '  docker compose -f docker/compose.yaml down -v clears it.\n'
fi
