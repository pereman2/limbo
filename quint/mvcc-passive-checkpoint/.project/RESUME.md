# Resume (2026-11-16, written by the first coordinator of this project)

## Focus (2026-11-16, Pere)

Only PR #11748 (spec) and PR #11779 (replay harness) are active. All other ledger rows are parked. The old next three actions (issue sweep, D1 options, spec CI workflow) are not started and wait until Pere lifts the focus. No rebase, push to those PR branches, ready mark, comment, merge or ping. Analysis and options for both PRs: `FOCUS.md`. L4 (the #11779 row) and L8 (the #11748 row) are the active rows.

## Intent

Make the MVCC passive checkpoint backed by a checked Quint spec, run that spec in CI, label every guard needed or removed, and close or model the open passive data-loss issues. See `GOAL.md`.

## State

- Spec PR #11748 (draft) holds the specs and docs in `quint/mvcc-passive-checkpoint/`. This `.project/` folder is on its own branch `pere/passive-checkpoint-project` and must not go into #11748.
- A stack of 7 draft PRs waits for Pere (ledger L1): #11831, #11832, #11833, #11834, #11835, #11836, #11837. All are open, draft, base main, on 2026-11-16.
- #11794 (step 4, one GC floor) is done as a draft PR. Merge is Pere's call.
- The old coordinator thread 6d01c3a3 finished run 109 on 2026-11-14 03:22. Nothing runs there now. Do not message it or its children.

## Open items from the old thread's last turn (run 109)

1. Pere: mark ready #11831, #11833, #11834, #11835, #11836 (ledger L1). #11832 is in the old linked list but not in the ready ask.
2. Pere: accept or reject the #11834 tradeoff. Under constant write contention the logical log has no hard size bound until an idle checkpoint truncates it.
3. Pere: choose D1, how an open treats WAL frames a passive checkpoint left behind. It decides bug 9 (a read-only open fails) and #9749 (ledger L3). The report with the full list is at https://claude.ai/artifact/85zEuzzELmtuVsup8RDBkT. No agent read its contents.
4. #11837 CI was pending when the old thread stopped. On 2026-11-16 it shows 212 passed, 0 failed. It conflicts with main and needs a rebase.
5. #11779 (trace replay): Go jobs failed after the 2026-11-12 rebase. On 2026-11-16 it shows 212 passed, 0 failed, 9 skipped, but it conflicts with main and needs a rebase (ledger L4). Pere's call: mark ready or merge, and keep or drop 10 committed trace files (about 965 KB).
6. The old thread recommends: `make typecheck` plus small variants on each PR that touches `core/mvcc`, main configurations nightly (ledger L2).
7. Bug 9 waits on D1. No agent has a fix in progress.
8. #11837 base is `main`, but the old thread says it stacks on #11794. Not resolved (ledger L8).

## In-flight branches and threads

| Item | Where | State |
|------|-------|-------|
| Old coordinator | thread 6d01c3a3, branch pere/formalize-mvcc-passive-checkpoint, worktree t3-d6b03a3e-cca07efc4328db5d | finished, idle |
| Replay harness child | mcp:0a745b71-04bc-43a0-aec9-5f0b828ce0de, branch pere/mvcc-trace-replay-harness, PR #11779 | done, waits on Pere |
| GC floor child | mcp:120cb040-225b-4b86-ba83-5545c8749bd9, branch pere/mvcc-one-gc-floor, PRs #11794 and #11831 | done, waits on Pere |
| Stale thread | 6df245de "Continue MVCC Lazy Cursor Optimizations", last message 2026-11-01 | probably stale, not verified |

Uncommitted files of the old worktree were not copied. Read them from `/home/peristocles/.t3/worktrees/turso/t3-d6b03a3e-cca07efc4328db5d` if needed.

## Where the facts are

- `/home/peristocles/scratch-modelgaps/EXTRACTION.md`: old-thread extraction. Its approvals list is partial.
- `/home/peristocles/scratch-modelgaps/PLAN.md`: 2026-11-13 plan for the model-gap units U1, U2, U3.
- `/home/peristocles/scratch-modelgaps/PRECONDITIONS.md`: output of U2. Not checked.
- Docs in this folder's parent: `DESIGN*.md`, `SIMPLIFY.md`, `HARNESS.md`, `MODEL-GAPS.md`, `SYNC.md`, `README.md`.
- Skill `quint-modeling` for the method.

## Next three actions (parked under the focus, not started)

1. A read-only delegate reads GitHub state of the G4 issues and fills ledger L7. Cost: a few `gh` calls.
2. Write the D1 option file: 2 or 3 options, each with a Quint trace (ledger L3). A strong-model design thread, one agent, no code changes in `core/`.
3. Add the CI workflow for the spec on a new branch (ledger L2). First measure the time of `make typecheck` and the small variants.

Pere decides L1, D1 and the #11779 call. No agent marks a PR ready, merges, comments or pings.
