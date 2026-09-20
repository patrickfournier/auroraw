CREATE TABLE camera(id INTEGER PRIMARY KEY, name TEXT NOT NULL);
CREATE TABLE lens(id INTEGER PRIMARY KEY, name TEXT NOT NULL);
CREATE TABLE source(id INTEGER PRIMARY KEY, path TEXT NOT NULL);
CREATE TABLE series(id INTEGER PRIMARY KEY, cover_photo_id INTEGER, resolved INTEGER NOT NULL DEFAULT 0);
CREATE TABLE photo(
  id INTEGER PRIMARY KEY,
  source_id INTEGER NOT NULL, path TEXT NOT NULL, filename TEXT NOT NULL, fingerprint BLOB NOT NULL,
  capture_time INTEGER NOT NULL, camera_id INTEGER, lens_id INTEGER,
  iso INTEGER, aperture REAL, shutter REAL, focal REAL, width INTEGER, height INTEGER,
  rating INTEGER NOT NULL DEFAULT 0, flag INTEGER NOT NULL DEFAULT 0, label INTEGER NOT NULL DEFAULT 0,
  caption TEXT, gps_lat REAL, gps_lon REAL,
  series_id INTEGER, stack_visible INTEGER NOT NULL DEFAULT 1,
  main_version_id INTEGER, version_count INTEGER NOT NULL DEFAULT 1,
  effective_rating INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE version(id INTEGER PRIMARY KEY, photo_id INTEGER NOT NULL, name TEXT NOT NULL,
  rating INTEGER, flag INTEGER, label INTEGER, updated INTEGER NOT NULL);
CREATE TABLE keyword(id INTEGER PRIMARY KEY, parent_id INTEGER, name TEXT NOT NULL, path TEXT NOT NULL, export INTEGER NOT NULL DEFAULT 1);
CREATE TABLE photo_keyword(photo_id INTEGER NOT NULL, keyword_id INTEGER NOT NULL, PRIMARY KEY(photo_id, keyword_id)) WITHOUT ROWID;
CREATE TABLE version_keyword(version_id INTEGER NOT NULL, keyword_id INTEGER NOT NULL, PRIMARY KEY(version_id, keyword_id)) WITHOUT ROWID;
CREATE TABLE collection(id INTEGER PRIMARY KEY, name TEXT NOT NULL, kind INTEGER NOT NULL);
CREATE TABLE collection_photo(collection_id INTEGER NOT NULL, photo_id INTEGER NOT NULL, PRIMARY KEY(collection_id, photo_id)) WITHOUT ROWID;
CREATE VIRTUAL TABLE photo_fts USING fts5(filename, caption, keywords, content='', tokenize='unicode61 remove_diacritics 2');
