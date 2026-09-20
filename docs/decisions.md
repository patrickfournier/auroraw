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
| D-007 | 2026-09-19 | **v1 scope**: fast culling, duplicates and series, versions, presets and styles, masks and local retouching, GPS, memory-card import, IPTC/XMP, catalogue backup and sync, batch export. Lens corrections through Lensfun: v1, in milestone M3. |
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
| D-018 | 2026-09-19 | **Originals are read-only** except at import: Auroraw does not modify, move, rename or delete them afterwards. Refined by D-022. |
| D-019 | 2026-09-19 | **Change detection**: online sources are monitored; new files are reported and added with one click after confirmation; moved files are relinked by content fingerprint. |
| D-020 | 2026-09-19 | **Non-writable sources**: sidecars are written next to the original when possible, otherwise versions live in the catalogue only, with a warning; sidecars can be exported on demand. *Superseded by D-022.* |
| D-021 | 2026-09-19 | **No development on offline sources in v1.** A reduced-resolution proxy is not trusted to give the same result as the original. To be reconsidered once the pipeline exists. |
| D-022 | 2026-09-19 | **Workspace**: a folder, used by a single catalogue, that receives all sidecars (of originals and of versions) and optionally exports. Auroraw writes nothing into source folders except at import and on explicit XMP export. Supersedes D-020. |
| D-023 | 2026-09-19 | **Sidecar hierarchy**: the sidecar of an original holds only that original's metadata; the sidecar of a version copies those metadata and adds its own. |
| D-024 | 2026-09-19 | **Exports and XMP compatibility**: exports may go into the workspace or elsewhere, and exporting into a source folder is discouraged with a warning. An option exports the originals' XMP files into the source folders, for compatibility with other software. |
