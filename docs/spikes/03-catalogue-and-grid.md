# Spike 3: the catalogue, the thumbnails and the workspace (interim report)

> **Status: interim.** Measured on Linux only, on a SATA SSD, with a synthetic but realistic
> catalogue. Windows (NTFS and antivirus with 243,000 small files) is the open risk. Nothing here
> is a final decision.

## Question

Does a SQLite catalogue with a thumbnail cache meet the budgets at 100,000 photos, and is the
rebuild of the catalogue from the workspace (D-026) fast enough to be routine?
(docs/technical-spikes.md, §4)

## What was built

`spikes/catalogue`, `spikes/viewer-catalogue` (at the tag `spikes-final`):

- **A generated catalogue of 100,000 photos** (54 MB): 250 shoots of about 400 photos, 4,420
  hierarchical keywords (20 categories, 400 groups, 4,000 keywords), 653,672 keyword links (six or
  seven per photo), 143,106 versions (70% of photos have one, 20% two, 10% three or four),
  2,728 series formed by bursts (about 15% of the photos), 200 collections, 30% of the photos
  with a caption, a stored effective rating that follows a version's override (D-063), and a
  full-text index. Cameras, lenses, ISO, dates and GPS are spread realistically.
- **100,000 thumbnails** of 256 px on the long edge, cut and encoded from six real developed images,
  stored two ways: as files and as blobs in a SQLite database (two page sizes).
- **A workspace** of 100,000 photo sidecars (XMP, 1.6 KB each) and 143,106 version sidecars
  (XMP with a JSON history, 3.3 KB each), plus the state files (vocabulary, collections,
  series), as in the specification (§5.1, §5.4).
- **A rebuild** of the catalogue from that workspace alone.
- **The grid of spike 2, fed by the real catalogue** and the thumbnail database, with thumbnails
  read and decoded on four worker threads.

## Results

Machine: Intel i7-10700 (8 cores, 16 threads), data on a **SATA SSD** (WD Blue SA510), ext4.
The `/home` NVMe disk was too full to hold the data.

### Queries (100,000 photos, median of 30, 200-photo pages)

| Question | Median | Budget |
| --- | --- | --- |
| Open the catalogue, cold, and show the first page | **6.4 ms** | a few seconds |
| The 200 newest photos | 0.08 ms | |
| A page at a random date (keyset) | 0.6 ms | |
| A page at offset 80,000 (the slow way) | 2.7 ms | |
| Count of the visible photos | 2.3 ms | |
| Rating 4 or more, stored effective rating: page / count | 0.2 / 5.2 ms | |
| Rating 4 or more, computed by a join: page / count | 0.4 / **15.8 ms** | |
| One month of shooting | 0.1 ms | |
| Camera and ISO range: page / count | 0.2 / 0.9 ms | |
| A keyword group (10 keywords, 1,400 photos): page / count | 1.4 / 0.5 ms | |
| A keyword category (200 keywords, 22,000 photos): page / count | 21 / 11 ms | |
| Category, rating and a year together | 12 ms | |
| **Full-text search** (prefix): page / count | **1.1** / 0.3 ms | under 200 ms |
| The same by `LIKE '%word%'`, no index | 24 ms | |
| A collection (20 to 2,000 photos) | 0.75 ms | |
| Expand a series | 0.02 ms | |
| Facet: ratings under a category | 15 ms | |
| Facet: top 20 keywords under a category | **53 ms** | |
| Set the rating of 1,000 photos, one transaction | 40 ms | |
| Set the rating of one photo (WAL) | 0.02 ms, or 0.6 ms with a forced sync | |
| Add a keyword to 1,000 photos | 41 ms | |

Everything is far under the 200 ms budget. The slowest questions are the keyword categories,
which cover 22,000 photos, and the keyword facet at 53 ms. A stored, denormalised effective
rating is three times cheaper to count than one computed by a join, which supports keeping it in
the table.

### Thumbnails

| | Result |
| --- | --- |
| One thumbnail, one core, from a 1620 px JPEG | decode 7.0 ms + resize 2.9 ms + encode 0.5 ms = **10.4 ms** |
| 100,000 thumbnails, 16 threads | **124 s, 805 per second** (about 100 per core) |
| Size | **8.1 KB each**, 787 MB for 100,000 |
| Decode a thumbnail for display | 0.25 ms |
| Files: written, in 16 threads | 0.3 s; 787 MB of data, **986 MB on disk** (block rounding) |
| Blobs, 4 KB pages: size | 866 MB |
| Blobs, 32 KB pages: size | 948 MB |

Reading a page of **200 thumbnails**, median of 20:

| Access pattern | Files, cold | Blobs 4 KB, cold | Blobs 32 KB, cold | Any, warm |
| --- | --- | --- | --- | --- |
| The next 200 by date (contiguous ids) | 27 ms | **5.8 ms** | 6.2 ms | 0.7 to 0.9 ms |
| A filter where every fifth photo matches | 28 ms | 53 ms | **30 ms** | 0.7 to 1.4 ms |
| A random set, in ascending order | 34 ms | 71 ms | **42 ms** | 0.7 to 1.5 ms |

Blobs in a single database read the default view (by date) four times faster and match the
files elsewhere when the pages are 32 KB. Files are steadier but cost 100,000 directory entries.

**Generation was memory-bound at first**: the first version plateaued at 350 thumbnails per second
with four threads, because it copied the full-size image four times. Cropping inside the resizer
removed three copies and raised the plateau to 830 per second, at the number of physical cores.
A decoder that decodes at reduced size (DCT scaling), as libjpeg-turbo does, would go further.

### The workspace

| | Result |
| --- | --- |
| Write 100,000 photo and 143,106 version sidecars, 16 threads | 1.2 s (1 GB on disk, 632 MB of data) |
| **Rewrite one photo sidecar** (temporary file, then rename) | **0.04 ms**, or **0.75 ms** with a forced sync (worst 186 ms) |
| Change detection: one `stat` for each of the 243,106 files | 28 ms |
| **Rebuild the catalogue from the workspace, warm cache** | **3.2 s** |
| **Rebuild, cold cache** (every sidecar read from disk) | **10.1 s** |
| Was the rebuilt catalogue right? | yes: same rows, versions, keyword links, sums |

In the rebuild the insertion (2.4 s, one thread, with the full-text index) is the main cost
when warm; reading the files (3.4 s) is when cold.

The sidecars carry a **cache of what was read from the original** (capture time, camera, lens,
exposure, size, GPS). Without it, a rebuild must open the RAW files. On real files, reading only
the metadata takes about **20 ms cold and 1.2 ms warm** per file, which for 100,000 files is over
four minutes cold on this SSD (and far more on a network share or a hard disk), and is **impossible
for a source that is offline**.

### The grid fed by the catalogue

The grid of spike 2, with the 87,715 visible photos of the catalogue and thumbnails loaded on
four threads (most recent request first, an 800-thumbnail cache). Scripted: ten seconds of
scrolling 45 px per frame down the whole list, 25 jumps to random places (as when dragging the
scrollbar), and eight filter changes. On the real display at 60 Hz:

| | Warm | Cold (database files dropped from the cache) |
| --- | --- | --- |
| Open the catalogue and list 87,715 photos | 38 ms | 559 ms |
| Frames while scrolling: median / p99 / worst | 16.6 / 18.5 / 19 ms | 16.7 / 18.3 / 20 ms |
| Frames over 20 ms | 0 of 601 | 1 of 601 |
| **Frames with an empty cell on screen** | **0 of 601** | **0 of 601** |
| Thumbnail request to shown: median / p95 | 16.0 / 17.5 ms | 15.8 / 17.4 ms |
| A random jump to a full page of thumbnails | median 17 ms, worst 31 ms | median 17 ms |
| A filter change to a full page: all / rating 4+ / group / category | 39 / 34 / 16 / 28 ms (the first of each: 130 / 116 / 15 / 44 ms) | similar |

The scrolling never shows an empty cell, at 21 rows per second. The cold run drops the files only
at the start, so the later parts are partly warm.

## What it means

1. **SQLite meets every budget with room to spare** at 100,000 photos: a page is under 1 ms,
   the slowest filter is 21 ms, the search is about 1 ms, and opening is 6 ms cold.
2. **Keep the effective rating in the table** (it is the value the grid shows and sorts on).
3. **Keep the thumbnails in a separate SQLite database with 32 KB pages** (a cache, deletable
   at will as in specification §5.1), unless Windows says otherwise. It is faster for the default
   view, holds one file instead of 100,000, and equals files elsewhere. Files remain the fallback.
4. **The photo sidecar must carry the cache of the original's metadata.** It is what makes the
   rebuild a 3 to 10 second operation instead of a four-minute one, and it is the only way to
   rebuild with an offline source.
5. **The rebuild from the workspace is routine** (D-026): seconds, and the result was checked.
   Metadata edits can be written immediately to the workspace (D-022): 0.04 ms, or under 1 ms
   with a sync.
6. **Thumbnail generation must avoid copies and decode at reduced size.** A first import of
   1,000 photos takes about a second and a half at 800 per second, from a 1.6 MP preview.
7. **The grid needs only a small page of data at a time**, thumbnails loaded ahead of the
   scroll on a few threads, most recent request first, and a modest cache.

## What remains

- [ ] **Windows.** File creation and reading on NTFS, with an antivirus scanning, is the classic
  weak spot of a design with 243,000 small files and 100,000 thumbnails. `run-spike3.sh` runs the
  whole spike there. Cold-cache figures are not available on Windows.
- [x] **A slower disk** (issue #1, 2026-09-22, and docs/design/001-workspace-layout.md §4.2): measured on
  Patrick's real machine, a 2 TB 7200 rpm HDD as the system disk. The design tolerates it (writing 225,074
  sidecars took 25 minutes there, reading them 54 s), but "a rebuild takes seconds" no longer holds
  unconditionally: on a spinning disk it is tens of seconds to read, tens of minutes to write a first
  workspace. A network share is still unmeasured.
- [ ] **Real data.** The catalogue is generated; real shoots are less regular. The thumbnails come
  from six images.
- [ ] **Several catalogues, several writers.** One writer at a time was measured; the interaction
  of the grid reading while an import writes was not.
- [ ] **The Intel iGPU and macOS** for the grid (uses spike 2's toolkit, so not new).

## Running it

```bash
cd spikes
cargo build --release
./target/release/build_db                      # the catalogue, about 2 s
./target/release/query_bench                   # the queries
./target/release/thumbgen --lean               # thumbnails, needs samples/previews (about 2 min)
./target/release/workspace --write --edit      # the sidecars
./target/release/workspace --rebuild [--cold]  # the rebuild
./target/release/exifcost samples              # the cost of reading metadata from RAW files
./target/release/viewer-catalogue --secs 10    # the grid
```

The data goes in `$AUR_DATA` (default `/tmp/auroraw-spike3`), about 3.5 GB. Raw results are in
`docs/spikes/results/spike3-*.json`.
