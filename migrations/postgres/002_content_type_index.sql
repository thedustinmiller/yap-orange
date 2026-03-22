-- Add index on atoms.content_type for efficient type-based queries
CREATE INDEX IF NOT EXISTS idx_atoms_content_type ON atoms (content_type);
