# Goal: the passive checkpoint is spec-backed and its open data-loss issues are closed

Done means all four conditions below hold. Each has a check that a command or a link can answer.

## G1. The passive checkpoint is spec-backed

- From `quint/mvcc-passive-checkpoint/`, `make check variants` exits 0 on `main`.
- Every mechanism row in `MODEL-GAPS.md` section 1 says "Modeled" with a spec name, or has a ledger row that models it.
- The replay test of `HARNESS.md` runs the bug traces of each spec and they fail before and pass after their fix.

## G2. The spec is checked in CI

- A GitHub workflow runs `make typecheck` and the small variants on every PR that changes `core/mvcc/` or `quint/`.
- A workflow runs the main configurations on a nightly schedule.
- One green run of each workflow is linked in the ledger row L2.

## G3. Every guard is labeled needed or removed

- Every row of the guard table in `SIMPLIFY.md` step 5 has one label: "needed" (with a variant that fails without it) or "removed" (with a merged PR).
- Every removal followed "How to delete a guard" in `.claude/skills/quint-modeling/SKILL.md`: reachability check, negative control, inductive invariant, Rust assertion first.
- `rg "Kind" MODEL-GAPS.md` shows no mechanism row with an empty kind.

## G4. The open passive data-loss issues are closed or modeled

The issue list: #11499, #8004, #11750, #11749, #11501, #11502, #7960, #8077, #11751, #7480, #9749. The list is from `SIMPLIFY.md` and the old thread. Not verified against GitHub yet (ledger row L7).

- Each issue is closed on GitHub by a merged PR, or
- each issue has a Quint variant that fails with a trace, a Rust reproducer, and a ledger row that names the fix PR or Pere's decision.
- The open-issue state of each is read with `gh issue view <n> --repo tursodatabase/turso --json state`.
