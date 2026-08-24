#!/usr/bin/env bash
#
# The release version lives in six manifests and two cargo lockfiles. This
# script reads the next version out of the git history, writes one version into
# all of them, or checks that all of them already agree.
#
#   scripts/version.sh next          prints the next version, x.y.z, on stdout
#   scripts/version.sh set 1.2.3     writes 1.2.3 into every file that carries it
#   scripts/version.sh check 1.2.3   fails, naming every file that disagrees
#
# `next` bumps the last release tag by the conventional commits made since it:
# a `!` after the type in any subject, or a `BREAKING CHANGE:` footer in any
# body, bumps major; a `feat` subject bumps minor; anything else bumps patch.
# There is no "nothing to release" answer, so every push to main ships a
# version.

set -euo pipefail

root=$(git rev-parse --show-toplevel)
cd "$root"

# "<path>:<table>". The version is the first `version =` key inside the table.
toml_manifests=(
  'Cargo.toml:workspace.package'
  'app/src-tauri/Cargo.toml:package'
)

# Top level `.version`, plus `.packages[""].version` when the file is an npm
# lockfile. `npm ci` compares the two, so both have to move.
json_manifests=(
  'app/src-tauri/tauri.conf.json'
  'ui/package.json'
  'ui/package-lock.json'
  'tests/e2e/package.json'
  'tests/e2e/package-lock.json'
)

# `cargo build --locked` compares the lockfile with the manifests it just read,
# so the path packages in both lockfiles have to carry the new version too.
cargo_locks=(
  'Cargo.lock'
  'app/src-tauri/Cargo.lock'
)

require_semver() {
  if ! [[ $1 =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "not an x.y.z version: $1" >&2
    exit 1
  fi
}

# The version key of one toml table, or empty when the table has none.
toml_get() {
  awk -v table="$2" '
    $0 ~ "^\\[" table "\\]" { inside = 1; next }
    /^\[/                   { inside = 0 }
    inside && /^version[[:space:]]*=/ {
      gsub(/^version[[:space:]]*=[[:space:]]*"|"[[:space:]]*$/, "")
      print
      exit
    }
  ' "$1"
}

toml_set() {
  local file=$1 table=$2 version=$3 rewritten
  # awk exits 1 when the table has no version key, which set -e turns into a
  # failed assignment, so a renamed table cannot pass silently.
  if ! rewritten=$(awk -v table="$table" -v version="$version" '
    $0 ~ "^\\[" table "\\]" { inside = 1; print; next }
    /^\[/                   { inside = 0 }
    inside && !done && /^version[[:space:]]*=/ {
      print "version = \"" version "\""
      done = 1
      next
    }
    { print }
    END { if (!done) exit 1 }
  ' "$file"); then
    echo "no version key in [$table] of $file" >&2
    exit 1
  fi
  # Rewriting in place, rather than moving a temp file over it, keeps the
  # file's mode.
  printf '%s\n' "$rewritten" >"$file"
}

# "<key> <version>" for every version an npm or tauri json file carries.
json_versions() {
  jq -r '
    [{ key: ".version", value: .version }]
    + (if has("packages") and (.packages | has("")) then
        [{ key: ".packages[\"\"].version", value: .packages[""].version }]
      else [] end)
    | .[] | "\(.key) \(.value)"
  ' "$1"
}

json_set() {
  local file=$1 version=$2 rewritten
  rewritten=$(jq --arg v "$version" '
    .version = $v
    | if has("packages") and (.packages | has("")) then
        .packages[""].version = $v
      else . end
  ' "$file")
  printf '%s\n' "$rewritten" >"$file"
}

# "<name> <version>" for every path package in a cargo lockfile. A package with
# no `source` key is a path package, which is to say one of ours.
lock_versions() {
  awk '
    /^\[\[package\]\]/ {
      if (name != "" && !remote) { print name " " version }
      name = ""; version = ""; remote = 0
      next
    }
    /^name = /    { gsub(/^name = "|"$/, "");    name = $0; next }
    /^version = / { gsub(/^version = "|"$/, ""); version = $0; next }
    /^source = /  { remote = 1; next }
    END { if (name != "" && !remote) { print name " " version } }
  ' "$1"
}

# The newest tag that is a release. Release tags are exactly v<x>.<y>.<z>: the
# retired v<version>-build.<run number> prerelease tags are not releases and
# must never be picked as the baseline.
last_release_tag() {
  local tag
  tag=$(git tag --list 'v*' --sort=v:refname |
    grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | tail -n 1 || true)
  # v0.1.0 is the first release this repository cut, so it is the baseline when
  # no release tag is in reach, as in a clone fetched without tags.
  printf '%s\n' "${tag:-v0.1.0}"
}

cmd_next() {
  local last range subjects bodies count bump major minor patch version

  last=$(last_release_tag)
  if git rev-parse -q --verify "refs/tags/$last^{commit}" >/dev/null; then
    range="$last..HEAD"
  else
    range=HEAD
  fi

  # Merge commit subjects are not conventional commits and the commits they
  # merge are already in the range.
  subjects=$(git log --no-merges --format='%s' "$range")
  bodies=$(git log --no-merges --format='%b' "$range")
  count=$(git rev-list --no-merges --count "$range")

  bump='patch'
  if printf '%s\n' "$subjects" | grep -qE '^[a-z]+(\([^)]*\))?!:' ||
    printf '%s\n' "$bodies" | grep -qE '^BREAKING[ -]CHANGE[[:space:]]*:'; then
    bump='major'
  elif printf '%s\n' "$subjects" | grep -qE '^feat(\([^)]*\))?:'; then
    bump='minor'
  fi

  IFS=. read -r major minor patch <<<"${last#v}"
  case $bump in
  major)
    major=$((major + 1))
    minor=0
    patch=0
    ;;
  minor)
    minor=$((minor + 1))
    patch=0
    ;;
  patch) patch=$((patch + 1)) ;;
  esac
  version="$major.$minor.$patch"

  echo "last release $last, $count commits since, $bump bump" >&2
  printf '%s\n' "$version"
}

cmd_set() {
  local version=$1 entry file table
  require_semver "$version"

  for entry in "${toml_manifests[@]}"; do
    file=${entry%%:*}
    table=${entry#*:}
    toml_set "$file" "$table" "$version"
    echo "wrote $version to $file [$table]"
  done

  for file in "${json_manifests[@]}"; do
    json_set "$file" "$version"
    echo "wrote $version to $file"
  done

  # The lockfiles are cargo's to write. `--workspace` updates the path packages
  # and nothing else, so no dependency moves behind the release. `--offline` is
  # deliberately absent: a fresh checkout has no registry index, and an offline
  # resolve without one fails. app/src-tauri comes second because its path
  # crates read their version from the root manifest.
  cargo update --quiet --workspace
  cargo update --quiet --workspace --manifest-path app/src-tauri/Cargo.toml
  echo "refreshed ${cargo_locks[*]}"

  cmd_check "$version"
}

cmd_check() {
  local version=$1 entry file table key found name bad=0
  require_semver "$version"

  for entry in "${toml_manifests[@]}"; do
    file=${entry%%:*}
    table=${entry#*:}
    found=$(toml_get "$file" "$table")
    if [ "$found" != "$version" ]; then
      echo "$file [$table] version says ${found:-nothing}, not $version" >&2
      bad=1
    fi
  done

  for file in "${json_manifests[@]}"; do
    while read -r key found; do
      if [ "$found" != "$version" ]; then
        echo "$file $key says ${found:-nothing}, not $version" >&2
        bad=1
      fi
    done < <(json_versions "$file")
  done

  for file in "${cargo_locks[@]}"; do
    while read -r name found; do
      if [ "$found" != "$version" ]; then
        echo "$file says $name is ${found:-nothing}, not $version" >&2
        bad=1
      fi
    done < <(lock_versions "$file")
  done

  if [ "$bad" -ne 0 ]; then
    echo "run scripts/version.sh set $version to make them agree" >&2
    exit 1
  fi
  echo "every manifest and lockfile says $version"
}

case ${1:-} in
next) cmd_next ;;
set) cmd_set "${2:?usage: version.sh set <x.y.z>}" ;;
check) cmd_check "${2:?usage: version.sh check <x.y.z>}" ;;
*)
  echo "usage: version.sh next | set <x.y.z> | check <x.y.z>" >&2
  exit 2
  ;;
esac
