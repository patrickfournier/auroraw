# Design note 002: the state files

> **Status: adopted (D-086).** Second design note of work package WP1
> ([M1 plan](../m1-plan.md) §5 and §6, item 3). It answers question 3 of the specification
> (§10): what format do the **state files** have, the files of the workspace that hold what would
> otherwise live only in the database (collections, smart collections, series, the keyword
> vocabulary, the list of sources, and later the publications)? It builds on
> [note 001](001-workspace-layout.md) (where files live and how they are written). It does not
> settle the content of the photo and version sidecars (note 003) or the fingerprint (note 004).
> Items are **[decided]** since D-086.

## 1. The question

The catalogue must be rebuildable from the workspace alone (D-026), and the workspace must be
backed up by copying a folder and carried between machines by ordinary sync tools (D-064). So
every piece of state that is not in a sidecar must be written in the workspace, in a format that
is documented, tolerant of change, and safe to write. Which files, with what content, in what
format?

## 2. What has to be stored

| State | Typical size | Changes | Milestone | Where it could live |
| --- | --- | --- | --- | --- |
| **Keyword vocabulary**: hierarchy, synonyms, "do not export" flag | 4,400 keywords: 460 KB compact, 690 KB indented | While the vocabulary is built; rarely after | M1 | State file |
| **Sources**: the list, their kind, name, settings, ignored patterns | A few entries | Rarely | M1 | State file |
| **Manual collections**: an ordered list of photos (order matters, for galleries) | 20 to 2,000 photos; a few larger | With every add or reorder | M1 | State file |
| **Smart collections**: a saved query | Tiny | Rarely | M1 (WP11) | State file |
| **Series**: members, cover, resolved or not, how it was made | 2,700 per 100,000 photos, about 400 bytes each | At import, and when culling | M1 | State file, or the photo sidecar |
| **Publications**: gallery records, published names, revisions, client-selection collections | Per gallery | At each publication | M4 | State file (reserved now) |
| **Annotations** from clients (text, drawing, audio) | Can be large | On fetching feedback | M4 | Beside the photo (decided in M4) |
| Ratings, flags, labels, keywords **of a photo**, captions, IPTC | | | M1 | **The photo sidecar** (note 003), not here |

## 3. Requirements

1. **Complete for a rebuild**, with **no secret**: nothing that must not be shared is written
   (tokens and keys stay in the keychain, D-061 and spec §5.9).
2. **Open and documented**, readable by a person and by other tools (spec §5.8).
3. **Tolerant of the future**: a newer Auroraw's file is neither damaged nor thrown away by an
   older one; an unknown file is left alone (architecture §5.5, note 001 §5.6).
4. **Safe to write** by the atomic write of note 001 §5.4, so a crash leaves the old or the new
   file, never a mixture.
5. **Friendly to synchronisation**: sync tools copy files; two machines used one after the other
   (D-064) must not create needless conflicts, and a conflict that does happen must touch as little
   as possible.
6. **Machine-independent**: a workspace carried to another machine works, so nothing that is
   specific to one machine (a mount point, a drive letter, a window layout) is in it.
7. **Cheap** at 100,000 photos: reading all the state at rebuild, and rewriting one file when
   something changes, stay in milliseconds.

## 4. The options

**A. One file per kind**, as in spike 3 (`vocabulary.json`, `collections.json`, `series.json`).
Few files, but each is a single point of conflict: two machines that each add a photo to a
different collection change the same file; the whole file is rewritten for any change; the series
file grows with the catalogue (about 1.1 MB at 100,000 photos).

**B. One file per entity** (one per collection, one per series), the vocabulary and the sources
staying single files because they are small and edited as a whole. A change touches one file,
a sync conflict concerns one collection, and every file is small.

**C. State inside the photo sidecars** (a photo lists its series, its collections). No state files
for those, and a rebuild reads only sidecars. But a collection's **order** and a series' **cover**
and **resolution** have no natural home in one photo, adding a photo to a collection would rewrite
its sidecar (and the sidecars of its versions follow, D-027), and finding "all members of the
collection" needs a full scan. Rejected for collections; kept in mind for series (below).

**D. A single database file in the workspace.** A binary file that sync tools cannot merge, not
readable by other tools, and against the point of the workspace (D-026, spec §5.8). **Rejected.**

## 5. Measurements

Generated data with the sizes of spike 3; Linux, ext4, median of 200 writes with the atomic
temporary file and rename of note 001.

| File | Size | Rewrite |
| --- | --- | --- |
| Vocabulary, 4,420 keywords, indented | 690 KB | 1.3 ms |
| A collection of 20,000 photos, indented | 780 KB | 1.5 ms |
| A collection of 2,000 photos | 78 KB | well under 1 ms |
| A series of 6 photos | 420 bytes | one small file |

A file of 780 KB is written in about a millisecond and read and parsed in a few milliseconds, so
**rewriting a whole file for every change is affordable**, even for a very large collection, if
changes come one at a time. A batch (adding 500 photos to a collection) is **one write**, not 500.
On NTFS, file creation costs about 0.75 ms (note 001), and a rewrite is a creation plus a rename,
so the same order of magnitude holds.

## 6. The proposal

### 6.1 Option B, with series taking a shortcut [decided, D-086]

**One file per collection and one per series; the vocabulary and the sources as single files.**

```
state/
  vocabulary.json
  sources.json
  collections/<collection id>.json          manual, smart and (M4) client-selection
  series/<xx>/<series id>.json              <xx> = first two characters, created on demand
  publications/<publication id>.json        reserved, M4
```

Collections stay in one flat folder (there are tens or hundreds). **Series are sharded** like the
sidecars (note 001): there can be 27,000 of them at a million photos, and a folder should not hold
more than a few thousand files. Older versions of Auroraw ignore folders they do not know
(note 001 §5.6), which is how M4 can add `publications/` and others without a layout change.

**Series membership is written in the series file, not in the sidecars.** Option C would save one
file per series, but the members' order, the cover and the resolution belong to the series and
not to a photo, and a series is edited as a whole (merge, split, resolve). The 2,700 small files
at 100,000 photos read in a fraction of a second (about 1.1 MB in all).

### 6.2 The form every state file has [decided, D-086]

- **JSON**, UTF-8 without a byte order mark, LF line ends.
- An **envelope** at the top of every file: `"format"` (for example `"auroraw/collection"`), `"schema"`
  (an integer, architecture §5.5), and, for an entity, `"id"` and `"updated"` (UTC, ISO 8601).
- **Canonical writing**: keys in a documented order, two-space indentation, arrays in a defined order
  (below), a final newline. The same content always gives the same bytes, so a file whose content
  did not change is **not rewritten** (no new modification time, no sync traffic), and a file can
  be compared with a diff or a checksum.
- **Unknown keys are kept**: the reader keeps everything it does not understand in an `extra`
  part and writes it back after the known keys, sorted by name (requirement 3), which keeps the
  canonical form deterministic. Unknown array **entries** of a kind the
  reader cannot interpret are kept as they are.
- **Identifiers** of state entities (keyword, collection, series, source, publication) are **64 random
  bits**, 16 lowercase hexadecimal characters, like version identifiers (note 001). A **photo
  reference** in a state file is the photo's 32-character identifier; a **version reference** is
  `<photo id>.<version id>`, which is also the stem of its sidecar's name.
- Sizes are not limited by the format; a very large file stays a normal one.

### 6.3 The content of each file [decided, D-086]

**`vocabulary.json`**

```json
{
  "format": "auroraw/vocabulary", "schema": 1, "updated": "2026-09-21T14:02:11Z",
  "keywords": [
    { "id": "…16 hex…", "name": "Fauna", "parent": null, "synonyms": ["Animals"], "export": true },
    { "id": "…", "name": "Heron", "parent": "…id of Fauna…", "synonyms": [], "export": false }
  ]
}
```

A tree by `parent` identifiers. **Keywords are sorted by identifier**, so that renaming or moving one
changes one entry and not the order of the file. A keyword has a stable identity: how sidecars
refer to it (by identifier, by path, or both, for interoperability with other software) is note 003.

**`sources.json`**

```json
{
  "format": "auroraw/sources", "schema": 1, "updated": "…",
  "sources": [
    { "id": "…", "kind": "local-folder", "name": "Working disk",
      "hint": { "path": "/mnt/photos" }, "ignore": ["*.tmp", "Thumbs.db"], "config": {} }
  ]
}
```

`kind` is the source plugin's identifier; **`config` is opaque** to Auroraw and belongs to that plugin
(validated by its declaration, D-078). **`hint` is only a hint**: a location from the machine that
last wrote it, useful to suggest where to look. The real location on **this** machine is kept in the
**local registry** (note 001 §5.7), not in the workspace, so that a workspace carried to another
machine shows its sources as missing and offers to relocate them (spec §5.1), and the answer stays
local. No credentials, ever: a source that needs sign-in stores its secret in the keychain and
only a reference here.

**`collections/<id>.json`**

```json
{
  "format": "auroraw/collection", "schema": 1, "id": "…", "updated": "…",
  "name": "Marie wedding", "kind": "manual", "parent": null,
  "members": ["…32 hex…", "…32 hex….…16 hex…"]
}
```

`kind` is `manual`, `smart` or (M4) `client-selection`. **`members` is ordered** (the order of a
gallery) and lists photo references and, where a collection pins a version, version references.
A **smart** collection has a `query` instead of `members`: a JSON tree of conditions whose fields
and operators are defined with the search in WP11, under its own `"query_schema"` number, so the
query language can grow without changing this file's schema. A query using a field this version does
not know is shown as "made by a newer version" and neither run nor modified. `parent` allows folders
of collections and is `null` at the top level.

**`series/<xx>/<id>.json`**

```json
{
  "format": "auroraw/series", "schema": 1, "id": "…", "updated": "…",
  "kind": "burst", "cover": "…32 hex…", "resolved": false, "kept": [],
  "members": ["…32 hex…", "…"]
}
```

`kind` says how it was made (`burst`, `bracket`, `similar`, `manual`). `members` are in capture order.
`kept` lists the photos designated to keep when the series is resolved; **resolving** sets `resolved`
and writes the Rejected flag on the others **in their sidecars**, which are their own state
(spec §5.3). Membership, cover and resolution therefore survive a rebuild, as the specification requires.

**`publications/<id>.json`** is reserved for M4 (spec §5.9): its content is decided in that milestone's
note.

### 6.4 Rules that apply to all of them [decided, D-086]

1. **A photo that is not in the workspace** (its sidecar is missing or moved out) but is named in a
   state file is **kept in the list and marked missing** when the catalogue is built, and reported;
   nothing is removed from a state file because a sidecar is absent (a synchronising machine may simply not
   have received it yet).
2. **Deleting an entity** (a collection, a series) moves its file to
   `removed/state/<kind>/<id>.<UTC time>.json`, never deletes it (architecture §5.3, item 4). Restoring
   is a menu entry and, at worst, a copy.
3. **A newer schema than the reader knows**: the file is **not modified**; the entity is shown as made by
   a newer version and left out of features that need it; everything else works. A file whose
   `format` is not recognised is **foreign** and only reported (note 001 §5.6).
4. **Reading is tolerant, writing is strict.** A reader accepts what it understands from a file with
   errors in an unknown part; it writes only canonical, valid content, and **never overwrites a file it
   failed to parse** (testing strategy §8).
5. **The rebuild order** is: marker, vocabulary, sources, collections, series, then photo and version
   sidecars. References to what does not exist yet are resolved after everything is read.
6. **Nothing here is written by anything but the single writer** (note 001 §5.5).

### 6.5 Synchronisation between machines [decided, D-086]

- **Used one after the other**, as D-064 documents, nothing conflicts: each file is complete and
  replaced atomically.
- **Changed on both machines before a sync**, a sync tool will typically keep both versions and
  make a "conflicted copy" beside one. The conflicted copy has an unknown name, so it is
  **left alone and reported** with the file it conflicts with (note 001 §5.6). Because there is one
  file per collection and per series, a conflict concerns one collection or one series, not
  the whole catalogue, and the reader can show the two versions side by side and let the
  photographer keep one (a later refinement, not M1). The vocabulary and the sources are single
  files and, being edited rarely, are the ones most exposed; they are small enough to compare by hand.
- **A merge tool is not proposed**: merging state files automatically would be a feature of its
  own, and D-064 puts built-in sync outside v1.

### 6.6 What goes in the local registry, not the workspace [decided, D-086]

Per machine and per catalogue, in the data directory: the real location of each source, the
last known **size and modification time** of every workspace file (the reconcile, architecture §5.3),
the window and panel layout and view state, and anything that depends on the machine. **Preferences that
should follow the photographer** (import profiles, export recipes, styles) are the **user library**
(D-052), not the workspace.

## 7. Consequences for WP1

- The `format` crate reads and writes exactly these files, with a **fixture for schema 1 of each**,
  a **round trip** test, a test that **unknown keys and entries survive**, a test that **the same
  content gives identical bytes**, and **fuzz targets** for the readers.
- The `workspace` crate places and scans them as note 001 says, and reports foreign and conflicted
  files.
- Tests for the rules of §6.4: a missing photo, a newer schema, a file that fails to parse, and a
  deleted entity landing in `removed/`.
- The 100,000-photo dataset generator (WP2) produces the same sizes as the measurements above, so
  the rebuild is timed with realistic state.

## 8. Not settled here

| Item | Where |
| --- | --- |
| How a sidecar refers to a keyword, and the content of the photo and version sidecars | Note 003 |
| The query language of smart collections | WP11 (its own schema number) |
| The content of publications and annotations | M4 |
| Very large collections (over about 50,000 photos) if rewriting a whole file proves too slow | Measured on Windows at the end of WP1; a chunked or appended form can be added under a new schema number |
| Showing and resolving a sync conflict in the interface | After M1 |

## 9. What is asked

Approval of: one file per collection and per series, with the vocabulary and the sources as single
files (§6.1); the common form and the canonical writing (§6.2); the content of each file
(§6.3); the rules of §6.4; and the split between workspace and local registry (§6.6), as **D-086**.
