#!/usr/bin/env bash
#
# Prints the change list for a release tag as markdown: every commit made since
# the previous release tag, grouped by its conventional-commit type.
#
#   scripts/release-notes.sh v0.2.0
#
# The release commit itself is left out, and so are merge commits. A commit
# whose subject is not a conventional commit lands under "Other", so nothing
# disappears.

set -euo pipefail

tag=${1:?usage: release-notes.sh <tag>}

# The release tag one step below this one in version order. Release tags are
# exactly v<x>.<y>.<z>, so the retired v<version>-build.<run number> tags are
# skipped.
previous=$(git tag --list 'v*' --sort=v:refname |
  grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' |
  awk -v tag="$tag" '$0 == tag { exit } { earlier = $0 } END { print earlier }')

if [ -n "$previous" ]; then
  range="$previous..$tag"
  echo "Changes since \`$previous\`:"
else
  range="$tag"
  echo 'Changes:'
fi
echo

# The subject is untrusted text. It reaches awk as data and markdown as data,
# and is never a shell word.
git log --no-merges --reverse --format='%h%x09%s' "$range" | awk '
  BEGIN {
    order = "feat fix perf refactor docs test build ci chore revert style other"
    total = split(order, keys, " ")
    heading["feat"]     = "Features"
    heading["fix"]      = "Fixes"
    heading["perf"]     = "Performance"
    heading["refactor"] = "Refactors"
    heading["docs"]     = "Docs"
    heading["test"]     = "Tests"
    heading["build"]    = "Build"
    heading["ci"]       = "CI"
    heading["chore"]    = "Chores"
    heading["revert"]   = "Reverts"
    heading["style"]    = "Style"
    heading["other"]    = "Other"
    for (i = 1; i <= total; i++) { known[keys[i]] = 1 }
  }
  {
    cut = index($0, "\t")
    sha = substr($0, 1, cut - 1)
    subject = substr($0, cut + 1)

    # The release commit is the tag, not a change in it.
    if (subject ~ /^chore\(release\)/) { next }

    type = subject
    if (match(type, /^[a-z]+(\([^)]*\))?!?:/)) {
      sub(/[(!:].*$/, "", type)
    } else {
      type = "other"
    }
    if (!(type in known)) { type = "other" }

    items[type] = items[type] sprintf("- %s (`%s`)\n", subject, sha)
    listed++
  }
  END {
    for (i = 1; i <= total; i++) {
      if (items[keys[i]] != "") {
        printf "### %s\n\n%s\n", heading[keys[i]], items[keys[i]]
      }
    }
    if (!listed) { print "No commits." }
  }
'
