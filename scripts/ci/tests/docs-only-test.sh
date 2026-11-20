#!/usr/bin/env bash
# Focused tests for scripts/ci/docs-only.sh.
#
# These tests create disposable Git repositories and exercise path
# classification only. They do not build Snowbar, need Nix, or touch a remote.

set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
classifier="$script_dir/../docs-only.sh"

work=$(mktemp -d "${TMPDIR:-/tmp}/snowbar-docs-only-test.XXXXXX")
trap 'rm -rf -- "$work"' EXIT

repo=
base=
head=
passes=0
failures=0

record_pass() {
    passes=$((passes + 1))
}

record_fail() {
    printf 'FAIL: %s\n' "$*" >&2
    failures=$((failures + 1))
}

commit() {
    git -C "$repo" add -A
    git -C "$repo" commit -q -m "$1"
    git -C "$repo" rev-parse HEAD
}

new_repo() {
    repo=$(mktemp -d "$work/repo.XXXXXX")
    git -C "$repo" init -q
    git -C "$repo" config user.email test@example.invalid
    git -C "$repo" config user.name "docs-only test"
    git -C "$repo" config commit.gpgsign false
    printf 'baseline\n' >"$repo/Cargo.toml"
    base=$(commit "baseline")
}

expect() {
    expected=$1
    shift
    actual=
    if actual=$(cd "$repo" && "$classifier" "$@"); then
        if [ "$actual" = "docs_only=$expected" ]; then
            record_pass
        else
            record_fail "expected docs_only=$expected, got '$actual' for: $*"
        fi
    else
        record_fail "classifier exited nonzero for: $*"
    fi
}

expect_failure() {
    if (cd "$repo" && "$classifier" "$@") >/dev/null 2>&1; then
        record_fail "classifier unexpectedly succeeded for: $*"
    else
        record_pass
    fi
}

# Exact root files are allowed.
new_repo
printf 'doc\n' >"$repo/AGENTS.md"
printf 'doc\n' >"$repo/README.md"
printf 'doc\n' >"$repo/niri-rune-bar.md"
head=$(commit "root documentation")
expect true "$base" "$head"

# Markdown under docs/, including nested paths, is allowed.
new_repo
mkdir -p "$repo/docs/nested"
printf 'doc\n' >"$repo/docs/README.md"
printf 'doc\n' >"$repo/docs/nested/deep.md"
head=$(commit "nested documentation")
expect true "$base" "$head"

# A non-Markdown file under docs/ is not documentation-only.
new_repo
mkdir -p "$repo/docs"
printf 'png\n' >"$repo/docs/image.png"
head=$(commit "docs asset")
expect false "$base" "$head"

# A Markdown-looking root file outside the allowlist is not allowed.
new_repo
printf 'doc\n' >"$repo/docs.md"
head=$(commit "root markdown outside allowlist")
expect false "$base" "$head"

# Only .md is accepted as Markdown under docs/.
new_repo
mkdir -p "$repo/docs"
printf 'doc\n' >"$repo/docs/note.markdown"
head=$(commit "markdown extension")
expect false "$base" "$head"

# A root file outside the allowlist is not documentation-only.
new_repo
printf 'license\n' >"$repo/LICENSE"
head=$(commit "license")
expect false "$base" "$head"

# A workflow change is never documentation-only.
new_repo
mkdir -p "$repo/.github/workflows"
printf 'name: CI\n' >"$repo/.github/workflows/ci.yml"
head=$(commit "workflow")
expect false "$base" "$head"

# Mixed documentation and source changes require full CI.
new_repo
mkdir -p "$repo/docs" "$repo/src"
printf 'doc\n' >"$repo/docs/note.md"
printf 'code\n' >"$repo/src/main.rs"
head=$(commit "mixed")
expect false "$base" "$head"

# Deletions of allowed documentation are allowed.
new_repo
mkdir -p "$repo/docs"
printf 'doc\n' >"$repo/docs/old.md"
base=$(commit "add old doc")
git -C "$repo" rm -q docs/old.md
head=$(commit "delete old doc")
expect true "$base" "$head"

# Renaming between two allowed documentation paths is allowed.
new_repo
mkdir -p "$repo/docs"
printf 'doc\n' >"$repo/docs/old.md"
base=$(commit "add doc to rename")
git -C "$repo" mv docs/old.md docs/new.md
head=$(commit "rename doc")
expect true "$base" "$head"

# Renaming source into docs/ must still see the source side.
new_repo
mkdir -p "$repo/src" "$repo/docs"
printf 'code\n' >"$repo/src/lib.rs"
base=$(commit "add source to rename")
git -C "$repo" mv src/lib.rs docs/lib.md
head=$(commit "rename source into docs")
expect false "$base" "$head"

# Renaming documentation into source must see the documentation side too.
new_repo
mkdir -p "$repo/src" "$repo/docs"
printf 'doc\n' >"$repo/docs/note.md"
base=$(commit "add doc to rename out")
git -C "$repo" mv docs/note.md src/note.rs
head=$(commit "rename doc into source")
expect false "$base" "$head"

# Spaces in an allowed path are handled.
new_repo
mkdir -p "$repo/docs"
printf 'doc\n' >"$repo/docs/space name.md"
head=$(commit "space in doc name")
expect true "$base" "$head"

# A newline in an allowed path is one path, not two.
new_repo
mkdir -p "$repo/docs"
newline_doc=$'docs/line\nbreak.md'
printf 'doc\n' >"$repo/$newline_doc"
head=$(commit "newline in doc name")
expect true "$base" "$head"

# A newline in a non-documentation path is still rejected, and its diagnostic
# must not become a second workflow command line.
new_repo
mkdir -p "$repo/src"
newline_source=$'src/bad\n::error::injected.rs'
printf 'code\n' >"$repo/$newline_source"
head=$(commit "newline in source name")
expect false "$base" "$head"

# Shell metacharacters in a rejected path are printed, not executed.
new_repo
mkdir -p "$repo/src"
weapon='src/$(touch should-not-exist).rs'
printf 'code\n' >"$repo/$weapon"
head=$(commit "metacharacters in source name")
expect false "$base" "$head"
if [ ! -e "$repo/should-not-exist" ]; then
    record_pass
else
    record_fail "classifier executed a path from the diff"
fi

# The diff uses the merge base, so unrelated base-branch code changes do not
# make a docs-only PR ineligible.
new_repo
main_branch=$(git -C "$repo" symbolic-ref --short HEAD)
git -C "$repo" checkout -q -b feature
mkdir -p "$repo/docs"
printf 'doc\n' >"$repo/docs/note.md"
head=$(commit "docs-only feature")
git -C "$repo" checkout -q "$main_branch"
mkdir -p "$repo/src"
printf 'code\n' >"$repo/src/main.rs"
advanced_base=$(commit "unrelated base change")
expect true "$advanced_base" "$head"

# An empty diff is not documentation-only; fail closed to full CI.
new_repo
expect false "$base" "$base"

# Invalid or unavailable revisions fail instead of reporting docs_only.
new_repo
head=$(git -C "$repo" rev-parse HEAD)
expect_failure not-a-sha "$head"
expect_failure AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA "$head"
expect_failure 0000000000000000000000000000000000000000 "$head"

printf 'docs-only classifier tests: %d passed, %d failed\n' "$passes" "$failures"
[ "$failures" -eq 0 ]
