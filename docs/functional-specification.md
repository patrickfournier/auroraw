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
| **Version** | One development of a photo: its chain of operations, its history, its name. A photo has at least one version. Creating a version copies no pixels. |
| **Snapshot** | A named state of a version's history, which can be returned to or turned into a new version. |
| **Operation** | A non-destructive development step (exposure, curve, mask, etc.), parameterised and ordered in the chain. |
| **Style / preset** | A reusable set of operations and parameters, applicable to one or more versions. |
| **Collection** | A grouping chosen by the photographer (manual) or defined by a query (smart). |
| **Catalogue** | The local database indexing photos, their versions, their metadata and their previews. |
| **Export recipe** | A named, reusable setting for producing files (format, size, profile, metadata, naming, watermark, etc.). |

## 3. Guiding principles

1. **Originals are never modified** [decided, implicit in "non-destructive"].
2. **Nothing is trapped in the catalogue** [proposed]. The catalogue is an index and a working
   area that can be rebuilt. Metadata and versions are also written to open files next to the
   originals (see §5.4).
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

### 5.1 Sources and catalogue

- Local, network and offline sources [decided]. A photo whose source is offline stays visible,
  searchable and sortable thanks to cached previews and metadata; operations that need the
  original are shown as unavailable, not hidden [proposed].
- Tracking of file moves and renames (by content fingerprint) so photos and their versions are
  not lost [proposed].
- Catalogue backup and sync [decided, v1]: details to be designed (§10).
- Manual and smart collections, hierarchical tags, ratings [decided].
- Search, filter and sort on all metadata (EXIF, IPTC, tags, ratings, dates, lens, version,
  publication state, etc.) [decided].

### 5.2 Import

- Import from a memory card with renaming, duplicate backup and metadata application
  [decided, v1].
- Import of geolocation from GPX tracks, and a map of photos [decided, v1].
- Import of all RAW formats (through import plugins; the core bundles the most common ones) and
  of all image formats used in the photo and film industries [decided]. Video is excluded.

### 5.3 Culling, series and duplicates

- Fast keyboard culling: ratings, flags (picked, rejected), colour labels, side-by-side
  comparison, 100% loupe with sharpness aids [decided, v1].
- Duplicate detection and grouping into series (bursts, bracketing, RAW+JPEG) [decided, v1].
- A series collapses into one thumbnail; it is "resolved" by keeping one or several photos
  [proposed].
- Optional AI assistance: sharpness, closed eyes, best frame of a series [proposed].

### 5.4 Development versions

This is a central point [decided: versions, virtual copies and snapshots in v1].

- A photo can have as many versions as wanted ("high-contrast black and white", "client
  version", "cinema experiment"). A version is created from scratch, from another version, from
  a snapshot or from a style.
- Versions **share the original**, copy no pixels, and are displayed grouped under the photo;
  switching between them is a single gesture [proposed].
- Each version has its full history and its named snapshots [decided].
- Two versions can be compared side by side or with a wipe [proposed].
- Every output operation (export, publication) applies to a chosen version, or to a set of
  versions [proposed].

**Persistence of versions** [proposed]:

| Layer | Contains | Format |
| --- | --- | --- |
| Catalogue | Index, previews, queries, anything that speeds things up | Local database, rebuildable |
| Auroraw sidecar (next to the original) | All versions, histories and snapshots | Open, documented format, schema version in each file |
| Standard XMP | Portable metadata: keywords, ratings, captions, IPTC, rights | XMP, readable by other software |

The sidecar lets the catalogue be rebuilt from the disks. Development settings from other
software are not portable as-is; a partial import of common settings (Lightroom, darktable) is
conceivable later [open].

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
- In XMP the photo's value is written as-is; overrides live in the Auroraw sidecar.

### 5.6 Development (RAW, scans, images)

- Non-destructive RAW development with GPU acceleration [decided].
- Colour management [decided]: input profiles, wide floating-point working space, output
  profiles; soft proofing is planned for later but the architecture allows it.
- Basic operations [proposed]: exposure, white balance, curves, colour and tone, sharpening,
  noise reduction, crop and straighten.
- Masks and local retouching [decided, v1]: gradients, brushes, selection by luminosity and by
  colour; AI-assisted selections [proposed].
- Lens corrections through Lensfun [decided: v1, milestone M3].
- Presets and styles, with settings sync between images [decided, v1]: selective copy/paste,
  apply to a selection, named styles.
- Negative scans [decided, priority 2]:
  - v1: inversion, film-base (orange mask) sampling, per-channel curves, frame cropping.
  - Later: emulsion profiles, infrared dust detection, multi-exposure scans.
- Later [decided]: merges (HDR, panorama, focus stacking).

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
- Operation plugins must be able to run on the GPU.
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

1. **Catalogue backup and sync**: what is synced, between what (several machines of the same
   person?), through which mechanism?
2. **Auroraw sidecar**: one file per photo or one folder per source? Exact format.
3. **Prooftide API for third-party clients**: status, stability and terms of use; do plugins need
   an access key?
4. **Plugin model**: language, isolation, distribution, compatibility with GPL-3.0.
5. **Importing settings** from other software (Lightroom, darktable): useful, and how far?
6. **Dependency licenses**: compatibility with GPL-3.0 (RAW libraries, Lensfun and its database,
   AI models).

## 11. Next steps

1. Detail each domain of §5 with usage scenarios (in this order: sources and catalogue, culling,
   development and versions, metadata, export and Prooftide, plugins, AI).
2. Define the development process (architecture, technology stack, testing, continuous
   integration, open source governance).
3. Plan milestone M1.
