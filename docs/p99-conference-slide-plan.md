# p99 CONF: Turso MVCC + io_uring — slide plan (18–20 min)

**Working title:** Concurrent writes in a SQLite-compatible engine: row-level MVCC, io_uring, and tail latency

**Slot:** 18–20 minutes, **pre-recorded in September**. No live Q&A. Target **16 slides** (~70–80s average). Record to ~19:00, leave 60s of trim room.

**Story:** SQLite is fast, but one writer. WAL solved read/write concurrency, not write/write. Turso adds row-level MVCC (`BEGIN CONCURRENT`, snapshot isolation, commit-time certification). Commits append a logical log flushed with cooperative `io_uring`, so writers are not parked on `fsync`. That yield model is also how a single-threaded multi-tenant server syncs `.db-log` + `.db` to S3 as segments. Then: p99 write latency under concurrent load.

**Sources:** current `tursodatabase/turso` (`core/mvcc/`, `core/io/io_uring.rs`, `sync/engine`). This repo is the 2024 Limbo `io_uring` prototype, not the MVCC code. `turso-server` is private — slide 11 is inferred; fill from that tree before recording.

---

## Talk spine (four sentences)

1. The exclusive write lock is a **p99** problem: queueing, not disk.
2. Turso certifies at **row** grain; SQLite’s `BEGIN CONCURRENT` branch was still **page** grain.
3. Commit = **logical log** + async flush; the SQLite file is a **checkpoint**.
4. **io_uring + VDBE yield** keeps durability off the writer — and fits one thread, many tenants.

---

## Timing

| Min | Slides | Beat |
|-----|--------|------|
| 0:00–0:20 | 1 | Title |
| 0:20–3:00 | 2–3 | SQLite single writer + WAL |
| 3:00–10:00 | 4–7 | MVCC |
| 10:00–13:30 | 8–9 | Hard parts |
| 13:30–16:00 | 10–11 | io_uring + server |
| 16:00–18:30 | 12–14 | Eval placeholders |
| 18:30–19:30 | 15–16 | Limits + takeaways |

If the recording is running long at slide 7, skip the SQLite-compat bullets on slide 9 (keep conflicts only). If short, spend the extra on slide 13 (hero chart).

---

## Act 1 — The lock (slides 1–3, ~3 min)

### Slide 1 — Title (~20s)

**On slide**

- Concurrent writes in SQLite’s shadow
- Row-level MVCC + io_uring
- What that did to p99 write latency
- Name / Turso / p99 CONF

**Speak:** One sentence: we rewrote SQLite in Rust to get concurrent writers without giving up the file, then made durability async so tails stay boring.

No agenda slide. The three subtitle lines *are* the agenda.

---

### Slide 2 — Single-writer transactions (~90s)

**On slide**

```
BEGIN;          -- exclusive writer
… business logic …
COMMIT;         -- everyone else waited
```

- Second writer: `SQLITE_BUSY`
- More threads ≠ more writes
- Interactive txns hold the lock for the *compute*, not just the SQL

**Visual:** Writer A occupies the lock for 1ms SQL + 5ms app logic. B–H queued. p99 is the queue.

**Speak:** Do not dunk on SQLite — it is fast (~150k durable rows/s batched, FULL). The tax is **one writer**. This is the problem the rest of the talk solves. Cite the compute-in-txn collapse: extra cores do nothing while the lock is held.

---

### Slide 3 — WAL is not write/write concurrency (~50s)

**On slide**

- WAL = page versions in `.db-wal`
- Readers take a read mark → they do not see in-flight writer pages
- Result: **read/write yes, write/write no**

**Visual:** two columns — reader snapshot vs single writer appending frames.

**Speak:** A lot of this audience thinks WAL already solved concurrent writes. Steal the abstract: page-level versioning for readers, still one writer.

---

## Act 2 — MVCC (slides 4–7, ~7 min)

### Slide 4 — Page OCC vs row OCC (~60s)

**On slide (table)**

| | SQLite | SQLite `BEGIN CONCURRENT` branch | Turso |
|---|---|---|---|
| Writers | 1 | optimistic N | optimistic N |
| Lock | BEGIN | COMMIT | COMMIT |
| Conflict | whole DB | **page** | **row** |
| Shipped | yes | never merged | engine + Cloud preview |

**Speak:** Two txns on different rows that share a B-tree page still collide in SQLite’s branch. That is why we went to Hekaton-style MVCC (Larson et al., VLDB 2011): snapshot isolation, first-committer-wins, `PRAGMA journal_mode = mvcc` then `BEGIN CONCURRENT`. Say SI out loud — not serializable.

---

### Slide 5 — Three tiers (~75s)

**On slide**

```
BEGIN CONCURRENT
        │
        ▼
   MvStore (row version chains)     ← hot path
        │ commit
        ▼
   logical log (.db-log)            ← durability
        │ checkpoint
        ▼
   B-tree → WAL → .db               ← SQLite file
```

**Speak:** Writes during the txn never touch the B-tree, so they never take the SQLite write lock. Dual-cursor reads merge MvStore + B-tree (`core/mvcc/cursor.rs`). The `.db` is the checkpoint/recovery image, not the commit path. That split is also the Cloud sync split (slide 11).

---

### Slide 6 — Version chains (~75s)

**On slide**

```
products
  Mug     v1 [T1, T3)   v2 [T3, ∞)
  Teapot  v1 [T2, ∞)
```

- Visible if `begin ≤ snapshot < end`
- Uncommitted versions carry a TxID; commit rewrites to a timestamp
- `Preparing(end_ts)` published **under the clock lock** (SI)
- Speculative reads + commit dependencies (Hekaton §2.7)

**Speak:** T2 inserting Teapot does not wait for T3 updating Mug. This is the photograph slide — keep it visually clean.

**Code:** `RowVersion` + `MvccClock` in `core/mvcc/database/mod.rs` / `clock.rs`.

---

### Slide 7 — Commit: certify, then log (~90s)

**On slide**

1. `end_ts`, atomically `Preparing`
2. Certify write set (row + index) — walk version chains
3. Append `.db-log` (row ops, not pages) + fsync if FULL
4. Publish timestamps

- No conflict check at insert (pure optimistic)
- WW conflict → `SQLITE_BUSY` → retry from `BEGIN CONCURRENT`
- Commit I/O scales with **dirty rows**, not dirty pages
- Yield every 1024 rowids so one fat commit does not freeze the executor

**Speak:** This is the whole MVCC punchline in one slide. Logical log torn-tail = EOF, same availability idea as WAL. `DurableStorage::on_log_write_complete` is where Cloud can wait on S3 without changing certification.

---

## Act 3 — Hard parts (slides 8–9, ~3.5 min)

### Slide 8 — Checkpoint (~90s)

**On slide — crash-safe order**

1. Write versions into pager (WAL)
2. Commit pager txn **with** `persistent_tx_ts_max`
3. WAL → DB, fsync DB
4. Truncate logical log
5. **Truncate WAL last**

- Default checkpoint is **stop-the-world** (p99 cliff)
- Passive checkpoint (experimental): I/O unlocked, short publish window
- Recovery replays only `commit_ts > persistent_tx_ts_max`

**Speak:** This is *the* hard part. Get the order wrong and recovery lies. Blocking checkpoint on a busy writer set is visible as latency spikes — `perf/checkpoint-bench` exists to plot that. Four artifacts: `.db` / `.db-wal` / `.db-log` / meta row.

---

### Slide 9 — Conflicts + SQLite compatibility (~60s)

**On slide**

- Distinct rows → both commit (even same B-tree page)
- Hot row → real WW conflict, retry (ACID, not a bug)
- `BEGIN IMMEDIATE` / DDL: concurrent txns run, **cannot commit**
- One txn per connection; concurrency = many connections
- SI, not serializable (phantoms / write skew still TODO)

**Speak:** “Do I still need retry?” Yes — less of it. Queueing on `BEGIN` is gone. Compatibility is why we still speak `SQLITE_BUSY` and still checkpoint into a SQLite file. Skip the last two bullets if time is tight.

---

## Act 4 — io_uring + the server (slides 10–11, ~2.5 min)

### Slide 10 — Durability without stalling writers (~90s)

**On slide**

```rust
enum IOResult<T> { Done(T), IO(IOCompletions) }
```

- VDBE yields; `io.step()` drives the ring (not Tokio on the hot path)
- `UringIO`: 512 SQEs, SQPOLL, concurrent submit, **one leader** in `submit_and_wait`
- `MAX_WAIT = 4` — fewer `io_uring_enter` vs a bit more single-op latency (p99 knob)
- Writer is in the completion loop, **not** inside `fsync`
- Other connections keep certifying while this commit’s CQEs are in flight

**Speak:** MVCC without async I/O would still serialize the thread on durability. This is the Limbo/EdgeSys lineage grown up: this repo’s `LinuxIO` was `pread` only; production `core/io/io_uring.rs` is the same shape. Abstract sentence: *we flush durability work without stalling writers.*

---

### Slide 11 — One thread, many tenants, S3 segments (~60s)

**On slide**

- turso-server: **single-threaded** event loop, many DBs
- Yield + uring = multiplex tenants without a blocked `fsync`
- `.db` as ~128kB **segments** (generation); lazy fetch
- With tursodb: hot path syncs **logical log** frames; checkpointed `.db` segments are cold
- Blocking checkpoint is lethal on one thread → passive mode is Cloud-relevant

**Speak:** “Single-threaded” here is a tail-latency choice, not a limitation. Label Cloud p99 separately later: local FULL fsync ~2ms vs batched S3 Express PUT ~6–7ms (diskless post). **Fill this slide from turso-server before recording** (event loop, batch window, exact PUT of log vs generation).

---

## Act 5 — Evaluation placeholders (slides 12–14, ~2.5 min)

Leave chart frames. Re-run in September; do not reuse the Oct 2025 4× throughput number as the p99 CONF figure.

Harnesses: `perf/throughput`, `perf/checkpoint-bench`, `perf/latency`.

### Slide 12 — Method (~45s)

**On slide (fill)**

- Box / kernel / SQPOLL on?
- `synchronous=FULL`
- N connections, batch, keyspace, conflict rate
- `BEGIN CONCURRENT` vs SQLite `BEGIN IMMEDIATE`
- Local engine vs Cloud (S3) — pick one axis for the hero chart

---

### Slide 13 — Hero: p99 write latency (~90s)

**Chart:** p50 / p99 / p999 commit latency vs #writers.

**Series**

1. SQLite WAL, FULL
2. Turso MVCC, FULL, blocking checkpoint
3. Turso MVCC, FULL, passive checkpoint
4. Optional: io_uring vs POSIX fallback (proves slide 10)

**Callouts:** SQLite p99 grows with writer count (lock queue). MVCC stays flatter on a low-conflict keyspace. Blocking checkpoint injects spikes (time-series from `checkpoint-bench`).

This is the slide the recording should linger on.

---

### Slide 14 — Supporting charts (~60s)

**On slide — two small charts, not a third eval act**

- Left: rows/s vs threads ± in-txn compute (`perf/throughput`)
- Right: checkpoint time-series, TRUNCATE vs PASSIVE

**Speak:** Throughput is supporting evidence. The compute-in-txn plot is why MVCC exists; the checkpoint plot is why tails still spike if you stop the world.

---

## Act 6 — Close (slides 15–16, ~1 min)

### Slide 15 — Limitations (~30s)

**On slide**

- Experimental / Cloud early preview
- SI, not serializable
- Full-row versions, not deltas; version table not wait-free
- Default checkpoint still blocking
- No multi-process MVCC

**Speak:** Trust slide. p99 audience respects unfinished cliffs.

---

### Slide 16 — Takeaways (~30s)

**On slide**

1. WAL solved readers. **Row-level MVCC** is how writers stop blocking.
2. Commit is a **logical log**; the SQLite file is a checkpoint.
3. **io_uring + yield** keeps durability off the critical section — and fits a one-thread multi-tenant server.
4. Measure **p99 commit latency under concurrent load**, including checkpoint.

No Q&A card (pre-recorded). End on takeaways; cut to black.

---

## What we cut from the 30-slide draft (and where it went)

| Cut | Why | If you need it |
|-----|-----|----------------|
| Agenda | Title carries it | — |
| “SQLite is great” | Folded into slide 2 | — |
| Separate “why p99” | One line on slide 2 | — |
| Separate lifecycle / logical-log / fsync slides | Folded into 7 and 10 | Appendix A |
| Three Cloud slides | One slide (11) | Appendix B |
| Fourth eval slide | Two charts on 14 | — |
| Q&A | Pre-recorded | — |

---

## Appendix (not in the recording)

Keep these as speaker-note backups if a chart needs a follow-up cut, not as extra slides.

- **A.** Full commit state machine; log frame layout; dual-cursor merge; `MAX_WAIT` / SQPOLL.
- **B.** Recovery case table (`docs/internals/mvcc/RECOVERY_SEMANTICS.md`); S3 Express batching; `RemotePullProtocol::MvccLogical`.
- **C.** How to enable: `PRAGMA journal_mode=mvcc;` / Cloud `BEGIN CONCURRENT`.

Do not quote `docs/agent-guides/mvcc.md` on stage — it is stale (claims recovery/GC/indexes are missing).

---

## Figma Slides — design lock (blocked on MCP auth)

Figma MCP in this cloud run is `needsAuth`; interactive login only works in the Cursor desktop IDE. Build the 16-slide deck there after connecting Figma. Do not invent a second visual system — use this lock.

**File:** new Slides file, name `p99 Turso MVCC`. Sections (rows): The lock | MVCC | Hard parts | I/O + server | Eval | Close.

**Voice:** Latency talk, not a product pitch. Dark, precise, a little industrial. One idea per slide.

**Palette (`C`)**

- `bg` `#0B0C0E` — near-black graphite (title/close)
- `bgMid` `#12141A` — content slides
- `surface` `#1A1E28` — panels, SQL, chart wells
- `text` `#F4F1EA` — body (high contrast, warm paper)
- `muted` `#8B8A84` — labels, timing
- `lock` `#E8A04A` — copper: contention, p99, lock, “the tax”
- `flow` `#3ECFCF` — teal: concurrent path, io_uring, the fix
- `danger` `#E85D4A` — WW conflict / BUSY only

**Type:** IBM Plex Sans (Light titles, Regular body, Bold labels) + IBM Plex Mono for SQL/code. If Plex is missing, Geist + JetBrains Mono. Titles 56–96pt; body 22–28pt; labels 13–14pt tracking-wide. Left-align body. Never a line under a heading.

**Motif:** a cropped **p99 histogram** — five vertical copper bars of rising height, bleeding off an edge. Title/close: large, top-right crop. Content slides: small mark bottom-left as a page tick. Architecture slides reuse the bars as stacked **tiers**. Do not fade it to 5% opacity.

**Speaker notes:** copy the **Speak** bullets from each slide in this doc into `slide.speakerNotes` (markdown lists). This is a conference recording script.

### Spatial plan (no pixel math)

1. **Title** — dark `bg`. Huge Light title lower-left, three subtitle lines in muted + one copper phrase (“p99 write latency”). Histogram bleeds off top-right. Footer: p99 CONF · Turso.
2. **Single writer** — `bgMid`. SQL block in `surface` left ~60%. Right: vertical stack of writer chips, A in copper occupying height, B–H muted and queued. Caption: “p99 is the queue.”
3. **WAL** — two unequal columns. Left (wider): “readers / snapshot” in teal. Right (narrow): one writer chip + WAL frames. Big punchline bottom: read/write yes · write/write no.
4. **Page vs row** — four-row comparison, three columns. Turso column sits on a teal surface; SQLite branch is quiet. Last row “Conflict” is the only loud row (page vs **row**).
5. **Three tiers** — full-height stacked bands (MvStore / .db-log / B-tree), left labels, right one-word roles (hot / durable / file). Not a flowchart of boxes.
6. **Version chains** — open. Two horizontal version tracks (Mug, Teapot) with `[T1,T3)` capsules. Visibility rule as a single line under the diagram. Photograph slide — almost no extra copy.
7. **Certify + log** — four numbered steps as a left-to-right process (not a bullet list). One copper callout: “I/O ∝ dirty rows.”
8. **Checkpoint** — vertical 1–5 with step 5 (“Truncate WAL last”) in copper, larger. Tiny note: blocking = p99 cliff.
9. **Conflicts** — two big opposing statements, not five bullets: “Different rows → both commit” (teal, left) vs “Same row → retry” (copper, right). Three quiet compat lines under.
10. **io_uring** — `IOResult` enum as the hero in mono, left. Right rail: five short facts. Teal accent. Histogram tick only.
11. **One thread / S3** — one horizontal spine (event loop) with many small DB ticks hanging off it; two streams below (hot log / cold .db segments). Stamp: “fill from turso-server.”
12. **Method** — sparse checklist, lots of air. Dashed copper “TBD” stamp.
13. **Hero p99** — chart well dominates (~70% of canvas). Empty axes labeled writers × latency. Four series keys (SQLite / MVCC block / MVCC passive / uring). Linger slide.
14. **Supporting** — two chart wells, not equal: left throughput, right checkpoint time-series. Both stamped TBD.
15. **Limitations** — five short lines, generous leading, no cards.
16. **Takeaways** — four numbered sentences, large Regular, 1–4 in copper. Histogram returns large, bottom-right crop. No Q&A.

**Layout variety check:** title (open/dark) → diagram/queue → split columns → table → stacked bands → timeline → process → vertical steps → split statements → code hero → spine diagram → sparse list → chart → two charts → airy list → open close. Not a repeating title+bullets template.

**Preamble to paste into every `use_figma` build script:** `C` as above (RGB 0–1), `Promise.all` load Plex Sans Light/Regular/Bold + Plex Mono Regular/Bold, plus `addFrame` / `addText` / `addRect` from slide-gotchas (appendChild before x/y). Batches of 3–5 slides. Overwrite the default light theme tokens on first script. Include speaker notes.

### Build batches

1. Theme + rows + slides 1–3 (screenshot title + slide 2)
2. Slides 4–7
3. Slides 8–11
4. Slides 12–16 (screenshot 13 + 16); validate each batch

---

## Open items before September recording

1. Fill slide 11 from `turso-server` (loop, segment PUT, batch window).
2. Decide **local** vs **Cloud** for slide 13; do not mix axes on one chart.
3. Produce plots for 13–14 from `perf/throughput` + `perf/checkpoint-bench`.
4. Time a table read of slides 2→16; if over 19:30, drop slide 14’s left chart and the last two bullets of slide 9.
