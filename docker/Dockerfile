# syntax=docker/dockerfile:1

# One image for every node (proposal 001 section 11, ticket 015). One command
# serves them all: `mabel serve`, over whatever the home holds.
#
# Build from the repository root:
#   docker build -f docker/Dockerfile -t mabel:dev .

# ---------------------------------------------------------------------------
# Stage 1: the UI bundle, so `ui/dist` exists when rust-embed compiles it in.
# ---------------------------------------------------------------------------
FROM node:22-bookworm AS ui

WORKDIR /src/ui
# The `playwright` dev dependency downloads browsers on install; the bundle
# does not need them.
ENV PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 \
    CI=1

# Manifests first, so a source edit does not re-run `npm ci`.
COPY ui/package.json ui/package-lock.json ./
RUN npm ci

# `vite.config.ts` aliases `@contracts` to the repository's contracts
# directory, so the UI build needs it.
COPY contracts /src/contracts
COPY ui /src/ui
RUN npm run build && test -f /src/ui/dist/index.html

# ---------------------------------------------------------------------------
# Stage 2: the release binary, with ui/dist in place before cargo runs.
# ---------------------------------------------------------------------------
FROM rust:1.98-bookworm AS build

WORKDIR /src

# Manifests and the lockfile first, then a dependency-only build against stub
# sources, so editing a crate does not rebuild every dependency. `protoc` comes
# from protoc-bin-vendored, so no system package is needed.
COPY Cargo.toml Cargo.lock ./
COPY crates/mabel-proto/Cargo.toml crates/mabel-proto/
COPY crates/mabel-core/Cargo.toml crates/mabel-core/
COPY crates/mabel-net/Cargo.toml crates/mabel-net/
COPY crates/mabel-node/Cargo.toml crates/mabel-node/
COPY crates/mabel-cli/Cargo.toml crates/mabel-cli/
RUN set -eux; \
    for crate in mabel-proto mabel-core mabel-net mabel-node; do \
        mkdir -p "crates/$crate/src"; \
        : > "crates/$crate/src/lib.rs"; \
    done; \
    mkdir -p crates/mabel-cli/src; \
    echo 'fn main() {}' > crates/mabel-cli/src/main.rs; \
    echo 'fn main() {}' > crates/mabel-proto/build.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/src/target,sharing=locked \
    cargo build --release --locked --package mabel-cli

# The sources. `contracts/` is compiled in with include_str! and `proto/` is
# what mabel-proto's build script reads.
COPY proto proto
COPY contracts contracts
COPY crates crates
COPY --from=ui /src/ui/dist ui/dist

# `touch` first: COPY keeps the context's mtimes, which can be older than the
# stub build above, and cargo decides freshness by mtime. `cp` runs inside the
# same layer, because a cache mount holds no image content.
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/src/target,sharing=locked \
    set -eux; \
    find crates proto contracts ui -type f -exec touch {} +; \
    cargo build --release --locked --package mabel-cli; \
    mkdir -p /out; \
    cp target/release/mabel /out/mabel; \
    /out/mabel --version

# ---------------------------------------------------------------------------
# Stage 3: the runtime. No compiler, no npm, no package installs: the scripts
# use bash, coreutils and getent, which the base image already has.
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# The node home and the shared ticket directory are created here and owned by
# the runtime user, so an empty named volume mounted over either one inherits
# that ownership instead of root's.
RUN set -eux; \
    groupadd --system --gid 10001 mabel; \
    useradd --system --uid 10001 --gid mabel --home-dir /home/mabel --create-home mabel; \
    mkdir -p /data /shared; \
    chown mabel:mabel /data /shared; \
    chmod 700 /data

COPY --from=build /out/mabel /usr/local/bin/mabel
COPY docker/entrypoint.sh /usr/local/bin/mabel-entrypoint
COPY docker/healthcheck.sh /usr/local/bin/mabel-healthcheck
RUN chmod 0755 /usr/local/bin/mabel-entrypoint /usr/local/bin/mabel-healthcheck

# The home lives on a volume; keys and ledgers survive a recreate.
ENV MABEL_HOME=/data
VOLUME ["/data"]

# TCP for the HTTP API and the UI, UDP for the Iroh endpoint. Compose fixes
# both per service; these are the defaults of `node.json` and of the entrypoint.
EXPOSE 9080/tcp
EXPOSE 9070/udp

USER mabel
WORKDIR /data

# The entrypoint prepares the home and then execs `mabel <command>`. What the
# node can do is read from the home the entrypoint prepared, not from the
# command (proposal 006 section 8).
ENTRYPOINT ["/usr/local/bin/mabel-entrypoint"]
CMD ["serve", "--iroh-port", "9070"]
