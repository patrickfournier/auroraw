-- Schema version 1 (architecture §5.4, design notes 001-004, M1 plan WP2).
--
-- The catalogue is an index, rebuildable from the workspace (D-026): every row here can be
-- recomputed from a photo sidecar, a version sidecar or a state file. The primary keys of
-- entities that have an identity of their own in the workspace (photo, version, keyword,
-- series, collection, source) are their hexadecimal identifiers as text, so a row can be found
-- or replaced without a lookup table. Camera and lens have no identity outside the catalogue:
-- they are strings deduplicated for filtering, and their integer ids are internal only.
--
-- `sidecar_size` and `sidecar_modified` are the file's size and modification time **as last
-- read**, local to this machine (design note 004 §6.2): the reconcile scan (architecture §5.3)
-- compares them with what it finds on disk to decide whether a sidecar needs to be re-read.

-- Catalogue-level facts: `workspace_id` (the workspace this catalogue indexes, so a mismatch
-- offers a rebuild instead of trusting stale data, note 001 §5.3) and `rebuilt_at`.
CREATE TABLE meta(
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE camera(
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE
);

CREATE TABLE lens(
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE
);

CREATE TABLE source(
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  name TEXT NOT NULL
);

CREATE TABLE keyword(
  id TEXT PRIMARY KEY,
  parent_id TEXT REFERENCES keyword(id),
  name TEXT NOT NULL,
  path TEXT NOT NULL,
  export INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE series(
  id TEXT PRIMARY KEY,
  cover_photo_id TEXT,
  kind TEXT NOT NULL,
  resolved INTEGER NOT NULL DEFAULT 0,
  sidecar_size INTEGER NOT NULL,
  sidecar_modified INTEGER
);

CREATE TABLE collection(
  id TEXT PRIMARY KEY,
  parent_id TEXT,
  name TEXT NOT NULL,
  kind TEXT NOT NULL,
  sidecar_size INTEGER NOT NULL,
  sidecar_modified INTEGER
);

CREATE TABLE collection_member(
  collection_id TEXT NOT NULL REFERENCES collection(id),
  position INTEGER NOT NULL,
  photo_id TEXT NOT NULL,
  version_id TEXT,
  PRIMARY KEY(collection_id, position)
) WITHOUT ROWID;

CREATE TABLE photo(
  id TEXT PRIMARY KEY,
  source_id TEXT,
  path TEXT,
  filename TEXT NOT NULL,
  fingerprint TEXT,
  capture_time INTEGER NOT NULL,
  camera_id INTEGER REFERENCES camera(id),
  lens_id INTEGER REFERENCES lens(id),
  iso INTEGER,
  aperture REAL,
  shutter REAL,
  focal_length REAL,
  width INTEGER,
  height INTEGER,
  rating INTEGER NOT NULL DEFAULT 0,
  flag INTEGER NOT NULL DEFAULT 0,
  label TEXT,
  title TEXT,
  caption TEXT,
  gps_lat REAL,
  gps_lon REAL,
  series_id TEXT REFERENCES series(id),
  main_version_id TEXT,
  version_count INTEGER NOT NULL DEFAULT 0,
  effective_rating INTEGER NOT NULL DEFAULT 0,
  effective_flag INTEGER NOT NULL DEFAULT 0,
  rating_overridden INTEGER NOT NULL DEFAULT 0,
  imported INTEGER,
  sidecar_size INTEGER NOT NULL,
  sidecar_modified INTEGER
);

CREATE TABLE version(
  id TEXT PRIMARY KEY,
  photo_id TEXT NOT NULL REFERENCES photo(id),
  name TEXT,
  rating INTEGER,
  flag INTEGER,
  label TEXT,
  created INTEGER,
  sidecar_size INTEGER NOT NULL,
  sidecar_modified INTEGER
);

CREATE TABLE photo_keyword(
  photo_id TEXT NOT NULL REFERENCES photo(id),
  keyword_id TEXT NOT NULL REFERENCES keyword(id),
  PRIMARY KEY(photo_id, keyword_id)
) WITHOUT ROWID;

CREATE TABLE version_keyword(
  version_id TEXT NOT NULL REFERENCES version(id),
  keyword_id TEXT NOT NULL REFERENCES keyword(id),
  PRIMARY KEY(version_id, keyword_id)
) WITHOUT ROWID;

-- Self-contained, not `content=''`: a contentless FTS5 index supports INSERT but not the plain
-- UPDATE/DELETE that keeping it in step with an edited photo needs (its own 'delete' command
-- requires the old column values, which a contentless table cannot give back). The duplicated
-- text costs little next to a photo's row and buys a normal SQL update path (crate::write).
CREATE VIRTUAL TABLE photo_fts USING fts5(
  filename, caption, title, keywords,
  tokenize='unicode61 remove_diacritics 2'
);
