# Decision log

Decisions confirmed by Patrick, in order. A reversed decision is not deleted: it is marked
*superseded* and points to its replacement.

| # | Date | Decision |
| --- | --- | --- |
| D-001 | 2026-09-19 | **Audience**: digital photography is priority 1, film workflows are priority 2. |
| D-002 | 2026-09-19 | **Platforms**: Linux, Windows and macOS from the start. |
| D-003 | 2026-09-19 | **Video** is out of scope. Still images only. |
| D-004 | 2026-09-19 | **Collaboration**: single-user for now, no shared catalogue. |
| D-005 | 2026-09-19 | **Positioning**: simpler and more intuitive than darktable; at least on par with Lightroom in speed, simplicity and capabilities. |
| D-006 | 2026-09-19 | **Versions**: a photo can have several kept development versions; virtual copies, versions and snapshots are in v1. |
| D-007 | 2026-09-19 | **v1 scope**: fast culling, duplicates and series, versions, presets and styles, masks and local retouching, GPS, memory-card import, IPTC/XMP, catalogue backup and sync, batch export. *Backup and sync refined by D-064.* Lens corrections through Lensfun: v1, in milestone M3. |
| D-008 | 2026-09-19 | **Not in v1**: merges (HDR, panorama, focus stacking), soft proofing and printing, tethering. |
| D-009 | 2026-09-19 | **Prooftide, naming**: Auroraw generates a unique file name for each published version and keeps the mapping; no server change required. |
| D-010 | 2026-09-19 | **Prooftide, interoperability**: no interdependence. Auroraw stays compatible with other gallery services; Prooftide stays compatible with Lightroom and other workflows. Prooftide extensions for tighter integration with Auroraw are allowed if they are optional. |
| D-011 | 2026-09-19 | **Metadata**: ratings and keywords live on the photo, with an optional override on the version. |
| D-012 | 2026-09-19 | **Milestones**: M1 (catalogue and culling), M2 (basic development and versions), M3 (masks, lens, negatives), M4 (export, Prooftide, backup), M5 (public plugin API, AI, polish). |
| D-013 | 2026-09-19 | **License**: GPL-3.0. |
| D-014 | 2026-09-19 | **Keyword overrides**: in v1 a version can only add keywords to those of its photo, not remove them. |
| D-015 | 2026-09-19 | **Project language**: English for all repository documentation and code. Conversation with Patrick stays in French. |
| D-016 | 2026-09-19 | **Git workflow**: same rules as Prooftide. Work on `dev`, merge into `main` only at release. Claude Code commits as `Claude Code <claude-code@straycat.ca>`. |
| D-017 | 2026-09-19 | **Catalogues**: several catalogues, all equal, chosen at opening (typically separate areas of activity, such as family and professional). The application must be fully functional with a single catalogue. |
| D-018 | 2026-09-19 | **Originals are read-only** except at import: Auroraw does not modify, move, rename or delete them afterwards. Refined by D-022 and D-024. |
| D-019 | 2026-09-19 | **Change detection**: online sources are monitored; new files are reported and added with one click after confirmation; moved files are relinked by content fingerprint. |
| D-020 | 2026-09-19 | **Non-writable sources**: sidecars are written next to the original when possible, otherwise versions live in the catalogue only, with a warning; sidecars can be exported on demand. *Superseded by D-022.* |
| D-021 | 2026-09-19 | **No development on offline sources in v1.** A reduced-resolution proxy is not trusted to give the same result as the original. To be reconsidered once the pipeline exists. |
| D-022 | 2026-09-19 | **Workspace**: a folder, used by a single catalogue, that receives all sidecars (of originals and of versions) and optionally exports. Auroraw writes nothing into source folders except at import and on explicit XMP export. Supersedes D-020. |
| D-023 | 2026-09-19 | **Sidecar hierarchy**: the sidecar of an original holds only that original's metadata; the sidecar of a version copies those metadata and adds its own. |
| D-024 | 2026-09-19 | **Exports and XMP compatibility**: exports may go into the workspace or elsewhere, and exporting into a source folder is discouraged with a warning. An option exports the originals' XMP files into the source folders, for compatibility with other software. |
| D-025 | 2026-09-19 | **One workspace per catalogue**, exactly. |
| D-026 | 2026-09-19 | **Catalogue database is local** (user data folder) and rebuildable from the workspace and the sources. Everything that would otherwise live only in the database is also written into the workspace. |
| D-027 | 2026-09-19 | **Sidecar consistency**: when a photo's metadata changes, the copies in its version sidecars follow, except for the fields the version overrides. |
| D-028 | 2026-09-19 | **XMP naming** for the export to source folders is the photographer's choice (`photo.xmp` or `photo.ARW.xmp`), `photo.xmp` by default. |
| D-029 | 2026-09-19 | **Import profiles**: an import profile stores destination, naming, backup, metadata template and style; inserting a card offers a one-click import with a profile. Nothing starts automatically. |
| D-030 | 2026-09-19 | **Import everything**: no pre-selection at import; culling happens afterwards in the catalogue. |
| D-031 | 2026-09-19 | **The card is never modified**: Auroraw signals when the copies are verified and it is safe to erase, and deletes nothing. |
| D-032 | 2026-09-19 | **RAW+JPEG**: a pair is one photo with two files; the RAW is developed, the JPEG is a companion. A profile setting can ignore either kind. |
| D-033 | 2026-09-19 | **Cull mode**: a dedicated full-screen mode for culling, with the grid as the navigation view. |
| D-034 | 2026-09-19 | **Series formation**: objective series (bursts, bracketing) form automatically; visually similar photos are only suggested and the photographer confirms. |
| D-035 | 2026-09-19 | **Resolving a series** keeps the designated photos and gives the others the Rejected flag. The series stays expandable. Nothing is deleted. |
| D-036 | 2026-09-19 | **Exact duplicates** are one photo with several locations. An optional duplicates report helps the photographer tidy up; Auroraw itself deletes nothing. |
| D-037 | 2026-09-19 | **Develop interface**: guided panels by task, advanced tools added on demand, and a default pipeline that works out of the box. Operations are not reordered in everyday use. The pipeline must be configurable for advanced users and plugins, outside the everyday interface. |
| D-038 | 2026-09-19 | **Main version**: each photo has one main version (chosen by the photographer, by default the last edited) that supplies the grid thumbnail and the default for export and publication. Other versions are collapsed under the photo. |
| D-039 | 2026-09-19 | **Reusing settings**: selective copy and paste, styles, and an optional auto-sync switch. Application is one-off, with no lasting link between photos. |
| D-040 | 2026-09-19 | **One concept, the style**: a named set of settings on one or several tools; a tool preset is a single-tool style. |
| D-041 | 2026-09-19 | **Scene-referred core** (linear, floating point) with familiar controls exposed in the interface. |
| D-042 | 2026-09-19 | **Base look**: a style applied when a version is created; neutral by default, with flat linear among the choices. A look imitating the camera's embedded JPEG is a planned evolution. |
| D-043 | 2026-09-19 | **Local adjustments** are first-class objects (a mask plus its settings), listed, stackable and reorderable. |
| D-044 | 2026-09-19 | **Negatives, first inputs**: camera-scanned on a light table, dedicated film scanners, and third-party or lab scans. Flatbed files open as ordinary images. |
| D-045 | 2026-09-19 | **Keywords**: a hierarchical vocabulary with optional synonyms and a "do not export" flag per keyword, written into the workspace. |
| D-046 | 2026-09-19 | **Metadata in exports**: configurable per recipe with named settings; the default is everything except location and camera serial number. |
| D-047 | 2026-09-19 | **External XMP changes** are monitored like new files: reported, and accepted with one click after import. |
| D-048 | 2026-09-19 | **Geocoding**: a bundled offline database by default (GeoNames); finer online lookup only through an optional plugin, with consent. |
| D-049 | 2026-09-19 | **Publication tracking**: for every publication Auroraw records what was sent (photo, version, published name, date, settings). Modified photos are flagged; republishing is manual, never automatic. |
| D-050 | 2026-09-19 | **Client feedback**: each published gallery gets a "client selection" collection filled automatically, with an optional flag, label or keyword of the photographer's choice. Annotations are attached to the photo. |
| D-051 | 2026-09-19 | **Prooftide API**: the routes the plugin uses become a documented, versioned public API, usable by any client. |
| D-052 | 2026-09-19 | **Library scope**: export recipes, styles and import profiles live at user level, shared by all catalogues, and are importable and exportable as files. A publication keeps a copy of its recipe. |
| D-053 | 2026-09-19 | **Gallery revisions**: republishing a gallery creates a new revision (v2, v3...) rather than replacing photos. The whole collection is republished by default, with an option for only the modified and new photos. The revision is named after the first gallery with a suffix, such as "Marie wedding (v2)". |
| D-054 | 2026-09-19 | **Plugin distribution**: a catalogue built into the application, fed by a public open index with trust levels (official, verified, community), plus manual installation from a file or an address. No hosted store. |
| D-055 | 2026-09-19 | **Plugin trust**: sandboxed by default with declared permissions shown before installation; a native level is allowed, clearly flagged and needing explicit confirmation. |
| D-056 | 2026-09-19 | **Source plugins, first scenarios**: local disk folders and mounted cards (through the same interface), cameras and phones (PTP/MTP), and online storage. Scanner acquisition is not among the first scenarios. Importing from other software is kept for later. |
| D-057 | 2026-09-19 | **Plugin licensing is deferred** to the technical phase, together with the isolation model. Moving the core to the LGPL is an option kept open. |
| D-058 | 2026-09-19 | **First AI priorities (M5)**: selections for masks (subject, sky, people) and noise reduction, upscaling to follow. Assisted culling, semantic search and keyword suggestion are not prioritised for now. |
| D-059 | 2026-09-19 | **AI models** are downloaded on demand as packs through the plugin catalogue; the core ships none. |
| D-060 | 2026-09-19 | **AI without a capable GPU**: features run on the CPU, slowly, as background tasks with an estimated duration. |
| D-061 | 2026-09-19 | **Online AI**: the core calls no external AI service. Plugins may, off by default, with consent per plugin and a display of what leaves the machine. No telemetry. |
| D-062 | 2026-09-19 | **Minimal export in M2** (JPEG, TIFF, PNG; size, profile, metadata setting), so that development is usable on its own. Recipes, queue, watermark and publication stay in M4. |
| D-063 | 2026-09-19 | **Rating level**: Cull mode works on the photo's values, Develop on the version's values; the grid shows the effective value and marks a photo whose value is overridden. |
| D-064 | 2026-09-19 | **Backup and sync in v1**: a backup helper, rebuilding the catalogue from the workspace, and documented compatibility with file-sync tools. No built-in sync. |
| D-065 | 2026-09-19 | **XMP export to the source folders** ships in M1. |
| D-066 | 2026-09-19 | **No automatic XMP mirroring** in v1: the export to source folders is on request only. |
| D-067 | 2026-09-19 | **Moving photos between catalogues** is in v1, by copying sidecars between workspaces. Milestone M4 (confirmed). |
| D-068 | 2026-09-19 | **Core language**: Rust, for the catalogue, the pipeline and the plugin host. |
| D-069 | 2026-09-19 | **Interface priorities**: the performance and colour fidelity of the image view, and lightness (fast start, little memory). A web view (Tauri) is therefore not a candidate unless the native toolkits fail. |
| D-070 | 2026-09-19 | **The stack is fixed after short spikes** on four risks (the GPU pipeline, the image view and toolkit, the catalogue and grid, the plugin sandbox), on measurements. See technical-spikes.md. |
| D-071 | 2026-09-19 | **Test platforms**: Linux on the development machine, Windows by Patrick, macOS by Patrick over remote desktop. Continuous integration builds all three. Colour is judged on Linux and Windows, not over remote desktop. |
