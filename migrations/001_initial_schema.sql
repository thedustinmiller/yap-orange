-- yap-orange schema: immutable atoms with lineage pointers
-- Uses parent_id hierarchy (no ltree), fractional indexing for position

-- atoms: immutable content snapshots (append-only)
CREATE TABLE atoms (
    id UUID PRIMARY KEY,
    content_type TEXT NOT NULL DEFAULT '',
    content_template TEXT NOT NULL DEFAULT '',
    links UUID[] NOT NULL DEFAULT '{}',
    properties JSONB NOT NULL DEFAULT '{}',
    content_hash TEXT NOT NULL DEFAULT '',
    predecessor_id UUID REFERENCES atoms(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- lineages: mutable pointer to current atom snapshot (stable identity)
CREATE TABLE lineages (
    id UUID PRIMARY KEY,
    current_id UUID NOT NULL REFERENCES atoms(id),
    version INTEGER NOT NULL DEFAULT 1,
    deleted_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- blocks: hierarchy entries (directory entries)
CREATE TABLE blocks (
    id UUID PRIMARY KEY,
    lineage_id UUID NOT NULL REFERENCES lineages(id),
    parent_id UUID REFERENCES blocks(id),
    name TEXT NOT NULL,
    position TEXT NOT NULL DEFAULT '80',
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- edges: non-hierarchical relationships
CREATE TABLE edges (
    id UUID PRIMARY KEY,
    from_lineage_id UUID NOT NULL REFERENCES lineages(id),
    to_lineage_id UUID NOT NULL REFERENCES lineages(id),
    edge_type TEXT NOT NULL,
    properties JSONB NOT NULL DEFAULT '{}',
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Indexes on atoms (no partial predicate — atoms are immutable, no deleted_at)
CREATE INDEX idx_atoms_links ON atoms USING GIN (links);
CREATE INDEX idx_atoms_properties ON atoms USING GIN (properties);
CREATE INDEX idx_atoms_content_hash ON atoms (content_hash);
CREATE INDEX idx_atoms_predecessor ON atoms (predecessor_id);

-- Indexes on lineages
CREATE INDEX idx_lineages_current_id ON lineages (current_id);

-- Indexes on blocks
CREATE INDEX idx_blocks_parent_id ON blocks (parent_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_blocks_lineage_id ON blocks (lineage_id) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX idx_blocks_unique_parent_name ON blocks (parent_id, name) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX idx_blocks_unique_root_name ON blocks (name) WHERE parent_id IS NULL AND deleted_at IS NULL;

-- Indexes on edges
CREATE UNIQUE INDEX idx_edges_unique ON edges (from_lineage_id, to_lineage_id, edge_type) WHERE deleted_at IS NULL;
CREATE INDEX idx_edges_to ON edges (to_lineage_id) WHERE deleted_at IS NULL;

-- Trigger for lineages.updated_at
CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER lineages_updated_at
    BEFORE UPDATE ON lineages
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();
