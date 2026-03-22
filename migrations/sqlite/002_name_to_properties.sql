-- Move block names into atom properties.
-- SQLite version: uses json_set and table rebuild (no ALTER TABLE DROP COLUMN).

-- Copy names into atom properties (only where name key doesn't already exist)
UPDATE atoms SET properties = json_set(properties, '$.name', (
  SELECT b.name FROM blocks b
  JOIN lineages l ON b.lineage_id = l.id WHERE l.current_id = atoms.id
)) WHERE EXISTS (
  SELECT 1 FROM blocks b JOIN lineages l ON b.lineage_id = l.id
  WHERE l.current_id = atoms.id
) AND json_extract(properties, '$.name') IS NULL;

-- Rebuild blocks without name column
CREATE TABLE blocks_new (
    id TEXT PRIMARY KEY,
    lineage_id TEXT NOT NULL REFERENCES lineages(id),
    parent_id TEXT REFERENCES blocks_new(id),
    position TEXT NOT NULL DEFAULT '80',
    deleted_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT INTO blocks_new SELECT id, lineage_id, parent_id, position, deleted_at, created_at FROM blocks;
DROP TABLE blocks;
ALTER TABLE blocks_new RENAME TO blocks;
CREATE INDEX idx_blocks_parent_id ON blocks(parent_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_blocks_lineage_id ON blocks(lineage_id) WHERE deleted_at IS NULL;
