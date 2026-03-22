-- yap-orange SQLite schema: immutable atoms with lineage pointers
-- Mirrors the PostgreSQL schema with SQLite-compatible types.
-- UUID columns → TEXT, TIMESTAMPTZ → TEXT, JSONB → TEXT + json_valid check,
-- atoms.links UUID[] replaced by atom_links junction table.

-- atoms: immutable content snapshots (append-only)
CREATE TABLE atoms (
    id TEXT PRIMARY KEY,
    content_type TEXT NOT NULL DEFAULT '',
    content_template TEXT NOT NULL DEFAULT '',
    properties TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(properties)),
    content_hash TEXT NOT NULL DEFAULT '',
    predecessor_id TEXT REFERENCES atoms(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- atom_links: junction table replacing UUID[] on atoms
CREATE TABLE atom_links (
    atom_id TEXT NOT NULL REFERENCES atoms(id),
    lineage_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    PRIMARY KEY (atom_id, position)
);
CREATE INDEX idx_atom_links_lineage ON atom_links(lineage_id);

-- lineages: mutable pointer to current atom snapshot (stable identity)
CREATE TABLE lineages (
    id TEXT PRIMARY KEY,
    current_id TEXT NOT NULL REFERENCES atoms(id),
    version INTEGER NOT NULL DEFAULT 1,
    deleted_at TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- blocks: hierarchy entries (directory entries)
CREATE TABLE blocks (
    id TEXT PRIMARY KEY,
    lineage_id TEXT NOT NULL REFERENCES lineages(id),
    parent_id TEXT REFERENCES blocks(id),
    name TEXT NOT NULL,
    position TEXT NOT NULL DEFAULT '80',
    deleted_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- edges: non-hierarchical relationships
CREATE TABLE edges (
    id TEXT PRIMARY KEY,
    from_lineage_id TEXT NOT NULL REFERENCES lineages(id),
    to_lineage_id TEXT NOT NULL REFERENCES lineages(id),
    edge_type TEXT NOT NULL,
    properties TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(properties)),
    deleted_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indexes on atoms
CREATE INDEX idx_atoms_content_hash ON atoms(content_hash);
CREATE INDEX idx_atoms_predecessor ON atoms(predecessor_id);
CREATE INDEX idx_atoms_content_type ON atoms(content_type);

-- Indexes on lineages
CREATE INDEX idx_lineages_current_id ON lineages(current_id);

-- Indexes on blocks (partial indexes supported in SQLite)
CREATE INDEX idx_blocks_parent_id ON blocks(parent_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_blocks_lineage_id ON blocks(lineage_id) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX idx_blocks_unique_parent_name ON blocks(parent_id, name) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX idx_blocks_unique_root_name ON blocks(name) WHERE parent_id IS NULL AND deleted_at IS NULL;

-- Indexes on edges
CREATE UNIQUE INDEX idx_edges_unique ON edges(from_lineage_id, to_lineage_id, edge_type) WHERE deleted_at IS NULL;
CREATE INDEX idx_edges_to ON edges(to_lineage_id) WHERE deleted_at IS NULL;
