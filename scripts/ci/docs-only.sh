#!/usr/bin/env bash
# Decide whether a pull request's complete Git diff is documentation-only.
#
# Usage: docs-only.sh BASE_SHA HEAD_SHA
#
# The diff is the PR's merge base to its head, like GitHub's pull-request diff.
# Rename detection is disabled so both sides of a rename are checked. Only
# these paths are eligible:
#   - AGENTS.md, README.md, niri-rune-bar.md at the repository root
#   - Markdown (*.md) anywhere under docs/
#
# The result is printed to stdout as exactly one of:
#   docs_only=true
#   docs_only=false
#
# Any error, unreadable revision, failed diff, or empty diff is failed closed:
# the script exits nonzero for errors, and an empty diff reports false so full
# CI runs. Diagnostics, including changed paths, go to stderr only. Paths are
# never interpolated into the workflow output.

set -euo pipefail

fail() {
    printf 'docs-only classifier: %s\n' "$*" >&2
    exit 1
}

if [ "$#" -ne 2 ]; then
    printf 'usage: %s BASE_SHA HEAD_SHA\n' "$0" >&2
    exit 2
fi

base=$1
head=$2

validate_sha() {
    [ "${#1}" -eq 40 ] || fail "SHA must be 40 lowercase hex characters"
    case "$1" in
        *[!0-9a-f]*) fail "SHA must contain only lowercase hex characters" ;;
    esac
}

validate_sha "$base"
validate_sha "$head"

git cat-file -e "$base^{commit}" 2>/dev/null ||
    fail "base commit is not available in the checkout"
git cat-file -e "$head^{commit}" 2>/dev/null ||
    fail "head commit is not available in the checkout"

tmp=
trap 'if [ -n "$tmp" ]; then rm -f -- "$tmp"; fi' EXIT
tmp=$(mktemp) || fail "could not create a temporary file"

# The three-dot form diffs the merge base to the head, matching the pull
# request rather than including unrelated changes that landed on the base
# branch after this PR branched off.
if ! git diff --no-renames --name-only -z "$base...$head" -- >"$tmp"; then
    fail "git diff failed for the pull-request range"
fi

docs_only=true
changed=false
while IFS= read -r -d '' path; do
    changed=true
    case "$path" in
        AGENTS.md | README.md | niri-rune-bar.md)
            ;;
        docs/*.md)
            ;;
        *)
            # %q escapes control characters so a crafted path cannot inject a
            # workflow command or a new log line.
            printf 'docs-only classifier: non-documentation change: %q\n' "$path" >&2
            docs_only=false
            ;;
    esac
done <"$tmp"

if [ "$changed" = false ]; then
    printf 'docs-only classifier: no changed paths; requiring full CI\n' >&2
    docs_only=false
fi

printf 'docs_only=%s\n' "$docs_only"
