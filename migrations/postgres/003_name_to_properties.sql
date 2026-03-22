-- Move block names into atom properties.
-- After this migration, the blocks table no longer has a name column;
-- names are stored as properties->>'name' on the current atom.

-- Copy block names into their atom's properties (skip atoms that already have a name key)
UPDATE atoms
SET properties = properties || jsonb_build_object('name', b.name)
FROM blocks b
JOIN lineages l ON b.lineage_id = l.id
WHERE atoms.id = l.current_id
  AND NOT (atoms.properties ? 'name');

-- Drop name-based unique indexes (they reference the name column)
DROP INDEX IF EXISTS idx_blocks_unique_parent_name;
DROP INDEX IF EXISTS idx_blocks_unique_root_name;

-- Drop the name column
ALTER TABLE blocks DROP COLUMN name;

-- Performance index for name lookups via properties
CREATE INDEX idx_atoms_prop_name ON atoms ((properties->>'name'));
