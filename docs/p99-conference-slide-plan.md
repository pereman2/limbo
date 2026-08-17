# p99 CONF: Turso MVCC + io_uring — slide plan

**Working title:** Concurrent writes in a SQLite-compatible engine: row-level MVCC, io_uring, and tail latency

**Length:** ~25–30 minutes, ~1 slide per minute (28 slides including title/close). Cut slides 21–23 if the room is engine-only; keep them if you want the Cloud punchline.

**Story:** SQLite is fast and simple, but one writer. WAL already solved read/write concurrency with page versions. Turso adds write/write concurrency with row-level MVCC (`BEGIN CONCURRENT`, snapshot isolation, commit-time certification). Durability is a logical log flushed through a cooperative async I/O stack (`io_uring`) so writers are not parked on `fsync`. That same yield model is what lets a single-threaded multi-tenant server multiplex many databases and sync `.db-log` + `.db` to S3 as segments. Then: what it did to p99 write latency.

**Sources for this plan**

- Engine: current `tursodatabase/turso` (`core/mvcc/`, `core/io/io_uring.rs`, `core/vdbe/`, `sync/engine/`).
- This workspace (`pereman2/limbo`) is the 2024 Limbo prototype: read-only SQLite file format + `io_uring` `pread`. Useful as the *origin story* of the async I/O stack, not as the MVCC implementation.
- Cloud: public durability / diskless posts + the user description of turso-server (single-threaded, multi-tenant, S3 sync of logical log and `.db` through segments).
- **`tursodatabase/turso-server` is private** (clone returned “repository not found” with the available GitHub token). Server slides are therefore labeled *inferred from engine interfaces + public Cloud posts + your description*. Fill speaker notes from the real server once you have that tree open.

---

## Talk spine (what the audience should remember)

1. SQLite WAL = many readers + **one writer**. The exclusive write lock is a p99 problem, not just a throughput problem.
2. SQLite’s experimental `BEGIN CONCURRENT` still certifies at **page** granularity. Turso certifies at **row** granularity (Hekaton-style MVCC).
3. Commits do not write B-tree pages. They append a **logical log** (`.db-log`). Checkpoint later materializes into the SQLite file. That split is the hard part, and also the Cloud sync unit.
4. The engine is **cooperatively async** (`IOResult` / VDBE yield), not Tokio-on-the-hot-path. On Linux, `UringIO` batches durability work so a writer is not stalled in `fsync`.
5. That yield model is why a **single-threaded multi-tenant** server can host many DBs: one thread, many connections, I/O completions instead of blocking syscalls. Logical log + `.db` go to S3 as **segments**.

---

## Act 1 — SQLite’s single writer (slides 1–6, ~6 min)

### Slide 1 — Title

**On slide**

- Concurrent writes in SQLite’s shadow
- Turso: row-level MVCC + io_uring
- What it did to p99 write latency
- p99 CONF / your name / Turso

**Speak:** One sentence of setup: we rewrote SQLite in Rust, then we had to make writes concurrent *and* keep tail latency boring.

---

### Slide 2 — Agenda (keep it short)

**On slide**

1. SQLite’s single-writer lock
2. Row-level MVCC (`BEGIN CONCURRENT`)
3. The hard parts (checkpoint, conflicts, compatibility)
4. io_uring: durability without stalling writers
5. One thread, many tenants, S3 segments
6. Evaluation (numbers TBD)

**Speak:** Promise the eval up front so people know this is a p99 talk, not a product pitch.

---

### Slide 3 — SQLite is the right database, except…

**On slide**

- In-process, one file, no server, battle-tested
- Fast: ~150k durable rows/s with batching (full sync) is not the issue
- The issue: **one writer at a time**

**Visual:** A single file → many readers OK / writers in a queue.

**Speak:** Do not dunk on SQLite. The audience loves it. Frame this as “the last remaining tax.”

---

### Slide 4 — Single-writer transactions (the problem slide)

**On slide**

```
BEGIN;          -- exclusive writer
… business logic …
COMMIT;         -- everyone else waited
```

- Second writer: `SQLITE_BUSY` / `database is locked`
- Adding threads does **not** add write throughput
- Interactive txns (read → compute → write) hold the lock for the whole compute window

**Visual:** Timeline: Writer A occupies the lock for 1ms of SQL + 5ms of app logic. Writers B–H are blocked. p99 is dominated by queueing, not disk.

**Speak:** This is the slide you asked for. Spend a full minute. Emphasize *blocking behavior*, not just “SQLite is slow” (it isn’t). Cite the Turso concurrent-writes blog: with 1ms of compute in the txn, throughput collapses and extra threads do nothing.

**Code / facts**

- SQLite WAL: N readers + 1 writer. Rollback journal: even readers block.
- `SQLITE_BUSY` is the production API for this lock.

---

### Slide 5 — WAL already did page-level versioning

**On slide**

- WAL = page versions in `.db-wal`
- Readers take a **read mark**; they do not see in-flight writer pages
- Result: **read/write concurrency, not write/write concurrency**

**Visual:** Two columns: “Reader snapshot (old pages)” vs “Writer (new WAL frames)”. Caption: still one writer.

**Speak:** Steal this from the abstract. A lot of people think WAL already solved concurrent writes. It solved concurrent *reads during a write*.

---

### Slide 6 — Why this is a p99 talk

**On slide**

- Mean write time can look fine (one fast writer)
- Tail = lock wait + `fsync` + checkpoint
- Edge / embedded / agents: you wanted SQLite’s simplicity **and** predictable tails

**Visual:** Cartoon latency histogram: p50 in the noise, p99 in the lock queue.

**Speak:** Set the metric you will return to: **p99 commit latency under concurrent writers**, durable (`synchronous=FULL`). Throughput is supporting evidence.

---

## Act 2 — MVCC (slides 7–16, ~10 min)

### Slide 7 — SQLite `BEGIN CONCURRENT` (the almost)

**On slide (table)**

| | SQLite default | SQLite BEGIN CONCURRENT branch | Turso |
|---|---|---|---|
| Writers | 1 | optimistic N | optimistic N |
| Lock moment | BEGIN | COMMIT | COMMIT |
| Conflict grain | whole DB | **page** | **row** |
| Shipped? | yes | experimental, never merged | engine + Cloud preview |

**Speak:** Two txns updating different rows on the same B-tree page still collide in SQLite’s branch. That is why page-level OCC is not enough for small-row OLTP.

---

### Slide 8 — Turso: Hekaton-style MVCC on a SQLite file

**On slide**

- Inspired by Larson et al., VLDB 2011 (Hekaton)
- In-memory version chains, optimistic writers, **first-committer-wins**
- Isolation: **snapshot isolation** (not serializable — say this out loud)
- Layered on the existing B-tree + WAL, not a greenfield store

**Visual:** `MvStore` sitting on top of pager/WAL/DB.

**Code**

- `core/mvcc/mod.rs` — paper citation + anomaly list
- `core/mvcc/database/mod.rs` — `MvStore`, `RowVersion`, commit SM
- Enable: `PRAGMA journal_mode = mvcc;` then `BEGIN CONCURRENT;`

**Speak:** We did not invent MVCC. We adapted a main-memory OCC design to a SQLite-compatible on-disk format.

---

### Slide 9 — Architecture: three tiers

**On slide**

```
BEGIN CONCURRENT
        │
        ▼
   MvStore (SkipMap of row version chains)   ← hot path
        │ commit
        ▼
   logical log (.db-log)                     ← durability
        │ checkpoint
        ▼
   B-tree pages → WAL → .db                  ← SQLite file
```

**Speak:** Writes during the txn never touch the B-tree. That is why they do not take the SQLite write lock. The B-tree is the checkpoint/recovery image, not the commit path.

**Code**

- Dual-cursor reads: `core/mvcc/cursor.rs` (merge MvStore + B-tree)
- If the row is in MvStore, that version wins (all writes go through MVCC)

---

### Slide 10 — Version chains (the picture people will photograph)

**On slide** — inventory example from the Oct 2025 blog:

```
products
  row 1 "Mug"
    v1  begin=T1  end=T3
    v2  begin=T3  end=∞
  row 2 "Teapot"
    v1  begin=T2  end=∞
```

- `begin` / `end` packed as TxID (uncommitted) or Timestamp (committed)
- Visibility: version visible if `begin ≤ snapshot < end`
- Speculative reads of `Preparing` versions + **commit dependencies** (Hekaton §2.7)

**Code**

- `RowVersion` in `core/mvcc/database/mod.rs`
- `MvccClock`: commit timestamp published as `Preparing(ts)` **under the clock lock** so a reader cannot observe a torn snapshot

**Speak:** This is snapshot isolation. T3’s update of Mug does not block T2 inserting Teapot.

---

### Slide 11 — `BEGIN CONCURRENT` lifecycle

**On slide**

1. Assign `begin_ts` from the logical clock (no pager write lock)
2. DML appends a new `RowVersion { begin: TxID(self), end: None }` — **no conflict check**
3. Track write set (and reads that matter for SI)
4. COMMIT → certification (next slide)

**Speak:** Pure optimistic. Two txns may insert the same rowid; first committer wins. Quote the comment in `insert()`: *“We do NOT check for conflicts at insert time.”*

**SQLite-compat note:** one active txn per connection. Concurrency = **multiple connections**, not interleaved statements.

---

### Slide 12 — Commit-time certification

**On slide — commit state machine (compressed)**

1. Prepare: `end_ts`, atomically `Preparing(end_ts)`
2. Gates: exclusive txn? schema cookie changed?
3. **Certify write set** (row + index): walk version chain backwards
4. Wait commit dependencies
5. Append + (optional) fsync logical log
6. Publish: rewrite TxIDs → timestamps, notify dependents
7. Maybe auto-checkpoint / incremental GC

**Conflict:** `LimboError::WriteWriteConflict` → `SQLITE_BUSY`. Retry from `BEGIN CONCURRENT`.

**Code**

- `CommitStateMachine` in `core/mvcc/database/mod.rs`
- Batch/yield every 1024 rowids so a huge commit does not monopolize the executor (`MVCC_COMMIT_BATCH_SIZE`)

**Speak:** Certification is the only serialization point that *must* be right. Lower `end_ts` wins if two txns are both `Preparing`.

---

### Slide 13 — Logical log: the real commit record

**On slide**

- File: `.db-log` (header 56B + TX frames)
- Frame: header (commit_ts, op_count) + upsert/delete ops + CRC trailer
- Torn tail = EOF (SQLite WAL prefix semantics)
- `synchronous=FULL` → fsync this file on commit
- This is **logical** (row ops), not page images

**Why it matters for p99:** commit I/O size scales with dirty rows, not dirty B-tree pages.

**Why it matters for Cloud:** this file is what you can ship as a sync stream / S3 object, independently of the `.db` generation.

**Code**

- `core/mvcc/persistent_storage/logical_log.rs` (format + durability contract)
- `DurableStorage::log_tx` / `on_log_write_complete` — hook for extra durability (the Cloud server can wait on S3 here without changing certification)

---

### Slide 14 — Hard part: checkpointing

**On slide — crash-safe order** (from `RECOVERY_SEMANTICS.md`)

1. Stop-the-world lock (TRUNCATE) *or* unlocked I/O + short publish (PASSIVE, experimental)
2. Write versions into pager (WAL)
3. Commit pager txn **including** `__turso_internal_mvcc_meta.persistent_tx_ts_max`
4. WAL → DB file, fsync DB
5. Truncate logical log, fsync log
6. **Truncate WAL last** (safety net)

**p99 hook:** blocking checkpoint is a tail-latency cliff. `perf/checkpoint-bench` exists specifically to plot that cliff vs passive mode.

**Speak:** Checkpoint is where MVCC meets SQLite compatibility. Get the order wrong and recovery lies. The metadata watermark is the replay boundary: recover only frames with `commit_ts > persistent_tx_ts_max`.

**Visual:** Four artifacts: `.db` / `.db-wal` / `.db-log` / meta row.

---

### Slide 15 — Hard part: conflict handling

**On slide**

- Hot row → genuine WW conflict, retry (this is ACID, not a bug)
- Distinct rows → both commit (even if they share a B-tree page)
- Exclusive `BEGIN IMMEDIATE` / DDL: concurrent txns may run but **cannot commit**
- Schema cookie mismatch → `SchemaConflict`
- Preparing writer aborted → dependents abort (`CommitDependencyAborted`)

**Speak:** “Do I still need retry logic?” Yes — less of it. Queueing on `BEGIN` is gone; remaining retries are real row conflicts. Show the two-connection `accounts` example from `docs/sql-reference/statements/transactions.mdx`.

---

### Slide 16 — Hard part: SQLite compatibility

**On slide (honest list)**

- Same file format after checkpoint; MVCC writes are invisible if you reopen without MVCC until checkpointed
- DDL / header updates still want exclusive txn
- One txn per connection (SQLite-shaped API)
- SI, not serializable (phantoms / write skew still TODO in `core/mvcc/mod.rs`)
- Manual still says “indexes don’t work” in places — **code has index version chains**. Don’t read stale `docs/agent-guides/mvcc.md` on stage (it also claims recovery/GC are missing; both exist)

**Speak:** Compatibility is a product constraint, not a footnote. Every Cloud customer has SQLite muscle memory.

---

## Act 3 — io_uring (slides 17–20, ~5 min)

### Slide 17 — `fsync` is the other p99 villain

**On slide**

- SQLite FULL: `fsync` WAL (and sometimes more) on every commit
- ~2ms local NVMe fsync is already a tail contributor
- A blocked writer holds the *SQLite* write lock → everyone pays
- MVCC removes the lock; **blocking I/O would still serialize the thread**

**Speak:** Concurrent certification is useless if the commit path does `pwrite`+`fsync` synchronously on the only executor.

---

### Slide 18 — Cooperative completions, not async/await

**On slide**

```rust
enum IOResult<T> {
    Done(T),
    IO(IOCompletions),  // yield; caller drives io.step()
}
```

- VDBE `StepResult::IO` / `Yield`
- Explicit state machines for commit + checkpoint
- Completions keep write buffers alive until the kernel is done

**Code**

- `core/types.rs` — `IOResult`
- `docs/agent-guides/async-io-model.md`
- `core/vdbe/mod.rs` — drive `io.step()` so other work is not starved

**Speak:** This is the Limbo/EdgeSys ’24 idea, grown up. This workspace’s `core/io/linux.rs` was a 128-entry ring and `pread` only. Production `UringIO` is the same shape: submit, yield, complete.

**Origin visual (optional):** tiny code snippet from this repo’s `LinuxIO::pread` vs today’s `UringIO`.

---

### Slide 19 — `UringIO` on Linux

**On slide**

- 512-entry ring, **SQPOLL**, iovec pool, writev for contiguous pages
- Threads may **submit concurrently**; **one leader** in `submit_and_wait` (Linux wakes a single waiter)
- `MAX_WAIT = 4`: fewer `io_uring_enter` syscalls vs a bit more single-op latency — say this; it is a p99 knob
- Unsupported opcodes (e.g. ftruncate) fall back to POSIX

**Code:** `core/io/io_uring.rs` (leader/follower comments around the wait lock)

**Visual:** SQ → kernel → CQ → `Completion::complete()` → waker → VDBE resumes commit SM.

---

### Slide 20 — Durability without stalling writers

**On slide**

| Stage | What is flushed | When (FULL) |
|---|---|---|
| Commit | logical log append | per commit, async completion |
| Checkpoint | DB file, then log | periodic |
| NORMAL | skip commit fsync | checkpoint still fsyncs DB |

- Writer thread is **not** inside `fsync`; it is in the io_uring wait/complete loop
- Other connections keep certifying / executing while this commit’s CQEs are in flight
- `DurableStorage::on_log_write_complete` is the Cloud hook: extra durability (S3 PUT) can be another completion, same state machine

**Speak:** This is the sentence from the abstract: *“we flush durability work without stalling writers.”* Tie it back to p99: you want the wait *overlapped*, not *queued behind a lock*.

---

## Act 4 — One thread, many tenants, S3 (slides 21–23, ~4 min)

> Server internals inferred. Replace with turso-server screenshots/code when you have the private tree.

### Slide 21 — turso-server: single-threaded multi-tenant

**On slide**

- One process, many SQLite-compatible files
- **Single-threaded event loop** (matches VDBE: one txn per connection, sequential opcodes)
- Concurrency = many connections / many DBs, cooperative yield on I/O
- Multi-tenant batching is also how S3 Express stays cheap (public diskless post)

**Speak:** People hear “single-threaded” and think “slow.” In this design, single-threaded is the *isolation and tail-latency* choice: no cross-tenant lock convoys on a thread pool, deterministic scheduling, io_uring as the parallelism.

**Why MVCC matters here:** without concurrent writes, each DB still has a single writer, but *interactive* txns would block that DB’s other writers. With agents / many small DBs, that is the Cloud workload.

---

### Slide 22 — Why the engine shape fits the server

**On slide**

```
request → VDBE step
            │ IOResult::IO
            ▼
     io_uring SQ  (maybe many tenants’ ops)
            │
            ▼
     CQE → resume that connection only
```

- Shared `MvStore` per database, not per process
- Commit SM yields every 1024 rows so one fat txn cannot freeze the loop
- Blocking checkpoint is lethal on a 1-thread server → **passive checkpoint** is a Cloud-relevant eval

**Speak:** This is the architectural punchline connecting Acts 2, 3, and 4.

---

### Slide 23 — S3: logical log + `.db` as segments

**On slide**

- `.db` split into **~128kB segments** (generation = the set of segments that make a file)
- Lazy fetch: serve a query without the whole file (public durability post)
- **Before tursodb:** durable unit ≈ WAL frames, then checkpoint into `.db` segments
- **With tursodb:** durable unit ≈ **logical log frames** (row ops) + checkpointed `.db` segments
- S3 Express: ack commit after batched PUT; local NVMe is a write-through cache
- Engine already has `RemotePullProtocol::MvccLogical` and generation/bootstrap catch-up (`sync/engine`)

**Visual:** two object streams — `log` segments (hot, small, per-commit batched) and `db` segments (cold, after checkpoint).

**Speak (your words):** “We add tursodb to a single-threaded multi-tenant server with S3 synchronization of the logical log and `.db` through segments.” That is this slide. If you can show a real segment layout from turso-server, do it here.

**Eval implication:** p99 in Cloud = engine certify + log append + **batched S3 Express PUT** (~6–7ms in the diskless post), not local fsync (~2ms). Be explicit which number you are showing.

---

## Act 5 — Evaluation placeholders (slides 24–27, ~4 min)

Leave these as **chart frames**. Fill after you run the benches. Suggested harnesses already in-tree:

- `perf/throughput/` — thread scaling + compute-in-txn vs rusqlite (`plot-thread-scaling.py`, `plot-compute-impact.py`)
- `perf/checkpoint-bench` — TRUNCATE vs PASSIVE tail latency under concurrent BEGIN CONCURRENT
- `perf/latency/` — p50–p999 tooling
- Blog numbers (Oct 2025, treat as *historical*, re-measure): up to **4×** write throughput vs SQLite with 1ms compute @ 8 threads; SQLite still wins single-thread no-compute

### Slide 24 — Methodology (placeholder)

**On slide (fill in)**

- Hardware / kernel / io_uring features (SQPOLL on?)
- `PRAGMA synchronous = FULL` vs NORMAL
- Workload: N connections, batch size, keyspace, conflict rate
- Isolation: `BEGIN CONCURRENT` vs SQLite `BEGIN IMMEDIATE`
- What is *not* measured (page cache cold vs warm, checkpoint on/off)

**Speak:** p99 people will roast you if this slide is missing.

---

### Slide 25 — Throughput vs SQLite (placeholder)

**Chart:** rows/s vs thread count, with and without in-txn compute.

**Expected shape (from blog, verify):** SQLite flat line; Turso rises until conflict or IO saturation.

**Caption to write later:** “SQLite: more threads ≠ more writes. Turso: cores help until we hit row conflicts or durability.”

---

### Slide 26 — Hero: p99 write latency under concurrent load (placeholder)

**Chart:** p50 / p99 / p999 commit latency vs #writers.

**Series to include**

1. SQLite WAL, FULL
2. Turso MVCC, FULL, blocking checkpoint
3. Turso MVCC, FULL, **passive** checkpoint
4. Optional: Turso MVCC + io_uring vs POSIX fallback (prove the I/O claim)

**Callouts you want the chart to make**

- SQLite p99 grows with writer count (lock queue)
- MVCC p99 stays flatter on low-conflict keyspaces
- Blocking checkpoint injects spikes (time-series from `checkpoint-bench`)
- io_uring should cut the *stall* component, not the S3 RTT if this is Cloud

---

### Slide 27 — Secondary eval (placeholder, pick 1–2)

**Options (do not show all)**

- Conflict rate vs keyspace (hot row vs disjoint rows)
- Checkpoint: logical log size over time (did checkpoint keep up?)
- Compute-in-txn: the 4× slide, redrawn with **latency** not just throughput
- Cloud: local FULL vs S3 Express batched PUT p99 (if you are telling the server story)

---

## Act 6 — Close (slides 28–30, ~2 min)

### Slide 28 — Limitations (trust slide)

**On slide**

- Experimental; Cloud concurrent writes are an early preview
- SI, not serializable
- Version GC / memory: full row copies, not deltas (blog future work)
- Row-version vector still under a lock — not wait-free Hekaton
- Blocking checkpoint still the default; passive is experimental
- Multi-process MVCC not supported

**Speak:** Don’t overclaim. p99 audience respects unfinished systems that know their cliffs.

---

### Slide 29 — Takeaways

**On slide**

1. WAL solved readers. **Row-level MVCC** is how writers stop blocking each other.
2. Commit is a **logical log** + certify; the SQLite file is a checkpoint.
3. **io_uring + cooperative yield** keeps durability off the writer’s critical section — and fits a one-thread multi-tenant server.
4. Measure **p99 commit latency under concurrent load**, including checkpoint.

---

### Slide 30 — Q&A / backup

Backup slides (not in the 30-min cut):

- B1: Full commit state machine
- B2: Recovery case table from `docs/internals/mvcc/RECOVERY_SEMANTICS.md`
- B3: Logical log frame layout (encrypted vs not)
- B4: Dual-cursor merge
- B5: `MAX_WAIT=4` / SQPOLL knobs
- B6: Hekaton paper vs Turso deltas (clock lock, logical log, B-tree materialization)
- B7: How to enable: `PRAGMA journal_mode=mvcc;` / Cloud `BEGIN CONCURRENT`

---

## Timing cheat sheet

| Min | Slides | Beat |
|-----|--------|------|
| 0–1 | 1–2 | Title, agenda |
| 1–6 | 3–6 | SQLite + WAL + why p99 |
| 6–16 | 7–16 | MVCC architecture + hard parts |
| 16–21 | 17–20 | io_uring |
| 21–25 | 21–23 | Server / S3 (or skip) |
| 25–29 | 24–27 | Eval |
| 29–30 | 28–30 | Limits, takeaways |

If you only have 20 minutes: drop 21–23 and 27, keep 26 as the only chart.

---

## Design notes for the actual deck

- Dark slides, one idea each. Slide 10 (version chains) and 14 (checkpoint order) are the two diagrams worth making beautiful.
- Do not paste Rust on stage except: `IOResult`, `BEGIN CONCURRENT`, and maybe the `MAX_WAIT` comment.
- Every architecture slide should end with a **latency implication** (“this is why p99 does / doesn’t spike”).
- Re-run `perf/throughput` and `perf/checkpoint-bench` close to the talk; the Oct 2025 4× number is a preview, not your p99 CONF figure.
- `docs/agent-guides/mvcc.md` is stale. Prefer `docs/internals/mvcc/RECOVERY_SEMANTICS.md` and the source.

---

## Open items before locking the deck

1. Clone `turso-server` and replace Act 4 speaker notes with: event loop, tenant scheduling, exact segment size, when logical log is PUT vs when `.db` generation is PUT, batching window.
2. Decide **local engine p99** vs **Cloud p99 (S3 Express)**. They are different talks; the abstract can cover both if you label axes.
3. Fill slides 25–27 with real plots.
4. Confirm current product names on stage: Turso (engine), Turso Cloud, `tursodb`, `BEGIN CONCURRENT`, `PRAGMA journal_mode=mvcc`.
