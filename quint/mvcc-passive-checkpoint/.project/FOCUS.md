# Focus: PR #11748 and PR #11779 (2026-11-16)

Pere wants to deal with only these two PRs. Everything else in the ledger is parked. This file is read-only analysis. Nothing was rebased, pushed to a PR branch, commented, marked ready, merged or pinged. Facts come from `gh` and read-only `git` runs on 2026-11-16 by two delegates. Raw reports: `/home/peristocles/scratch-modelgaps/FOCUS-11748.md` and `FOCUS-11779.md`.

## The stack in one picture

```
main (242 commits ahead of the merge-base 0c9fe0dd9, Nov 11)
 └─ #11748  8 commits, +7003 -0, spec and docs only, no Rust       head 7c607534b
     ├─ #11779  +6 commits (replay test + 10 traces)               head 2836c87e6
     ├─ #11794  +commits (one GC floor)
     ├─ #11832  +commits (recovery model)
     └─ #11837  +commits (checkpoint-stops model)
```

All of these PRs target `main`, not #11748. Each carries the 8 commits of #11748, so their file lists include its 19 files. #11831, #11833, #11834, #11835 and #11836 do not contain #11748 and share no file with it. Whether #11794 also contains #11779 is not verified.

## PR #11748: quint: specify the MVCC passive checkpoint

State: open, draft, base main, 0 comments, 0 reviews.

What it contains: 8 commits. Three specs (row protocol, table ids and schema, sequence compaction), four design and plan docs, and a skill guide for Quint modeling.

Size: 19 files, +7003, -0. No Rust. No product code.

| Kind | Lines |
|---|---|
| Specs, 6 `.qnt` files | 4879 (70%) |
| Docs, 9 `.md` files in `quint/mvcc-passive-checkpoint/` | 1759 |
| Skill `.claude/skills/quint-modeling/SKILL.md` | 227 |
| Makefile | 133 |
| `.gitignore`, `AGENTS.md` | 5 |

Biggest files: `MvccPassiveSchemaConfigs.qnt` 1408, `MvccPassiveSchema.qnt` 1156, `MvccPassiveCheckpoint.qnt` 895, `DESIGN-schema.md` 441.

CI: 221 checks. 212 passed, 9 skipped, 0 failed, 0 pending. No workflow runs the Quint specs, so these checks say nothing about the spec. The spec runs only by hand with `make check variants`.

Conflicts with main: none. `gh` says MERGEABLE and CLEAN. `git merge-tree` finds no conflict. It is 242 commits behind main, which does not matter because it adds files only.

What a reviewer struggles with:
- Size. 4879 lines of Quint is hard to review by a person who does not read Quint. No reviewer has looked yet.
- No CI. A reviewer cannot see the spec pass. They must install Quint, Java and Apalache and run `make check`.
- The PR mixes four things: specs, design docs, plans for future work (`SIMPLIFY.md`, `SYNC.md`, `MODEL-GAPS.md`, `HARNESS.md`) and a skill. A reviewer must judge each differently.
- The plan docs describe work that is not built. They can go stale.

What is left to make it mergeable: nothing technical. It has no conflict and green CI. Left: a ready mark, a reviewer, and Pere's choice on size (options below). The merge method (squash or merge commit) decides how the four stacked PRs must be rebased. Not verified which method turso uses.

### Options for #11748

A. Land as is (recommended).
- Why: it is purely additive and touches no product code, so the risk of a regression is zero. Four PRs stack on it. Landing it shrinks each of their diffs by 19 files and 7003 lines, which makes them reviewable.
- Cost: one big read for a reviewer. Mitigate with a reading order in the PR description: `README.md`, `DESIGN.md`, then the specs.
- Exposure: the spec lands with no CI, so it can rot. The CI row (L2) stays parked and comes right after.

B. Split into smaller PRs.
- The commits already map to a split: (1) row spec + Makefile + README + `DESIGN.md` + `DESIGN-rows.md`, about 2300 lines; (2) schema spec + `DESIGN-schema.md`, about 3000 lines; (3) sequence spec + `DESIGN-sequences.md`, about 1100 lines; (4) plan docs + skill, about 800 lines.
- Cost: this rewrites the history of #11748 and every stacked branch needs a new base. It adds a lot of rebasing for a change that has no risk. The schema part is still about 3000 lines.

C. Move the plan docs out.
- Take `SIMPLIFY.md`, `SYNC.md`, `MODEL-GAPS.md`, `HARNESS.md` and the skill out of #11748 (about 800 lines). Land only specs and the design docs. Keep the plan docs on a branch until the work they describe exists.
- Cost: `HARNESS.md` is what #11779 builds. `SIMPLIFY.md` is referenced by the stacked PRs. Moving them breaks those links and edits all stacked branches.

Recommendation: A. The problem is size and not risk. Splitting costs more work than it saves, and C breaks links in four PRs.

## PR #11779: core/mvcc: replay Quint traces of the passive checkpoint on the engine

State: open, draft, base main, head 2836c87e6, 0 comments, 0 reviews.

What it contains: 14 commits against main. 8 are #11748's. 6 are its own: a Quint change so the row spec writes traces the engine can replay, a test hook in the checkpoint state machine, the row replay, the crash-recovery check, the schema bug trace replay, and the `HARNESS.md` update.

How the test works: it reads a Quint trace (ITF, a JSON format). Each state names an action and the values it picked. The test runs that action on a real database and steps the checkpoint by hand. After each step it compares reads, snapshot, WAL frames, backfill, logical log and crash recovery with the trace. A main-config trace must match. A bug-variant trace must fail at the same step.

Size: 35 files, +8764 -2 against main, which includes #11748. Its own parts:

| Part | Lines or bytes |
|---|---|
| Rust test code `core/mvcc/database/mbt_tests/{mod,itf,rows,schema}.rs` | +1496 |
| Engine file `core/mvcc/database/checkpoint_state_machine.rs` | +78 -2 |
| `core/mvcc/database/tests.rs` | +3 |
| 10 committed trace files `mbt_tests/traces/*.itf.json` | 964,941 bytes |
| Makefile, `HARNESS.md`, `DESIGN-rows.md` edits | per-file split not verified |

Trace files: 8 random-seed rows traces of 116 KB to 130 KB each (about 850 KB), one hand-written rows trace (35 KB) and two schema bug traces (44 KB and 33 KB). A trace is a single JSON line, so it cannot be reviewed.

The engine change is test-only. All new code in `checkpoint_state_machine.rs` is `#[cfg(test)]`. In a release build the preemption threshold stays the constant `COLLECT_PREEMPTION_THRESHOLD`. Release behavior does not change.

CI: 221 checks. 212 passed, 9 skipped, 0 failed, 0 pending. The Go failure from the old thread is gone. These results are from 2026-11-12 and are old. The replay test runs inside the normal `cargo nextest` job and adds no job. The tests that appear in the log take 0.006 s to 0.277 s. Times of the others are not verified.

Conflicts with main: yes. `gh` says CONFLICTING. `git merge-tree` finds 2 conflicted files: `core/mvcc/database/checkpoint.rs` and `core/mvcc/database/tests.rs`. The PR does not edit `checkpoint.rs`, so main changed it and the cause needs a look (not verified). It is 242 commits behind main and 14 ahead. Because the test uses `pub(crate)` engine items from a branch that is 242 commits old, the replay test may fail after a rebase. Not verified.

What a reviewer struggles with:
- Reviewing 10 one-line JSON traces, 965 KB, that nobody can read.
- The test reads `pub(crate)` engine items and a hook inside the checkpoint state machine. A core reviewer must accept those hooks.
- The test needs no Quint at test time, but the traces are only useful with the specs of #11748. A reviewer must read #11748 first.
- The 8 random traces are one sample each. They do not say which behavior they cover.

What is left to make it mergeable:
1. Land #11748 first (or fold it in, see below).
2. Rebase on main. If #11748 is squash-merged, use `git rebase --onto main 7c607534b` so the 8 old commits drop out. Resolve the 2 conflicts.
3. Run the `mbt_tests` on the rebased branch. Fix any breakage from 242 commits of engine change.
4. Decide the trace files.
5. Rerun CI.

### Options for #11779

A. Land after #11748, rebased, with all 10 traces as is.
- Cost: adds 965 KB to git history for good. Large generated blobs cannot be removed from history later.

B. Land after #11748, rebased, with fewer traces (recommended).
- Keep `rows-hand` (35 KB), the two schema bug traces (77 KB) and 2 of the random traces (about 240 KB). That is about 350 KB, 36% of today. The other 6 random traces go away. The folder mode `MBT_TRACES=<dir>` with the `--ignored` test `row_traces_from_folder` stays, so anyone can replay a fresh batch from `make traces`.
- Why: the bug traces and the hand trace prove the harness works both ways (matches and fails at the right step). More random samples add little. The test list in `rows.rs` (about lines 583 to 599) must lose 6 `include_str!` lines.
- Cost: less random coverage in CI. Not measured: whether any single random trace catches something the others do not.

C. Land the harness without committed random traces.
- Keep only the hand trace and the two bug traces (112 KB). Random traces stay a manual step with `make traces` and the folder mode.
- Cost: CI never replays a random trace.

D. Fold #11779 into #11748. Not recommended. It adds Rust and a core change to a PR that has none today, so it loses the zero-risk property that makes option A for #11748 work. It also makes the first review bigger (+8764 lines).

Recommendation: B, after #11748 lands. If #11748 will wait long for a reviewer, option D is not the answer. Instead rebase #11779 onto main without squashing, so it carries #11748's commits and merges in order.

## Decisions Pere needs to make

1. #11748: land as is (A), split (B) or move plan docs out (C)? My recommendation: A.
2. Who reviews #11748, and when do you mark it ready? An agent does not mark it.
3. How does turso merge PRs: squash or merge commit? It decides how #11779, #11794, #11832 and #11837 are rebased after #11748 lands. I can check the merge history with a read-only `gh` call.
4. #11779 traces: all 10 (A), 5 (B) or 3 (C)? My recommendation: B.
5. May I rebase #11779 onto current main in a local scratch worktree, with no push, and run `mbt_tests` to see whether the replay test still passes? It answers the biggest unknown and changes nothing on GitHub.
6. Accept that #11748 lands with no CI running the spec? The CI row stays parked and would follow right after.
