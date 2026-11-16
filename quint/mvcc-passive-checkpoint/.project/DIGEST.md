# Digest (2026-11-16)

## Goal
## G1. The passive checkpoint is spec-backed
## G2. The spec is checked in CI
## G3. Every guard is labeled needed or removed
## G4. The open passive data-loss issues are closed or modeled

## Open ledger rows (id | status | what | owner)
- L1 | parked (was: waiting on Pere) | Pere's ready-for-review call on the stacked draft PRs #11831 (own-connection checkpoint snapshot), #11832 (recovery model spec), #11833 (vacuum vs pas | owner: Pere
- L2 | parked (was: open) | Run the spec in CI (G2). Workflow for `make typecheck` plus small variants on PRs that touch `core/mvcc/` or `quint/`. Nightly run of the main configu | unassigned
- L3 | parked (was: open, not started) | Design decision D1: how opening a database treats WAL frames that a passive checkpoint left behind. Deliverable: 2 or 3 options, each with a Quint tra | choice: Pere. Options work: unassigned
- L4 | waiting on Pere | Status of the trace replay harness (HARNESS.md), PR #11779.  | child mcp:0a745b71-04bc-43a0-aec9-5f0b828ce0de (done, waits 
- L5 | parked (was: open) | Remaining model gaps: index rows, blocking-mode GC, larger bounds. Also the flag-on TRUNCATE mode (T) that no spec models.  | unassigned
- L6 | parked (was: open) | SIMPLIFY steps 1, 2, 3, 5, 6, 7. One PR per step.  | unassigned
- L6a | parked (was: done (PR open as draft, not merged)) | SIMPLIFY step 4: one GC floor (#11794).  | child mcp:120cb040-225b-4b86-ba83-5545c8749bd9 (done)
- L7 | parked (was: open) | Read the GitHub state of each issue in G4 (open or closed, linked PR).  | unassigned
- L8 | waiting on Pere (active, see FOCUS.md) | Land PR #11748 (spec) and its stack: Pere's merge call and order.  | owner: Pere

## Last decisions
- 2026-11-12. Pere gave a rebase command for the trace replay branch. The child rebased six commits onto 7c607534b and pushed with `--force-with-lease`. Source: child mcp:0a745b71, run 14, 18:25. The exact command is not
- 2026-11-12. Marking a PR ready and merging are Pere's calls. Agents open drafts only. Source: child mcp:0a745b71, 17:22, and the pere-mode standing approvals.
- 2026-11-12. All CPUs may be used, after an earlier limit to one. Source: Claude memory file `feedback_one_cpu.md`.
- 2026-11-13. "Fill all the gaps yourself": the main thread takes the default for every open model-gap decision. Source: `/home/peristocles/scratch-modelgaps/PLAN.md`.
- 2026-11-13. New subagents use GPT (codex, gpt-6.1-sol) for now. Threads that already run keep their model. Source: `PLAN.md`, "Model choice (Pere, 2026-11-13)". Update 2026-11-16: this project's brief says the coordina
- 2026-11-13. Pere shared issue #9749 for the recovery work. Source: thread 6d01c3a3, 19:39. Surrounding text not shown.

## Resume
## Open items from the old thread's last turn (run 109)

1. Pere: mark ready #11831, #11833, #11834, #11835, #11836 (ledger L1). #11832 is in the old linked list but not in the ready ask.
2. Pere: accept or reject the #11834 tradeoff. Under constant write contention the logical log has no hard size bound until an idle checkpoint truncates it.
3. Pere: choose D1, how an open treats WAL frames a passive checkpoint left behind. It decides bug 9 (a read-only open fails) and #9749 (ledger L3). The report with the full list is at https://claude.ai/artifact/85zEuzzELmtuVsup8RDBkT. No agent read its conten
4. #11837 CI was pending when the old thread stopped. On 2026-11-16 it shows 212 passed, 0 failed. It conflicts with main and needs a rebase.
5. #11779 (trace replay): Go jobs failed after the 2026-11-12 rebase. On 2026-11-16 it shows 212 passed, 0 failed, 9 skipped, but it conflicts with main and needs a rebase (ledger L4). Pere's call: mark ready or merge, and keep or drop 10 committed trace files
6. The old thread recommends: `make typecheck` plus small variants on each PR that touches `core/mvcc`, main configurations nightly (ledger L2).
7. Bug 9 waits on D1. No agent has a fix in progress.
8. #11837 base is `main`, but the old thread says it stacks on #11794. Not resolved (ledger L8).

## Next three actions (parked under the focus, not started)

1. A read-only delegate reads GitHub state of the G4 issues and fills ledger L7. Cost: a few `gh` calls.
2. Write the D1 option file: 2 or 3 options, each with a Quint trace (ledger L3). A strong-model design thread, one agent, no code changes in `core/`.
3. Add the CI workflow for the spec on a new branch (ledger L2). First measure the time of `make typecheck` and the small variants.

Pere decides L1, D1 and the #11779 call. No agent marks a PR ready, merges, comments or pings.
