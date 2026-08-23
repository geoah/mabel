#!/usr/bin/env bash
# `GET /api/node` over loopback inside the container, with the `Host` header
# the API's loopback rules require (proposal 001 section 10).
#
# Usage: mabel-healthcheck [port], default $MABEL_HTTP_PORT or 9080.
#
# bash's /dev/tcp is used rather than curl so the runtime image installs no
# packages. A 403 from the Host rule fails the check, which is the point: the
# container port and the published host port have to match for a request from
# the host to be accepted, so the check exercises the same rule.

set -euo pipefail

port="${1:-${MABEL_HTTP_PORT:-9080}}"

exec 3<>"/dev/tcp/127.0.0.1/$port"
printf 'GET /api/node HTTP/1.1\r\nHost: 127.0.0.1:%s\r\nConnection: close\r\n\r\n' \
    "$port" >&3

IFS= read -r status <&3
case "$status" in
"HTTP/1.1 200"*) ;;
*)
    printf 'healthcheck: %s\n' "$status" >&2
    exit 1
    ;;
esac

if ! grep -q '"ok":true' <&3; then
    printf 'healthcheck: /api/node did not answer ok\n' >&2
    exit 1
fi
