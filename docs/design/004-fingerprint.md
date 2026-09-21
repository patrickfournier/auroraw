# Design note 004: the content fingerprint and relinking

> **Status: adopted (D-088).** Fourth and last design note of work package WP1
> ([M1 plan](../m1-plan.md) §5 and §6, item 5). It answers question 6 of the specification (§10):
> what is hashed so that moved and renamed files are found again, duplicates are recognised and
> imports skip what is already there, while staying fast on network shares and never confusing two
> different files? It builds on [note 001](001-workspace-layout.md) (identifiers),
> [note 002](002-state-files.md) and [note 003](003-sidecars.md) (where the values are recorded: the
> `aur:Files` entries). Items are **[decided]** since D-088.

## 1. The question

Sidecars are not next to the originals (D-022), so a sidecar is tied to its original by more than
a path (D-023). The specification asks for a **content fingerprint** for five jobs, and each needs a
different strength of proof. Hashing every whole file is affordable on a local disk and not on a
network share, and a cheap fingerprint is not proof. What exactly is computed, when, where it is
recorded, and what each job is allowed to conclude from it?

## 2. What the fingerprint is used for

| # | Job | Source | What a mistake costs |
| --- | --- | --- | --- |
| 1 | **Change detection**: did the original at this path change? ("original changed", previews refreshed on request) | D-019 | A stale preview, or an alert for nothing. Cheap. |
| 2 | **Relinking**: a moved or renamed file is found again, silently when the match is unambiguous | D-019 | A photo linked to the wrong file: its versions would render another image. Serious but visible. |
| 3 | **Exact duplicates**: the same content at several locations is **one photo with several locations** | D-036 | Two different photos merged into one and one hidden from the grid. Serious, and hard to notice. |
| 4 | **Import skips** files already imported, and reports it | Spec §5.2 | **A file on the card is not copied because it was wrongly thought to be there, and the photographer then formats the card (D-031). Data loss.** |
| 5 | **Verification of a copy** against its source | Spec §5.2 | A corrupt copy accepted. Data loss. |

Jobs 4 and 5 are about **certainty**; jobs 1 to 3 are about **finding things quickly**. That split
drives the whole design.

## 3. Constraints

- **Network shares and slow disks**: reading whole files is impossible at library scale (below).
- **A file edited elsewhere changes its content**, so no fingerprint can follow it: it needs the
  stable identifier and hints (spec §10, question 6, and note 001), and it is **never relinked
  automatically**.
- **The catalogue is rebuilt from sidecars** (D-026): whatever is recorded must be in the sidecar,
  or it is lost and recomputed at the price of reading the originals, which may be offline.
- **Nothing here may delete or overwrite a file** (D-018). A wrong conclusion can hide or mislink
  a photo; it can never destroy one, unless a decision about *copying* trusts it (job 4).
- **Cloud "files on demand"** (OneDrive, iCloud Drive, Dropbox placeholders): reading a few bytes
  of a placeholder makes the system **download the whole file**. Hashing must not trigger that.

## 4. The options

**A. Path, size and modification time only.** Free, and it is what change detection already uses
(the reconcile stat scan). It cannot find a moved file, so it cannot serve jobs 2 to 5.

**B. The whole file's hash, always.** Certain, and simple. But reading every original once is
hours on a network share (§5), and it must be repeated whenever a file is added.

**C. A sampled fingerprint only** (size plus a few chunks of the file). Fast and enough to find files,
but **it is not proof**: two files of the same size that differ only outside the sampled chunks (an
uncompressed TIFF with a spot retouched, for example) have the same fingerprint. Unsafe for
jobs 4 and 5.

**D. Two levels.** A **sampled fingerprint** computed at once, for finding (jobs 1 to 3), and a
**whole-file hash** computed when it is free (at import) or in the background (other files), and
**required for every decision that can lose data** (jobs 4 and 5).

## 5. Measurements

`blake3` 1.x, one thread, real RAW files from the samples (15 to 62 MB), local NVMe disk.

| | Warm cache | Cold cache |
| --- | --- | --- |
| Whole-file hash | 2.5 to 3.2 GB/s (6 to 20 ms per file) | **1.2 to 1.4 GB/s** (13 to 49 ms per file), limited by the disk |
| Sampled fingerprint (three chunks of 64 KB) | 0.1 ms | **0.7 to 1.4 ms** |

Projected to a library of **100,000 RAW files of 30 MB (3 TB)**:

| | Sampled fingerprint | Whole-file hash |
| --- | --- | --- |
| Local NVMe (1.2 GB/s) | **about 70 seconds** | about 42 minutes |
| SATA SSD (about 0.5 GB/s) | about 2 minutes | about 100 minutes |
| Gigabit network share (about 0.11 GB/s) | a few minutes (three small reads per file) | **about 7.5 hours** |

Hashing is not the bottleneck (BLAKE3 outruns the disk); reading is. That is why a whole-file hash
of an existing library must be a **background, throttled, cancellable** job, and why relinking
cannot depend on it.

## 6. The proposal

### 6.1 Three values per file [proposed]

| Value | Definition | When it is known |
| --- | --- | --- |
| **Size** | The file's size in bytes. | Always. |
| **Fingerprint** | `sampled-v1:<hex>`: **BLAKE3** over the text `auroraw-sampled-v1`, the size as 8 bytes little-endian, then **64 KB from the start, 64 KB from the middle (at the offset `size/2` rounded down to a multiple of 4,096), and 64 KB from the end**. A file of 192 KB or less is read whole. | At the first sight of the file: added in place, imported, or scanned. |
| **Hash** | `blake3:<hex>`: **BLAKE3 of the whole file**, 256 bits. | At import (the copy is read anyway); otherwise **later, in the background** (§6.6). Absent until then. |

Both are lowercase hexadecimal, 64 characters, tagged with their algorithm so that a future
version can change it. **BLAKE3** is fast enough to disappear behind the disk, parallel and
implemented in Rust with SIMD on all three platforms; it is not used for anything that needs
resistance to an attacker (there is no signature).

### 6.2 Where they are recorded [proposed; amends note 003 §4.3]

In the photo sidecar, in each `aur:Files` entry: **`Size`, `Fingerprint` and, when known, `Hash`**,
next to `Name`, `Role`, `Format` and `Locations` (note 003 listed `Size` and `Hash`; the fingerprint
is added). The photo sidecar is written with the fingerprint when the photo is added; **when the
whole-file hash is computed later, the sidecar is rewritten once**, in a batch, at low priority
(about 0.7 ms per file on NTFS, so 100,000 files in about a minute and a half). The catalogue keeps
an index on (`Size`, `Fingerprint`) and on `Hash` for lookups (WP2). The size and modification time of
each file **as last seen** are local to the machine, in the catalogue, not in the workspace.

### 6.3 What each job may conclude [proposed]

1. **Change detection** uses **size and modification time**, as the reconcile scan does. When they
   differ from the last time, the **fingerprint is recomputed**: if it is unchanged the file was only
   *touched* (a sync tool, a copy, a share with coarse timestamps) and nothing is reported; if it
   differs the photo is marked **"original changed"** (§6.5).
2. **Relinking** (§6.4) rests on **size and fingerprint**: enough to find a file and to link it
   silently when the match is unique, never enough to conclude anything destructive.
3. **Duplicates** rest on **size, fingerprint and capture data** (time and camera read from the two
   files must agree). The two files then become **one photo with two locations**, immediately, and
   the **whole-file hash is confirmed in the background** when both locations are online. If the
   hashes differ, the merge is **undone**: the second file becomes **its own photo** (a new
   identifier, a new sidecar), the report says so, and nothing is lost, since nothing was deleted.
4. **Import skips a file only when its whole-file hash equals the recorded hash of a photo already
   in the catalogue**, and the report says "already imported (verified identical)". The card's file is
   hashed whole (it is read for the copy anyway, so the decision costs nothing extra); if the
   existing photo has only a fingerprint, its whole-file hash is computed first, from the imported
   copy, which is local. **A fingerprint alone never causes a file to be skipped.** This is the rule
   that protects the card (D-031).
5. **Verification of a copy** compares the **whole-file hash of the source, computed while reading it,**
   with the **whole-file hash of the destination read back after a flush**, asking the system to bypass its
   cache where it can (a re-read from the cache would prove nothing about the disk). Details in WP7.

### 6.4 Relinking, step by step [proposed]

When a scan finds a file at a path the catalogue does not know, it looks it up by **size and
fingerprint**, then decides:

| The new file matches | The existing photo's known location | Action |
| --- | --- | --- |
| **Exactly one** file record | **Gone**: its source is **online** and the file is not there | **Relink**: the location moves. Silent (D-019). Counted in a report ("42 files relinked"). |
| Exactly one file record | **Still there**, or its source is **offline** | **Add a location**: the same photo at two places (D-036). An offline source is never treated as "file gone": when it returns, both locations exist. |
| **Several** records (identical files that are separate photos) | | **Ambiguous**: put to the photographer (D-019). |
| **None** | | A **new file**: offered for adding (D-019). Then the **hints** below are tried. |

**Hints for a file edited elsewhere.** A photo whose location is gone and that matched nothing may
correspond to a new file that has the **same capture time and camera**, a **similar name** and
a size in the same range: the file was edited and saved back under another path. This is
**proposed to the photographer** ("Is this the edited version of Marie-042.ARW?") and **never applied
automatically**; accepting it keeps the versions and takes the new file as the original changed.

### 6.5 "Original changed" [proposed]

A file at a known path whose fingerprint no longer matches (§6.3, item 1) is marked **original
changed** in the catalogue (not in the workspace: it is a state to be resolved, recomputed at each
reconcile). The photo **keeps its versions and its sidecars**. Nothing else happens until the
photographer chooses:

- **Accept**: the sidecar's `Size`, `Fingerprint`, `Hash` and the cache of capture data are
  replaced, the previews are regenerated, and the versions render the new pixels (a development that
  no longer suits the new file is the photographer's to adjust);
- **Ignore**: the old values stay, and the mark stays too.

### 6.6 The background job for the whole-file hash [proposed]

- It computes **`Hash` for files that have only a fingerprint**, when a source is **online and idle**,
  **throttled**, on **one file at a time on a network share** and on a few threads on a local disk,
  **cancellable**, resumable at start-up, and **lowest priority** (behind everything the
  photographer is doing).
- It **never runs on files that are cloud placeholders** (§7), and never wakes a sleeping drive on
  its own schedule.
- It is also **run on demand for a duplicate to confirm** (§6.3, item 3) and before an import skip.
- A "**Verify originals**" command in the backup helper (M4) uses the recorded `Hash` to detect
  corruption: it reads the files, compares, and reports; it changes nothing.
- Progress is visible and can be paused. On battery it pauses by default.

## 7. Risks and how they are handled

| Risk | Handling |
| --- | --- |
| **Sampled false match** (same size, same three chunks, different content) | Cannot happen between different photos in practice, since the header and pixel data differ. It can happen for a **file retouched in an unsampled region without a change of size**. Consequence: at worst a wrongly merged duplicate or relink, caught by the background hash (§6.3, item 3) and visible as "original changed" at the next size/time change; **never a skipped import or a deleted file**. A test documents the case (§8). |
| **Cloud placeholders** | A placeholder is recognised by the system's attribute (Windows recall-on-data-access, macOS dataless file, and the equivalents of Dropbox and others) and **never read** for a fingerprint or a hash unless the photographer allows it for that source. Its size, name and time are used, and the photo is treated as **not yet available** rather than as changed or missing. |
| **Coarse or unreliable timestamps** (FAT, some shares, sync tools touching files) | A changed time with an unchanged fingerprint is only a touch (§6.3, item 1). |
| **Two identical files that are two photos** (a copy imported twice before this rule existed, or imported from two cards) | The "several records" row of §6.4 asks. Import does not create the second one, since it skips on the hash. |
| **Two identical files in one scan** (a copy in the same folder) | The first becomes the photo, the second an additional location. |
| **Reading cost of the first scan** | Three small reads per file, piggybacked on the read already made for the metadata and the embedded preview where the decoder allows. |
| **Changing the algorithm** | The tag (`sampled-v1`, `blake3`) says which one a value is; a later version recomputes lazily and keeps both while migrating. |

## 8. Consequences for WP1, WP4 and WP7

- **WP1 (`format`, `workspace`)**: the two encodings and their exact definition, with **known-answer tests** (a
  fixed byte sequence gives a fixed fingerprint and hash, identical on all three platforms and both
  byte orders); files of 0, 1, 192 KB, 192 KB + 1 and larger sizes; the sidecar fields.
- **A test that documents the limit**: two generated files of the same size that differ by one byte
  outside the sampled chunks have the **same fingerprint and different hashes**, and the
  duplicate merge is undone by the background confirmation.
- **WP4 (sources)**: the scan and the relinking table (§6.4) with fake sources: a file moved, renamed, moved between
  sources, present twice, its old source offline, ambiguous copies, a touched file, a changed
  file, a placeholder that is never read; **relinking is silent only in the unique case**.
- **WP7 (import)**: **a fingerprint match without a hash match copies the file** (the test that protects
  the card); an interrupted import and its resume; the verification reads back after a flush.
- The **benchmark** of §5 becomes a small tool, and the projections of §5 are repeated on Windows and on a network share when one is available.

## 9. Not settled here

| Item | Where |
| --- | --- |
| The **visual similarity** of photos that are not the same file (a perceptual hash of the preview) | WP5 and WP9, a different mechanism |
| The exact system calls to bypass the cache when re-reading a copy, per platform | WP7 |
| How placeholders are detected for each cloud provider | WP4, starting with the two system mechanisms |
| The interface of the ambiguous cases and of the hints | WP4 and WP8 |
| Whether the hash of a **pair's** two files is also combined into one identity for the pair | Not needed: each file has its own record; the pair is the photo |

## 10. What is asked

Approval of: the three values and their exact definitions (§6.1); their place in the sidecar, which
amends note 003 (§6.2); **the rule that data-losing decisions (import skips, copy verification) rest on
the whole-file hash and never on the fingerprint** (§6.3, items 4 and 5); the relinking table and the
hints (§6.4); "original changed" (§6.5); the background job (§6.6); and the handling of the risks
(§7), as **D-088**. The point most worth a second look is §6.3, item 3: **duplicates are merged at
once on the fingerprint and confirmed later by the hash**, with an undo if they differ, which is
friendlier than showing two entries for hours after adding a backup folder, and a little riskier.
