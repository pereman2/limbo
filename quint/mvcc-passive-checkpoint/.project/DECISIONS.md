# Decisions

Each line: date, what, source. The approvals list comes from a partial read of the old thread (the delegate paged only the last runs and searched keywords). Pere's earlier approvals may be missing. Treat a gap as unknown, not as "no".

## From Pere

- 2026-11-12. Open draft PRs for the specification and the test work, in pere-mode. Source: thread 6d01c3a3, Pere message at 10:10.
- 2026-11-12. Write notes to a file and push them so nothing is forgotten. Source: thread 6d01c3a3, Pere message at 13:51.
- 2026-11-12. Pere gave a rebase command for the trace replay branch. The child rebased six commits onto 7c607534b and pushed with `--force-with-lease`. Source: child mcp:0a745b71, run 14, 18:25. The exact command is not shown.
- 2026-11-12. Marking a PR ready and merging are Pere's calls. Agents open drafts only. Source: child mcp:0a745b71, 17:22, and the pere-mode standing approvals.
- 2026-11-12. All CPUs may be used, after an earlier limit to one. Source: Claude memory file `feedback_one_cpu.md`.
- 2026-11-13. "Fill all the gaps yourself": the main thread takes the default for every open model-gap decision. Source: `/home/peristocles/scratch-modelgaps/PLAN.md`.
- 2026-11-13. New subagents use GPT (codex, gpt-6.1-sol) for now. Threads that already run keep their model. Source: `PLAN.md`, "Model choice (Pere, 2026-11-13)". Update 2026-11-16: this project's brief says the coordinator is Sonnet and chores use Sonnet low or Haiku. Hard root-cause work picks a strong model per `routes/orchestration.md`.
- 2026-11-13. Pere shared issue #9749 for the recovery work. Source: thread 6d01c3a3, 19:39. Surrounding text not shown.

## Defaults the old coordinator took (2026-11-13, under "fill all the gaps yourself")

Source: `PLAN.md`. Pere has not been shown to confirm these one by one.

1. The own-connection checkpoint fix goes in its own PR from main. #11794 deletes only `rootpage_gc_protected` and keeps the reader-mark term of the GC floor with its assertion. Result: #11831 and #11794.
2. The model gets recovery as a step that can refuse, the open flags (passive on or off, read-only) and the blocking TRUNCATE checkpoint, in a new spec file. Result: #11832.
3. The precondition audit runs as a table plus Rust probes. Spec edits wait for the GC floor thread.
4. No GitHub issue or comment is posted. New bugs go to Pere with a Rust reproducer.

## For this project (2026-11-16, from Pere's brief to this coordinator)

- The project folder lives on `pere/passive-checkpoint-project`, based on `pere/formalize-mvcc-passive-checkpoint`. It does not land in PR #11748.
- Push only to the `pere` remote (pereman2/limbo). Never push to tursodatabase. No PR, no GitHub comment, no ready mark, no merge, no reviewer ping, no message to the old thread or its children.
- At most 4 live delegates. Chores go to Haiku or Sonnet low. No work on D1 or new implementation until Pere OKs the plan.

## Pending Pere choices

- D1: how an open treats WAL frames left by a passive checkpoint. Options not yet written. See ledger L3.
- Ready for review on the stack. See ledger L1.
