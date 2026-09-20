# Auroraw: functional specification

> **Status: living draft.** This document describes *what* the application does, not *how* it is
> built. Every item carries a status:
>
> - **[decided]**: confirmed by Patrick, recorded in [decisions.md](decisions.md).
> - **[proposed]**: working proposal, to be validated.
> - **[open]**: unresolved question.

## 1. Vision

Auroraw is a free (GPL-3.0) application for photographers: organise photos, develop RAW files and
film negative scans non-destructively, convert between formats, edit metadata and deliver
galleries, all in a simple, fast and professional workflow.

**What should set it apart** [decided]:

- **Against darktable**: simplicity and an intuitive workflow.
- **Against Lightroom**: at least parity in speed, simplicity and capabilities. (An ambitious
  goal, knowingly. The comparison will be refined once Patrick has tried Lightroom.)
- **By design**: openness (documented data formats, plugins, no lock-in), development versions
  as a first-class concept, and delivery to clients built into the workflow.

### Audience and scope [decided]

- Priority 1: digital photography. Priority 2: film workflows (negative scans).
- Single-user. No shared catalogue or team collaboration for now.
- Still images only. Video is out of scope.
- Platforms: Linux, Windows and macOS from the start.
- Multilingual (the interface language is selectable in the application).

## 2. Concepts and vocabulary

| Term | Definition |
| --- | --- |
| **Source** | A place where files live: local folder, network share, removable or offline drive, memory card, or a source provided by a plugin. |
| **Original** | An image file as it exists on a source. Never modified by Auroraw. |
| **Photo** | What the photographer thinks of as "one shot": one original, or a RAW+JPEG group treated as a unit. It has a stable identity in the catalogue, independent of file name and location. |
| **Series** | Several related photos (burst, bracketing, near-duplicates) grouped for culling. Distinct from versions. |
| **Version** | One development of a photo: its chain of operations, its history, its name. Creating a version copies no pixels. A photo always has a default version, which exists only in the catalogue until the first edit [proposed]. |
| **Workspace** | A folder, owned by exactly one catalogue (one workspace per catalogue), that receives everything Auroraw writes about photos: sidecars and, optionally, exports. Originals' folders are never written to. |
| **Photo sidecar** | An XMP file in the workspace holding the metadata of an original (rating, keywords, captions, IPTC...). |
| **Version sidecar** | A file in the workspace holding one version: a copy of its photo's metadata, the version's own metadata, and its development chain, history and snapshots. |
| **Snapshot** | A named state of a version's history, which can be returned to or turned into a new version. |
| **Operation** | A non-destructive development step (exposure, curve, mask, etc.), parameterised and ordered in the chain. |
| **Style / preset** | A reusable set of operations and parameters, applicable to one or more versions. |
| **Collection** | A grouping chosen by the photographer (manual) or defined by a query (smart). |
| **Catalogue** | The local database indexing photos, their versions, their metadata and their previews. |
| **Export recipe** | A named, reusable setting for producing files (format, size, profile, metadata, naming, watermark, etc.). |

## 3. Guiding principles

1. **Originals are never modified** [decided]. After import Auroraw does not move, rename or
   delete them either (D-018).
2. **Nothing is trapped in the catalogue** [proposed]. The catalogue is an index and a working
   area that can be rebuilt. Metadata and versions are also written to open files in the
   workspace (see §5.1 and §5.4).
3. **Workflows, not panels** [proposed]. The interface is organised around tasks (Import, Cull,
   Develop, Publish), not a stack of tools.
4. **Keyboard first** [proposed]. All routine culling and navigation works without a mouse.
5. **Local AI by default** [proposed]. No image or metadata leaves the machine for an external
   service without explicit consent, feature by feature.
6. **Openness and interoperability** [decided for Prooftide, proposed elsewhere]. No feature may
   require a particular service or piece of software; integrations go through open formats and
   plugins.
7. **Measured performance budgets** [proposed]. See §9.

## 4. Reference workflow [proposed]

1. **Import**: from a card or a folder, with duplicate backup, renaming and metadata applied.
2. **Cull**: ratings, flags, labels, comparison, resolving series and duplicates.
3. **Develop**: one or more versions of the kept photos; styles and settings sync.
4. **Publish**: a client gallery (Prooftide or another), with selections returned to the
   catalogue.
5. **Deliver**: batch export of the chosen versions through recipes.

Moving from one step to the next should be a single gesture (key or button), with no needless
dialog in between.

## 5. Features by domain

The milestone of each feature (M1 to M5) is described in §8.

### 5.1 Sources, catalogues and workspaces

**Catalogues** [decided, D-017]

- A photographer can have **several catalogues, all equal**, and picks one when opening the
  application. The intended use is separate areas of activity (family versus professional, for
  example).
- The application must be **fully functional with a single catalogue**. The first launch creates
  one silently; switching and creating catalogues is available from a menu but never required
  [proposed].
- Catalogues are isolated: search, collections, keyword lists and duplicate detection work within
  the open catalogue [proposed].
- Auroraw writes nothing into sources (see below), so catalogues do not need to claim source
  folders exclusively: two catalogues may reference the same folder, each with its own workspace
  and its own metadata. A notice tells the photographer when a folder is already in another
  catalogue [proposed].
- Each catalogue has **exactly one workspace** [decided, D-025]. To the photographer the two can
  present as a single thing: "my catalogue and its folder" [proposed].
- The catalogue database stays on the **local disk**, in the user's data folder, and is
  rebuildable from the workspace and the sources [decided, D-026]. Previews live in a separate
  cache that can be deleted without losing anything [proposed].

**Sources are never written to** [decided, D-018, refined by D-022]

- Auroraw writes into a source in only two cases: copying files at import, and an explicit
  request from the photographer to export XMP files into source folders (see below). It never
  modifies, moves, renames or deletes an original.
- Consequences [proposed]: the Rejected flag deletes nothing. A "Rejected" view offers *Show in
  file manager* and an exportable list of rejected files, so tidying happens outside Auroraw.
  Adding a folder "in place" copies and touches nothing. Read-only media and archives need no
  special handling.

**Workspaces** [decided, D-022 and D-023]

- A workspace is a **folder** that receives the sidecars of the originals and of their
  versions, and optionally exports. Each catalogue has exactly one, and a workspace is never
  shared between catalogues.
- The **photo sidecar** contains only the metadata of the original. It is standard XMP.
- The **version sidecar** copies the photo's metadata and adds the version's own: overrides,
  development chain, history and snapshots. A version sidecar is therefore self-contained.
- Sidecars are not next to the originals, so a sidecar is tied to its original by **content
  fingerprint**, with the last known path and file name as hints [proposed].
- **Exports** (JPEG and others) may go into the workspace or anywhere else. Exporting into a
  source folder is discouraged and triggers a warning, not a ban [decided, D-024].
- **XMP export to the source folders** [decided, D-024]: an option for compatibility with other
  software. It writes the photo sidecars of the RAW files next to the originals, on explicit
  request only (for a selection, a folder or a whole source). It is one-way and covers
  photo-level metadata. The naming convention (`photo.xmp` or `photo.ARW.xmp`) is the
  photographer's choice, `photo.xmp` by default [decided, D-028].
- **Existing XMP** files found next to originals at import (from Lightroom or others) are read,
  never modified, and their ratings and keywords are copied into the photo sidecar [proposed].
- Because sidecars live in the workspace, metadata edits are written immediately, whatever the
  state of the source: read-only card, unplugged drive, network share down [proposed].
- The catalogue can be rebuilt from the workspace and the sources [decided, D-026]. For that to
  be complete, everything that would otherwise exist only in the database (collections, smart
  collection definitions, the list of sources, gallery publications) is also written into the
  workspace; the format is open. Backing up the work is copying a folder.
- When a photo's metadata changes, the copies in its version sidecars **follow**, except for the
  fields a version overrides [decided, D-027]. A version sidecar is a denormalised copy of the
  effective values.

The exact layout of a workspace and its default location are open (§10).

**Source states** [proposed]

| State | Meaning | What works |
| --- | --- | --- |
| Online | Reachable | Everything |
| Offline | Not reachable (unplugged drive, network down) | Browse, search, sort, rate, tag, view previews. Metadata edits are written to the workspace as usual. Anything that needs the original (development, export, publication) is shown as unavailable, not hidden. |
| Missing | The expected location no longer exists | Same as offline, plus a prompt to relocate the source |

Each source shows its state as a badge. **Developing on an offline source is not supported in
v1** [decided, D-021]: a reduced-resolution proxy would not give results identical to the
original for detail-dependent operations (see §10).

**Detecting new, changed and moved files** [decided, D-019]

- Online sources are **monitored**. New files are reported ("42 new photos in this folder") and
  added with one click; nothing enters the catalogue without confirmation. Folders or patterns
  can be ignored [proposed].
- **Moved or renamed** files are found again through a content fingerprint. An unambiguous match
  is relinked automatically and silently; ambiguous cases are put to the photographer.
- **Changed** files (edited in another application) keep their versions; the photo is marked
  "original changed" and previews are refreshed on request [proposed].
- **Deleted** files show as missing and are never removed from the catalogue automatically; a
  "Remove missing photos" action exists [proposed].
- Network shares do not deliver reliable change notifications, so those sources are rescanned on
  demand and at intervals [proposed].

**Search, filters and sorting**

- Search, filter and sort on all metadata (EXIF, IPTC, tags, ratings, dates, lens, version,
  publication state, etc.) [decided].
- Manual and smart collections, hierarchical tags, ratings [decided].
- Catalogue backup and sync [decided, v1]: details to be designed (§10).

### 5.2 Import

**Ways to bring photos in** [proposed]

| Way | What happens |
| --- | --- |
| Copy from a card or a folder | Files are copied to a destination folder in the photographer's archive, then added to the catalogue. This is the only case, besides the explicit XMP export, where Auroraw writes into a source. |
| Add in place | An existing folder becomes a source. Nothing is copied or written. |
| Other | Cameras and phones connected directly, cloud services and so on, through source plugins (§5.10). |

**Import profiles** [decided, D-029]

- A profile stores everything an import needs: destination folder template, renaming template,
  backup copy destinations, metadata template (creator, copyright, keywords), a style to apply,
  and the RAW+JPEG rule below [proposed].
- When a card is inserted, Auroraw offers "Import with profile X": **one click**. Nothing
  starts on its own; the click is always the photographer's [proposed].
- The full dialog is the profile editor (and can run a one-off import with unsaved settings).
  Everyday imports do not go through it.

**Everything is imported, culling comes after** [decided, D-030]

- No pre-selection: every file on the card is copied.
- Culling can begin as soon as the first photos are copied, while the copy continues; thumbnails
  come from the previews embedded in the files [proposed].

**Copy safety** [proposed]

- Each copy is verified by checksum against the source.
- An optional second copy (backup) goes to another location and is verified the same way.
- An interrupted import resumes where it stopped.
- Files already imported are recognised by content fingerprint and skipped, with a report.
- A file is never overwritten; a name collision gets a unique suffix.

**After the import** [decided, D-031]

- Auroraw **deletes nothing on the card**. When every copy and backup is verified it shows "All
  N files copied and verified to X and Y" and offers to eject the card. The photographer formats
  the card in the camera.

**RAW+JPEG pairs** [decided, D-032]

- A pair is **one photo with two files**. The RAW is the file that is developed; the JPEG is a
  companion (the camera's rendering, quick delivery). Both are copied.
- A profile setting can ignore the JPEGs or the RAW files. A JPEG without a RAW is a photo like
  any other.

**Metadata and geolocation at import** [proposed]

- The profile's metadata template is written to the photo sidecars in the workspace, never into
  the copied files.
- Geolocation from GPX tracks [decided, v1] applies at import or afterwards, with a correction
  for the camera clock offset and time zone. Photos can be shown on a map.

**Formats** [decided]

- Import of all RAW formats (through import plugins; the core bundles the most common ones) and
  of all image formats used in the photo and film industries. Video is excluded.

### 5.3 Culling, series and duplicates

**Vocabulary** [decided in principle, details proposed]

- Rating from 0 to 5 stars, flags (Picked, Rejected) and colour labels. While culling they apply
  to the photo; a version may override them later (§5.5).
- Every action is one key and can be undone, several steps back [proposed].
- Rejected photos are hidden by default, with a toggle to show them. **Nothing is ever deleted**
  (D-018): the Rejected view offers *Show in file manager* and an exportable list (§5.1).

**Cull mode** [decided, D-033]

A dedicated, full-screen mode for culling. The grid stays the navigation view.

- The image fills the screen, with a discreet filmstrip [proposed].
- One key per rating, flag and label; optional auto-advance after each action [proposed].
- Side-by-side comparison of 2 to 4 images, with zoom and pan synchronised [proposed].
- A series expands in place [proposed].
- Culling can start on the last import, even while the copy is still running (§5.2) [proposed].
- Quality aids that need no AI, in M1 [proposed]: focus peaking, clipping warnings, blur score,
  optional histogram.
- AI aids (closed eyes, best frame of a series) come in M5 and only **suggest**; they never rate
  or reject on their own [proposed].

**Series** [decided, D-034 and D-035]

- **Objective series form automatically**, at import and whenever photos are added: bursts and
  bracketing recognised from metadata (sequence, gaps between capture times, exposure
  parameters). The time gap is adjustable. RAW+JPEG pairs are already one photo (§5.2).
- **Visually similar photos are suggested**, not grouped: "5 similar photos", and the
  photographer confirms. Similarity is computed locally from the previews [proposed].
- A series collapses into one thumbnail with a count and expands in place. Members can be added,
  removed, merged or split by hand [proposed].
- **Resolving a series**: the photographer designates the photos to keep, and **the others get
  the Rejected flag**. The series stays expandable. It is one gesture and can be undone.
- A series shows whether it is resolved, and can be filtered on that [proposed].
- Series membership and resolution are catalogue state, so they are also written into the
  workspace (D-026) [proposed].
- A series is not a version: a series groups several photos, a version is one photo developed
  several ways.

**Duplicates** [decided, D-036]

- **Exact duplicates** (the same content fingerprint at several locations, for example the
  working disk and its backup) are **one photo with several locations**. The catalogue uses
  whichever location is available, and sidecars and versions are not duplicated. The photo's
  information shows all its locations.
- An optional **duplicates report** lists the photos that exist at several locations (which
  sources, which sizes) so the photographer can tidy up in the file manager. Auroraw never
  deletes files. The list can be exported. Milestone: M1 or shortly after [proposed].
- **Near duplicates** are handled as series suggestions, not as duplicates.
- Import already skips files it has imported before (§5.2).

### 5.4 Development experience and versions

**Interface** [decided, D-037]

- The Develop view is organised in **guided panels by task** (Light, Colour, Detail, Geometry
  and so on) showing the common controls. Advanced tools are added on demand and appear only
  once used [proposed].
- The internal order of operations is **not something the photographer rearranges** in everyday
  use: a default pipeline that works out of the box is provided (§5.6). Configuring the pipeline
  is possible for advanced users and plugin authors, outside the everyday interface.
- Tools shared by every view [proposed]: before/after (split or side by side), histogram,
  clipping warnings, 100% zoom, and moving to the previous or next photo from the keyboard
  without leaving Develop.

**History and snapshots** [proposed, principle decided in D-006]

- Each version has a **linear history**, kept persistently in its sidecar; a button compacts it.
  Editing after an undo discards the undone steps: exploring variants is what versions and
  snapshots are for.
- A **snapshot** is a named bookmark in a version's history. It can be compared with the current
  state, restored, or turned into a new version.

**Versions** [decided: versions, virtual copies and snapshots in v1]

- A photo can have as many versions as wanted ("high-contrast black and white", "client
  version", "cinema experiment"). A version is created from scratch, from another version, from
  a snapshot or from a style. A one-key "duplicate and try" creates a version from the current
  one [proposed].
- Versions **share the original** and copy no pixels.
- **Main version** [decided, D-038]: each photo has one main version, chosen by the photographer
  (by default the last one edited). It supplies the grid thumbnail and is the default for export
  and publication. Other versions are **collapsed** under the photo, with a badge showing how
  many there are, and unfold on demand.
- In Develop, a **version strip** switches between versions with one key [proposed].
- The grid shows the main version's effective metadata (§5.5) [proposed].
- Two versions can be compared side by side or with a wipe [proposed].
- Every output operation (export, publication) applies to a chosen version, to the main version
  or to a set of versions [proposed].

**Reusing settings between photos** [decided, D-039 and D-040]

- **Selective copy and paste**: copy chosen settings, paste them onto one or several photos.
- **Styles**: a style is a named set of settings covering one or several tools. There is **one
  concept only**: what other software calls a tool preset is simply a style that holds a single
  tool, and appears in that tool's menu as well as in the global style list.
- Applying a style or a paste is a one-off action; it leaves **no lasting link** between photos.
  Changing a style later does not alter photos that already used it.
- **Auto-sync** (optional switch): each adjustment made on the active photo is applied live to
  the selection, for a series or a whole shoot.
- A style can be applied at import (import profile, §5.2) and can be applied selectively
  (which tools) [proposed].

**Persistence of versions** [proposed]:

| Layer | Contains | Format |
| --- | --- | --- |
| Catalogue | Index, previews, queries, anything that speeds things up | Local database, rebuildable |
| Photo sidecar (workspace) | The original's metadata: keywords, ratings, captions, IPTC, rights | Standard XMP, readable by other software |
| Version sidecar (workspace) | A copy of the photo's metadata, the version's own metadata, its chain, history and snapshots | Open, documented format built on XMP if practical, with a schema version in each file |

The workspace lets the catalogue be rebuilt from the sidecars and the sources. Development
settings from other software are not portable as-is; a partial import of common settings
(Lightroom, darktable) is conceivable later [open].

### 5.5 Metadata: photo and version [decided]

Some metadata can be **overridden at the version level**. Example given: rate the photo for its
composition, then rate each version for its rendering.

**Rule** [decided in principle, details proposed]: the effective value of an overridable field is
the version's if it defines one, otherwise the photo's. The interface shows both and makes clear
where the displayed value comes from.

| Field | Scope | Proposed behaviour |
| --- | --- | --- |
| Rating (stars) | Photo, overridable per version | Replacement. |
| Flag / colour label | Photo, overridable per version | Replacement. |
| Keywords | Photo, plus additions on the version | Union of the photo's and the version's keywords. In v1 a version can only add, not remove [decided, D-014]. |
| Title, caption | Photo, overridable per version | Replacement. |
| Capture EXIF, GPS | Photo only | Not overridable. |
| Copyright, creator | Photo, applied at export | Not overridable (except by export recipe). |

**Consequences**:

- Filters and sorts ("4 stars or more") apply to the effective value; one can choose to filter on
  the photo alone.
- Export and publication write the **effective** metadata of the exported version into the
  produced file.
- The photo sidecar carries the photo's values as-is. A version sidecar carries the effective
  values and records which fields it overrides, so inheritance survives a rebuild [proposed].

### 5.6 Development (RAW, scans, images)

- Non-destructive RAW development with GPU acceleration [decided].
- Colour management [decided]: input profiles, wide floating-point working space, output
  profiles; soft proofing is planned for later but the architecture allows it.
- Basic operations [proposed]: exposure, white balance, curves, colour and tone, sharpening,
  noise reduction, crop and straighten.
- Masks and local retouching [decided, v1]: gradients, brushes, selection by luminosity and by
  colour; AI-assisted selections [proposed].
- Lens corrections through Lensfun [decided: v1, milestone M3].
- Styles and settings reuse between images [decided, v1]: see §5.4.
- Negative scans [decided, priority 2]:
  - v1: inversion, film-base (orange mask) sampling, per-channel curves, frame cropping.
  - Later: emulsion profiles, infrared dust detection, multi-exposure scans.
- Later [decided]: merges (HDR, panorama, focus stacking).

**Pipeline composition and operation plugins** [proposed, following D-037]

The photographer does not manage the order of operations, but plugins must fit into it. The
model:

- The pipeline is described by a **pipeline definition**: a documented, versioned list of
  **stages**, each working in a defined data space (for example: raw, scene-linear working
  space, display-referred, output). Auroraw ships a default definition that works.
- Each operation, built-in or plugin, **declares** its stage, optional ordering constraints
  within the stage ("after exposure", "before sharpening"), and its capabilities (GPU shader,
  needs neighbouring pixels and how many, depends on pixel scale, supports masks) and the panel
  it belongs in (Light, Colour, Detail...).
- Auroraw **places** an operation from its declaration; a plugin with no constraint goes at the
  end of its stage. Conflicts are reported at load time, not silently resolved.
- The order used is **recorded in each version's sidecar**, with the version of each operation
  and its plugin. An old edit therefore renders the same way after a plugin is installed or
  updated.
- A version that uses a **missing plugin** opens with that operation disabled and clearly marked;
  its settings are kept in the sidecar and are never dropped.
- Advanced users and plugin authors can **configure the pipeline definition** (a documented,
  shareable file). Configuration is outside the everyday interface, changes only apply to new
  versions, and is validated (an operation cannot leave its stage).

### 5.7 Metadata editing

- Editing of EXIF, IPTC and XMP tags (copyright, captions, hierarchical keywords), one by one or
  in batch [decided, v1].
- Metadata templates applicable at import and at export [proposed].
- EXIF edits do not touch the original by default; writing into the file is an explicit action
  [proposed].

### 5.8 Export

- Export to all image formats used in the photo and film industries [decided].
- Batch export with reusable recipes [decided, v1]: format, size, output sharpening, profile,
  metadata, naming, watermark, destination.
- Destination: the workspace or any folder. Exporting into a source folder shows a warning
  [decided, D-024].
- Export of one or several versions per photo; deterministic naming (a `{version}` token, for
  example) [proposed].
- Gallery export through plugins (§5.9) [decided].

### 5.9 Galleries and Prooftide

**Principle** [decided]: a polished integration without interdependence. Auroraw must work with
gallery services other than Prooftide, and Prooftide must stay usable with Lightroom and other
workflows. Prooftide may be extended to integrate better with Auroraw, but such extensions are
optional and backward compatible.

**Model** [proposed]:

1. **Gallery export plugin**: Prooftide is the first implementation, separate from the core.
   Another service plugs in through the same mechanism.
2. **Publishing a version**: rendering (colour, resizing, watermark) happens in Auroraw's
   pipeline, within the service's constraints (for Prooftide: JPEG, 1600 px maximum).
3. **Published name** [decided, D-009]: Auroraw generates a unique file name for each published
   version and keeps the mapping *gallery → photo, version, published name* in the catalogue.
   **No server change is required.** Proposed rule: the name stays that of the original when a
   single version of the photo is published, and gets a suffix (`marie_black-and-white.jpg`) when
   several are published or on collision.
4. **Client feedback**: selections and annotations (text, audio, drawing in Prooftide's case)
   come back into the catalogue, resolved through the mapping above: a "Client selection"
   collection or flag, and annotations visible on the photo.
5. **Delivery**: from the selection, batch export of the chosen versions through a recipe.
   Delivered versions are rendered from the catalogue, not copied from the originals.

**Interoperability safeguards** [proposed]:

- **Fallback without a plugin**: Auroraw can export a folder of ready-to-publish JPEGs, which can
  be handed to the current Prooftide application or to any other service.
- **Generic selection import**: from a list of file names or a selection file, for any gallery
  service or any client.
- The Prooftide plugin uses only the service's public, documented API. The status of that API for
  third-party clients is to be clarified [open] (§10).

### 5.10 Plugins

Four families [decided]: **import** (including RAW reading), **export** (images and galleries),
**image-processing operations**, **image sources**.

High-level requirements [proposed]:

- Auroraw's internal modules (operations, base formats) use the same interface as external
  plugins, so the interface is validated as early as milestone M2.
- Operation plugins must be able to run on the GPU and declare where they sit in the pipeline
  (§5.6).
- A plugin declares its permissions (network, disk access); the user sees them before
  installation.
- The public API is frozen in milestone M5, not before.

Language, ABI, isolation and distribution are handled in the "development process" phase (§10).

### 5.11 AI

Integrated where relevant [decided]; principles in §3 (local by default).

Candidates in order of interest [proposed]: assisted culling; subject and sky selection for
masks; noise reduction; semantic search and keyword suggestion; captions and metadata
translation (external services, only with consent).

### 5.12 Interface and languages

- Multilingual interface [decided]; translations are plain language files open to contributions.
- Professional and friendly [decided]: see the principles in §3.
- Light and dark themes, with a neutral-background viewing mode for judging colour [proposed].
- Accessibility (keyboard, contrast, screen readers) [proposed].

## 6. Formats [decided in principle]

| Category | Scope |
| --- | --- |
| RAW | All manufacturer RAW formats, DNG included. The core relies on an existing decoding library; exotic cases go through import plugins. |
| Images | JPEG, TIFF, PNG, WebP, HEIF/HEIC, AVIF, JPEG XL, OpenEXR, DPX, PSD (read), etc. |
| Film industry (stills) | DPX, OpenEXR, 16 and 32-bit TIFF, ACES/OCIO management considered. |
| Video | Out of scope. |

The detailed list (read-only or read and write, per format) will be drawn up in milestone M1
[open].

## 7. Explicitly not in v1

Merges (HDR, panorama, stacking), soft proofing and printing, tethering, multi-user
collaboration, shared catalogues, video. Architecture decisions taken before those milestones
must not rule them out (notably soft proofing, which depends on colour management).

## 8. Milestones [decided]

| Milestone | Content | What it gives the user |
| --- | --- | --- |
| **M1** | Catalogue, sources, import (card and folders), previews, culling, series and duplicates, tags, ratings, IPTC/XMP, search and filters, GPS | A complete culling and organising tool |
| **M2** | Non-destructive pipeline (basic operations), versions and snapshots, presets and styles, colour management, GPU acceleration, internal modules written as plugins | Basic RAW development |
| **M3** | Masks and local retouching, lens corrections, negative scan module, advanced operations | Professional-level development |
| **M4** | Batch export with recipes, Prooftide plugin, selection feedback, catalogue backup and sync | The full loop: from the card to the client |
| **M5** | Public, documented plugin API, AI features, polish, documentation | Openness and ecosystem |

Each milestone must be usable on its own. The exact content of each will be refined when we get
to it.

## 9. Performance budgets [proposed, to be validated by measurement]

| Situation | Target |
| --- | --- |
| Scrolling a grid of 10,000 thumbnails | No perceptible stutter |
| Showing a photo from the preview cache | Under 100 ms |
| Visual feedback for a development adjustment | Under 50 ms on a 24 MP image, standard GPU |
| Opening the catalogue cold with 100,000 photos | A few seconds |
| Search across 100,000 photos | Under 200 ms |

## 10. Open questions

1. **Catalogue backup and sync**: the workspace is plain files, so a file-sync tool can carry it
   between machines. How does the second machine's local database notice and absorb changes
   (rescan by modification time and fingerprint)? Is built-in sync wanted, or only documented
   compatibility with such tools?
2. **Workspace layout and default location**: mirror of the source folder tree, or one folder per
   photo? Human-readable names plus a short identifier, or identifiers only? Where does a new
   workspace go by default?
3. **Format of the catalogue-only state** written into the workspace (collections, smart
   collections, sources, gallery publications).
4. **Format of the version sidecar**: pure XMP with an Auroraw namespace, or XMP plus a companion
   file for the history.
5. **Automatic XMP mirroring** into the source folders, in addition to the on-demand export
   (D-024): wanted or not?
6. **Content fingerprint**: what is hashed (whole file or head, tail and size) so relinking and
   sidecar association stay fast on network shares without false matches.
7. **Workspace unavailable** (for example on a network share that is down): read-only mode, or
   queue the edits?
8. **Moving photos between catalogues** with their versions and metadata: copying sidecars
   between workspaces is probably enough. Confirm the wish for it, and its milestone.
9. **Offline development**: reconsider after the pipeline exists (M2 or later). A proxy would be
   reliable for colour, tone and geometry, but not for operations that depend on pixel scale:
   sharpening, noise reduction, local contrast, retouching, lens corrections at the edges. It
   could be offered for the reliable subset, with a warning.
10. **Prooftide API for third-party clients**: status, stability and terms of use; do plugins need
    an access key?
11. **Plugin model**: language, isolation, distribution, compatibility with GPL-3.0.
12. **Importing settings** from other software (Lightroom, darktable): useful, and how far?
13. **Dependency licenses**: compatibility with GPL-3.0 (RAW libraries, Lensfun and its database,
    AI models).
14. **Import details**: default destination and renaming templates and their tokens, what happens
    with cards from several cameras or with clashing file numbers, time zone handling for GPX,
    and how card insertion is detected on each operating system.
15. **Culling details**: how an objective series is detected (metadata rules, default time gap),
    how visual similarity is computed (perceptual hashes or learned embeddings, and the cost on
    100,000 photos), which key bindings, how a series' cover is chosen, and which location a
    multi-location photo prefers (online first, then fastest).
16. **Duplicates with different metadata**: when two copies of the same file carry different XMP
    (ratings, keywords) found at import, how are they merged?
17. **Pipeline definition**: exact stages and data spaces, the ordering-constraint language, how a
    plugin's shader is packaged, and how conflicts between plugins are shown to the user.
18. **Auto-sync**: how it behaves with photos that already have different settings for the same
    tool (overwrite, or only relative changes such as an exposure delta)?
19. **Main version and metadata**: the grid shows the main version's effective rating; confirm
    this is what the photographer expects when sorting a catalogue by rating.
20. **History size**: brush strokes and masks can make histories large; what does "compact"
    keep, and at what point is it proposed automatically?

## 11. Next steps

1. Detail each domain of §5 with usage scenarios (in this order: sources and catalogue, culling,
   development and versions, metadata, export and Prooftide, plugins, AI).
2. Define the development process (architecture, technology stack, testing, continuous
   integration, open source governance).
3. Plan milestone M1.
