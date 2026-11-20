# CI docs-only gate

Snowbar's pull-request workflow always runs a cheap classifier and reports a
stable `CI gate` check. The classifier reads the complete Git pull-request diff
(merge base to head) instead of relying on GitHub's changed-file API or its
path limits. Rename detection is disabled, so both the old and new path of a
rename are checked.

These paths are eligible for a docs-only skip:

- `AGENTS.md`, `README.md` and `niri-rune-bar.md` at the repository root;
- Markdown (`*.md`) anywhere under `docs/`.

Anything else, including a mixed change, a workflow change, or an empty diff,
runs full CI. A classifier error fails the gate instead of permitting a skip.
The classifier prints only `docs_only=true` or `docs_only=false`; changed
paths go to stderr, escaped, and are never put in the workflow's output.

## Checks

`CI gate` is always reported. In full-CI mode it requires successful results
from:

- `Anonymous Cargo build`
- `Nix package build` (x86_64-linux)
- `Nix package build (ARM64)` (aarch64-linux)

The two Nix legs are separate jobs with the same display names as the former
matrix entries. This lets the gate check each architecture directly instead of
depending on matrix fan-in aggregation. Their common steps live in the local
composite action `.github/actions/nix-package-build`, after checkout.

`Experimental niri layer-shell smoke` remains independent. A docs-only skip
also skips it, but the gate does not wait for it or require it to pass.

## Opt-in skip

The skip is controlled by the GitHub Actions repository variable
`SNOWBAR_SKIP_DOCS_CI`. It applies only when the variable is exactly `true`
(case-sensitive) and the classifier reports a docs-only pull request. In that
case the Cargo build, both Nix package builds, and the smoke job are skipped;
the gate requires those three build jobs to report `skipped`. If the variable
is absent or has any other value, every existing check and the smoke job run
exactly as before, under the same check names.

The classifier tests run in the classifier job:

```sh
bash scripts/ci/tests/docs-only-test.sh
```

## Rollout

Repository variables and branch rules are owner settings. Do not change them
from a code review without explicit owner authorization.

1. Verify the workflow change on its own pull request before merging: the
   classifier, `CI gate`, both Nix jobs, the Cargo job, and the smoke job
   all report their results.
2. Verify a docs-only pull request with `SNOWBAR_SKIP_DOCS_CI` unset. All
   existing checks and the smoke job must still run.
3. With owner authorization, add `CI gate` to the branch ruleset as a required
   check. Wait for it to report successfully on a pull request before retiring
   the three existing required checks (`Anonymous Cargo build`,
   `Nix package build`, `Nix package build (ARM64)`). Keep the old checks until
   the gate has a successful run.
4. Only after the gate and branch rules are verified, and with owner
   authorization, set `SNOWBAR_SKIP_DOCS_CI=true`. Open a docs-only pull request
   and confirm the classifier output, the skipped build jobs, and the passing
   gate.
5. To roll back, unset the variable or set it to any value other than `true`.
   The build jobs run again; no workflow edit is needed.
