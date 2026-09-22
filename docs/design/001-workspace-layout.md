# Design note 001: the layout of a workspace

> **Status: adopted (D-085).** First design note of work package WP1
> ([M1 plan](../m1-plan.md) §5 and §6, items 2 and part of 5). It answers question 2 of the
> specification (§10): how is a workspace organised on disk, what are the files called, and where
> does a workspace go by default? It does **not** settle the formats of the files themselves
> (notes 002 to 004), only where they live and how they are named and written. Statuses are
> **[decided]** since D-085; the approval is decision D-085.

## 1. The question

A workspace (D-022, D-023, D-025) is the folder that holds the truth about a catalogue: the
photo sidecars, the version sidecars, the state files, and optionally exports. What is the shape
of that folder, so that it is correct, quick to scan (the reconcile of architecture §5.3 and the
rebuild of §5.4 walk all of it), safe to write, easy to back up and synchronise, and works on
NTFS, APFS, ext4 and network shares?

## 2. What the layout must satisfy

These come from decisions already made, not from taste.

| # | Requirement | From |
| --- | --- | --- |
| 1 | A sidecar is tied to its photo by an identifier and a content fingerprint, **not by the original's path** (sidecars are not next to the originals; files move and rename; one photo can have several locations). So the layout cannot depend on the source's tree. | D-023, D-036, spec §5.1 |
| 2 | One sidecar per photo and one per version; **a version sidecar is self-contained**, so that moving photos between catalogues copies files and nothing more. | D-023, D-067 |
| 3 | The workspace is **backed up by copying a folder**, and carried between machines by **ordinary file-sync tools**. Two machines can create files at the same time, so names must not collide. | D-064 |
| 4 | The **whole workspace is walked** at start-up (a stat per file, 28 ms for 243,000 files measured) and read in full for a rebuild (3.2 s warm, 10.1 s cold). The layout must not make that slower. | D-026, spike 3 |
| 5 | **Metadata edits are written immediately** and must not corrupt a file if the machine dies mid-write. | D-022, architecture §5.3 |
| 6 | It works from **100 to about a million photos**, on **NTFS, APFS, ext4 and network shares**, without a directory too large to list. | spec §9, architecture §12 |
| 7 | Files from a **future Auroraw**, or **stray files** (a sync tool's conflict copy, a note), are neither damaged nor deleted. | architecture §5.5 |
| 8 | **Nothing is ever deleted** by Auroraw; sidecars of photos moved out of a catalogue go to a recoverable folder. | D-067 |

## 3. The options

**A. Mirror the source tree.** The workspace copies the folders of the sources, with a sidecar
beside each original's would-be position. Readable at first sight, but it breaks requirement 1:
a moved or renamed original, a photo in two locations, two sources with the same folder names,
or an offline source, all make the mirror wrong or ambiguous. **Rejected.**

**B. Flat.** Two folders, `photos/` and `versions/`, every file inside. Simple.

**C. Sharded by the photo identifier.** The same two folders, each divided by the first
characters of a random identifier, so no folder holds more than a few thousand files. The
number of shards is the parameter: 256 (two hexadecimal characters) or 4,096 (three).

**D. One folder per photo.** `photos/<shard>/<photo id>/` holding the photo sidecar and its
versions. Self-contained per photo, and copying a photo is copying a folder.

## 4. Measurements

`tools/layout-bench.rs` writes 100,000 photo sidecars (1.6 KB) and about 100,000 version
sidecars (3.3 KB), 199,600 files in all, with 16 threads and the spike 3 sizes, then walks
the tree with metadata, reads every file, and rewrites single files through a temporary file and
a rename. Times are in milliseconds unless stated; the caches are warm.

**Linux, the developer machine** (ext4 on a SATA SSD, 16 threads):

| Layout | Folders | Create the folders | Write all | Walk | Read all | One rewrite |
| --- | --- | --- | --- | --- | --- | --- |
| Flat | 2 | 33 | 1,200 to 1,600 | 290 | 105 | 0.036 |
| Shard 256 | 512 | 210 | 340 to 1,400 | 290 to 320 | 105 to 112 | 0.035 |
| Shard 4096 | 8,192 | 360 | 1,400 | 335 | 143 | 0.034 |
| One folder per photo | 100,000 | 1,860 | 1,340 | **810** | 112 | 0.035 |

**Continuous integration runners** (shared machines, two passes, range of the two; useful for
the ratios between layouts, not for absolute figures):

| | Layout | Create the folders | Write all | Walk | Read all | One rewrite |
| --- | --- | --- | --- | --- | --- | --- |
| **Linux** | Flat | 35 | 3,300 to 6,900 | 520 | 4,900 to 5,500 | 0.08 |
| | Shard 256 | 180 to 205 | 2,000 to 7,400 | 520 | 3,900 to 4,500 | 0.07 to 0.08 |
| | Shard 4096 | 570 to 630 | 3,100 to 3,400 | 600 | 3,900 to 4,200 | 0.07 to 0.08 |
| | One folder per photo | 4,100 to 5,400 | 8,200 to 8,400 | **2,100** | 2,900 to 3,000 | 0.07 |
| **macOS (APFS)** | Flat | 34 | 25,100 to 26,700 | 5,900 to 10,000 | 21,900 to 27,800 | 0.5 |
| | Shard 256 | 190 | 18,600 to 21,200 | 5,500 to 6,200 | 21,100 to 21,600 | 0.5 |
| | Shard 4096 | 400 to 415 | 16,400 to 17,100 | 6,200 to 6,300 | 22,100 to 24,700 | 0.5 |
| | One folder per photo | 2,200 | 14,300 to 17,100 | **30,000 to 35,700** | **39,600 to 51,600** | 0.5 to 0.9 |

**Windows (NTFS, a runner with Windows Defender active)**, one pass, 16 threads. The write is the
creation of 40,000 files (20,000 photos) or 199,600 files (100,000 photos); the other columns are as above.

| Photos | Layout | Folders | Create the folders | Write all | Walk | Read all | One rewrite |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 20,000 | Flat | 2 | 47 | 26,200 | 52 | 630 | 0.65 |
| | Shard 256 | 512 | 380 | 58,300 | 89 | 800 | 0.75 |
| | Shard 4096 | 8,014 | 8,450 | 89,300 | 580 | 540 | 0.69 |
| | One folder per photo | 20,000 | 8,200 | 45,700 | **1,740** | 830 | 0.75 |
| 100,000 | Flat | 2 | 260 | 202,000 | 260 | 4,600 | 0.76 |
| | Shard 256 | 512 | 970 | 153,300 | 2,460 | 3,500 | 0.63 |
| | Shard 4096 and one folder per photo | | *cancelled, see §4.1* | | | | |

What they say:

1. **The number of folders barely matters for writing and reading**, from 2 to 8,192, on ext4,
   and the two sharded layouts are as good as flat or better on APFS (writing 20 to 35 % faster
   than flat).
2. **One folder per photo is the clear loser** where it counts for start-up: walking the tree is
   **3 to 6 times slower** (2.8x on ext4, 4x on the Linux runner, 5 to 6x on APFS, and about 20x
   on NTFS at 20,000 photos), creating the
   100,000 folders costs 2 to 5 seconds on their own, and reading is up to twice as slow on APFS.
3. **256 and 4,096 shards are equivalent.** 256 makes about 400 files per folder at 100,000
   photos, about 4,000 at a million: comfortable on every file system, and half as many
   folders to create.
4. **Rewriting one sidecar** (temporary file, then rename) costs 0.03 to 0.08 ms on Linux, about
   0.5 ms on the APFS runner and **0.6 to 0.8 ms on NTFS**, whatever the layout.
5. **On NTFS, creating files is slow**: about 0.75 to 1 ms per file, so 200,000 files take 150 to
   200 seconds where Linux takes about 1.4 seconds. This is the cost of scanning every new file
   (Windows Defender), and **it does not depend on the layout enough to choose one**: at
   100,000 photos 256 shards were faster than flat (153 s against 202 s), at 20,000 they were
   slower (58 s against 26 s), and creating a folder costs about 1 ms, so 8,000 folders cost 8 s.
   **Reading is not affected** (200,000 files in 3.5 to 4.6 s).
6. **A flat folder of 200,000 files works on ext4 and APFS in these tests**, but it is the
   layout that file managers, sync tools and network shares handle worst (a listing that
   returns 200,000 names at once, a synchronisation that re-scans one giant folder). The
   measurements do not show a benefit that would justify that risk, and sharding costs almost
   nothing.
7. **Absolute figures on the APFS and NTFS runners are slow** (reading 200,000 files takes 21 s): a shared,
   throttled machine. The rebuild budget of "seconds" is judged on Patrick's machines, not on CI.

### 4.1 What NTFS changes

- **One folder per photo is still the worst on NTFS**: the walk takes 1.7 s at 20,000 photos
  against 0.05 to 0.09 s for the others, and creating the folders alone takes 8 s.
- **Sharding is not free on NTFS**: walking 100,000 photos through 512 folders takes 2.5 s, against 0.26 s
  for two folders. That is still small for a start-up scan (the budget is a few seconds and the scan
  runs in the background), but it is a tenfold cost that the flat layout would avoid. **The
  folders are therefore created when they are first needed, not all at once** (creating 512 costs
  about 1 s on NTFS, and a small catalogue never needs them all).
- **Bulk writing is the real Windows cost, and it is the same in every layout.** An import of
  2,000 photos writes 2,000 sidecars, about 2 s. A batch edit of 10,000 photos rewrites
  10,000 sidecars (D-027: the version sidecars follow), about **7 s on NTFS** against 0.4 s on Linux.
  That is fine as a background job with progress, and it means **a batch edit must not
  block the interface** and the sidecar rewrites should run on several threads (they scale). The
  first full write of a large workspace, made only when converting or restoring, takes minutes.
- **The measurements do not favour flat enough to give up sharding.** The gain is one scan of
  about 2 s at 100,000 photos; the loss would be a folder of 200,000 files that Explorer, cloud
  synchronisation and network shares handle badly. 256 shards stay the proposal.
- Two jobs (4,096 shards and one folder per photo, at 100,000 photos) were cancelled after more
  than an hour without finishing, which says something in itself about the creation of 8,000 to
  100,000 folders on NTFS; neither layout is proposed, so they cannot change the choice. The measurement is
  repeated on Patrick's Windows machine, with real sidecars, at the end of WP1.

### 4.2 What the real code measured (WP1)

The first `workspace` and `format` crates, with real sidecars (about 3 KB each), were run on **100,000
photos and 125,074 versions (225,074 files)** by `crates/workspace/tests/large.rs`, nightly on the
three platforms. Sixteen threads on the developer machine (warm), four on the runners.

| | Developer machine (Linux, 16 threads) | CI Linux (4 vCPU) | CI macOS (3 vCPU) | CI Windows (4 vCPU, Defender on) | Patrick's Windows machine (16 threads, HDD system disk) |
| --- | --- | --- | --- | --- | --- |
| Write every sidecar (atomic) | 4.1 s | 7.6 to 10 s | 47 to 59 s | 500 to 700 s | **1,513 s (25 minutes)** |
| Walk the whole tree | 0.43 s | 0.5 to 0.8 s | 10 to 17 s | 0.5 to 0.6 s | 2.9 s |
| Read and parse every sidecar | **1.9 s** (119,000 files a second) | 11 to 18 s (12,000 to 20,000 a second) | 43 s (5,200 a second) | 9.6 to 14 s (16,000 to 23,000 a second) | 54.3 s (4,150 a second) |

One atomic write, step by step, one thread (milliseconds per file): Linux 0.02; macOS 0.16;
**Windows about 1.0** (writing the temporary file 0.29, the rename **0.55**, checking that the shard
folder exists 0.15, which is no longer done: the folder is created only when the rename finds it
missing). Under four threads each step is 2 to 5 times slower on the runners.

What it says:

- **The parse is not the bottleneck**: requirement 9 of note 003 (tens of thousands of files a second on all cores) holds
  everywhere the file system lets it. The rebuild's reading is limited by the disk, as note 004 found for hashing.
- **A rebuild of 100,000 photos reads 225,000 files in 2 s on a fast machine and 10 to 45 s on the slowest
  runners.** Seconds to tens of seconds, as the plan expected.
- **Writing is where Windows hurts**, and it is the rename and the antivirus scan of every new file, in every layout
  (note 001 §4.1). About 1 to 2 ms per file on the CI runner (an SSD), so a batch that rewrites 10,000 photo
  sidecars and their version copies takes 20 to 45 seconds there, and must be a background job with progress
  (note 003 §5.3).
- **On a real Windows machine with a spinning hard disk, it is far worse** (issue #1, 2026-09-22): Patrick's
  system disk is a 2 TB Seagate ST2000DM008 (a 7200 rpm HDD, also carrying the page file, the worst realistic
  case). Writing 225,074 files there took **1,513 s, about 6.7 ms per file, 25 minutes for the whole 100,000-photo
  workspace** — 2 to 3 times slower than the CI Windows runner's SSD, and about 375 times slower than the
  developer's NVMe. Reading was 4,150 files a second, 4 to 5 times slower than the CI Windows runner and 28
  times slower than the developer machine, but **the walk stayed fast (2.9 s)**: metadata reads hit the file
  table, not the platters, on every system tried.
- **This changes what "a rebuild takes seconds" means.** On the fast machines it holds; on a spinning system
  disk, reading and parsing 225,000 sidecars alone takes about a minute, and writing a first workspace of that
  size (an initial import, or converting an existing library) takes tens of minutes, not seconds. The design
  itself does not change (a rebuild is still the only way to recover, and it is still correct, D-026): the
  consequence is for the **interface**, which must show progress and let a rebuild run in the background rather
  than imply "a few seconds" unconditionally, and possibly warn when the disk under a workspace looks like a
  spinning drive. Left open for WP1 or WP8 to decide.
- These CI runners are shared and scanned; their figures stay useful for the ratios between file systems, not
  as absolute numbers, now that a real machine's are in hand.

## 5. The proposal

### 5.1 The tree [proposed]

```
<workspace>/
  workspace.json            the marker: format, layout version, workspace id, catalogue name
  README.txt                two lines saying what this folder is and not to edit it by hand
  photos/<xx>/<photo id>.xmp
  versions/<xx>/<photo id>.<version id>.xmp
  state/                    the state files (note 002)
  exports/                  optional: where exports go by default (never scanned)
  removed/                  recoverable sidecars of photos moved out (D-067)
  .auroraw/
    tmp/                    temporary files of atomic writes
    lock                    the writer's lock
```

`<xx>` is the first two characters of the photo identifier: **256 shards, layout C**, the
version sidecar being in the shard of its photo. **A shard folder is created the first time a
file needs it** (§4.1), never all at once. The layout is layout version **1**, written in
`workspace.json`.

### 5.2 Identifiers and file names [proposed]

- The **photo identifier** is **128 random bits**, written as **32 lowercase hexadecimal
  characters**, generated when the photo is first added to any catalogue, immutable, and written
  into the photo sidecar (architecture §5.2). Random rather than time-ordered, so that the shards
  fill evenly; 128 bits so that two workspaces merged later (moving photos between catalogues,
  or two machines) never collide in practice.
- The **version identifier** is **64 random bits**, 16 lowercase hexadecimal characters, generated
  when the version is created. Random rather than a counter, so that two machines creating a
  version of the same photo at the same time (requirement 3) do not produce the same file name.
- **File names contain the identifier only**: `3f2a…9c.xmp` and `3f2a…9c.7be1…04.xmp`.
  The **original's name is not in the file name**; it is in the sidecar (D-074).

**This changes the proposal of the M1 plan** (§6 item 2), which suggested `<original name>~<short id>.xmp`.
The reasons, from working it through:

1. A file name derived from the original's name inherits every hazard of file names: Windows
   reserved names (`CON`, `NUL`) and forbidden characters, the 255-character limit, trailing dots and
   spaces, the difference between composed and decomposed accents that macOS introduces, case-
   insensitive collisions. Lowercase hexadecimal has none of these.
2. The name goes stale when the original is renamed, and two originals can share a name. An
   identifier does not.
3. It does not help the person browsing: with 256 shards named by hash, nobody browses by hand.
   The catalogue is the index, and a search for the original's name works because it is inside the
   sidecar.
4. Path lengths become predictable: `photos/3f/` plus 36 characters, and `versions/3f/` plus 53.

### 5.3 The marker, `workspace.json` [proposed]

A small JSON file with the format name, the **layout version**, a **workspace identifier** (128 random bits,
hexadecimal), the catalogue's name, the creation date and the Auroraw version that made it. It
allows to:

- check that a catalogue database belongs to this workspace (the database stores the identifier;
  a mismatch offers a rebuild instead of trusting stale data);
- recognise a folder as a workspace when the user chooses "Open a workspace" (moving to another
  machine, restoring a backup, D-064);
- refuse cleanly a workspace whose layout version is newer than the application.

Unknown keys are preserved when the file is rewritten (architecture §5.5).

### 5.4 Writing a file safely [proposed]

1. Write the content to a **new file in `.auroraw/tmp/`**, on the same volume as the target, so the
   final step is a rename.
2. Flush according to the sync policy that architecture §5.3 leaves open (no forced sync for an
   interactive metadata edit; a sync at the end of a batch and when the application closes).
3. **Rename over the target.** POSIX makes this atomic. On Windows `rename` replaces the target
   with `MoveFileEx`, which **fails while another program has the target open without allowing
   deletion**, which antivirus scanners and search indexers do. The write **retries with a short
   backoff** (up to about 200 ms) before it reports an error, and a test covers it on Windows.
4. **At start-up**, with the lock held, everything in `.auroraw/tmp/` is a leftover of a crash
   and is removed. Nothing else is ever deleted.

### 5.5 One writer: the lock [proposed]

The application holds an **advisory lock** on `.auroraw/lock` (the standard library's
`File::try_lock`) for as long as the workspace is open for writing. A second instance, or a second
program, gets a clear message and opens the workspace read-only. On a **network share**, locks are
unreliable; the lock is then best-effort, and the reconcile at start-up remains what repairs a
race. Two machines writing the same workspace through a sync tool at once are **not supported**;
the layout at least makes it non-destructive (random names, one file per photo).

### 5.6 What the scan does with files it does not know [proposed]

The scan (reconcile and rebuild) reads only files whose **path matches the two patterns** of §5.1,
in `photos/` and `versions/`. Anything else there (a sync tool's "conflicted copy", a stray file, a
newer format's extra file) is **left alone and listed once** in a "workspace notes" report, never
modified or removed. A sidecar whose content is invalid is reported the same way and **not
overwritten** until the user decides (testing strategy §8).

### 5.7 Where a workspace goes by default, and the local data [proposed]

- **The default workspace of the first launch** is `<Pictures>/Auroraw/<catalogue name>`, created
  silently (D-017), where the default catalogue name is **a translated string** ("Main" in
  English, "Principal" in French). `<Pictures>` is the operating system's Pictures folder, which
  the system already localises (Known Folders on Windows, `XDG_PICTURES_DIR` on Linux,
  `~/Pictures` on macOS: "Images" on a French system); `Auroraw` is the product name and is not
  translated. The workspace is **the thing to back up**,
  so it lives where users already back up their photos, visible, and not in a hidden application-data
  folder that a clean-up or a profile reset could remove.
- **Another catalogue** asks for a name and a place, offering `<Pictures>/Auroraw/<name>`. The
  folder name is the catalogue's name, normalised (composed accents) and cleaned of the
  characters and names a file system refuses, made unique. It may contain any script.
- **Localising the name is safe because it happens once.** The name is fixed when the catalogue
  is created, in the interface language of that moment, and is stored in `workspace.json` and the
  registry; changing the language later does not rename anything, and a workspace is found again
  by its marker, never by its folder name. The **names inside the workspace** (`photos`,
  `versions`, `state`, `exports`, `removed`, `.auroraw`) are part of the format and are **never
  translated**: they must be identical on every machine and in every language, or a workspace
  moved between machines could not be read. The cost of localising the root name is only that
  support instructions cannot say "the Main folder": they say "the workspace folder".
- **The catalogue database** is local (D-026): `<data dir>/catalogues/<workspace id>/catalogue.db`;
  **the previews database** is in the cache directory: `<cache dir>/catalogues/<workspace id>/previews.db`
  (D-075). Both are deletable and rebuilt.
- **A small registry** in the data directory lists the catalogues (workspace identifier, name,
  path). It is a convenience: lost, it is rebuilt by opening the workspace folder.
- A workspace can be **on another disk or a network share** (D-022) and is then reached through
  the registry's path; if it is unreachable the catalogue opens read-only from the local database
  (a separate question, plan §6 item 10).

### 5.8 Changing the layout later [proposed]

The layout version is in the marker. A **new layout version is a migration of the whole workspace**,
expensive and to be avoided: that is why the choices above favour the boring ones (fixed 256
shards, identifier-only names, room in the marker for new keys). If a million-photo catalogue ever
needs more shards, the marker can say so, and readers find the shard count in it instead of
assuming 256 [proposed].

## 6. Consequences for WP1

- The `workspace` crate implements exactly this: creating and opening a workspace, the marker, the
  paths for a photo and a version, atomic writes with the retry, the lock, the scan with foreign
  files reported, the removal of leftovers.
- Tests: **the Windows retry** on a target opened by another process; a crash between the temporary
  file and the rename; foreign files and sync-conflict names in the scan; identifiers that collide
  in their first two characters; paths at the maximum length on Windows; a workspace with a newer
  layout version is refused.
- The **benchmark** (`tools/layout-bench.rs`) stays as a tool, and is rerun on the real Windows
  machine at the end of WP1 with real sidecars (WP1's "done when" in the plan).
- Notes 002 (state files), 003 (the sidecar's content and the version sidecar) and 004 (the
  fingerprint) follow.

## 7. Not settled here

| Item | Where |
| --- | --- |
| Formats of the state files, and how they avoid conflicts under file synchronisation | Note 002 |
| The content of the photo and version sidecars | Note 003 |
| The content fingerprint and the relinking of moved files | Note 004 |
| Whether a network workspace opens read-only when unreachable, or queues edits | Plan §6, item 10 (read-only proposed) |
| The sync policy of interactive writes (forced sync or not) | WP1, from a measurement on the three systems |
| A README's wording and its translation | WP1 (English only, a fixed text) |

## 8. What is asked

Approval of the tree and names of §5.1 and §5.2, the marker (§5.3), the write, lock and scan
rules (§5.4 to §5.6), and the default place (§5.7), as **D-085**. The change from
`<name>~<id>.xmp` to identifier-only names (§5.2) is the point most worth a second look.
