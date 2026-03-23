/**
 * Core data types for yap-orange
 * Matches the Rust server models
 */

// UUIDv7 represented as string in JSON
export type Uuid = string

// JSONB properties stored as Record
export type Properties = Record<string, unknown>

/**
 * Atom - The actual content storage (like filesystem inodes)
 * Contains the data itself. Multiple blocks can reference the same atom.
 */
export interface Atom {
  id: Uuid
  lineage_id: Uuid
  content_type: string
  content_template: string
  links: Uuid[]
  properties: Properties
  content_hash: string
  predecessor_id: string | null
  created_at: string
}

/**
 * Atom with rendered content (links resolved to paths)
 */
export interface RenderedAtom {
  id: Uuid
  lineage_id: Uuid
  content_type: string
  content: string  // Links rendered as [[path::to::block]]
  properties: Properties
  created_at: string
}

/**
 * Block - References to atoms in the hierarchy (like directory entries)
 * Defines position in namespace hierarchy.
 */
export interface Block {
  id: Uuid
  lineage_id: Uuid
  parent_id: Uuid | null
  namespace: string
  name: string
  position: string
  deleted_at: string | null
  created_at: string
}

/**
 * Block with its rendered atom content included
 */
export interface BlockWithContent extends Block {
  content: string  // Rendered content from atom
  content_type: string
  properties: Properties
}

/**
 * Edge - Non-hierarchical relationships between atoms
 * For relationships that aren't inline content links or parent-child.
 */
export interface Edge {
  id: Uuid
  from_lineage_id: Uuid
  to_lineage_id: Uuid
  edge_type: string
  properties: Properties
  deleted_at: string | null
  created_at: string
}

// ============================================
// Request/Response DTOs
// ============================================

export interface CreateBlockRequest {
  namespace: string
  name: string
  content: string
  content_type?: string
  properties?: Properties
  position?: string
  /** Direct parent block ID — if provided, namespace is ignored */
  parent_id?: string
}

export interface CreateBlockResponse {
  block_id: Uuid
  lineage_id: Uuid
  namespace: string
  name: string
}

export interface UpdateBlockRequest {
  name?: string
  position?: string
}

export interface MoveBlockRequest {
  parent_id: Uuid | null
  position?: string
}

export interface UpdateAtomRequest {
  content: string
  content_type?: string
  properties?: Properties
}

export interface CreateEdgeRequest {
  from_lineage_id: Uuid
  to_lineage_id: Uuid
  edge_type: string
  properties?: Properties
}

export interface ResolveRequest {
  path: string
  from_namespace?: string
}

export interface ResolveResponse {
  lineage_id: Uuid
  block_id: Uuid
  namespace: string
}

export interface HealthResponse {
  status: string
  database: string
}

export interface Backlink {
  lineage_id: Uuid
  content: string
  content_type: string
  namespace: string | null
}

export interface EdgeResponse {
  id: Uuid
  from_lineage_id: Uuid
  to_lineage_id: Uuid
  edge_type: string
  properties: Properties
  created_at: string
}

export interface EdgesResponse {
  outgoing: EdgeResponse[]
  incoming: EdgeResponse[]
}

export interface AtomRenderedResponse {
  id: Uuid
  lineage_id: Uuid
  content_type: string
  content: string
  properties: Properties
  created_at: string
}

export interface HardLink {
  block_id: Uuid
  namespace: string
  name: string
}

export interface AtomGraph {
  atom: AtomRenderedResponse
  backlinks: Backlink[]
  outlinks: Backlink[]
  edges: EdgesResponse
  hard_links?: HardLink[]
}

export interface ContentLinkResponse {
  from_lineage_id: Uuid
  to_lineage_id: Uuid
}

export interface SubtreeGraphResponse {
  content_links: ContentLinkResponse[]
  edges: EdgeResponse[]
}

export interface ApiErrorResponse {
  error: string
}

// ============================================
// Custom Types / Schema System
// ============================================

/**
 * A field definition in a schema
 */
export interface SchemaField {
  name: string
  type: 'string' | 'number' | 'boolean' | 'date' | 'enum' | 'ref' | 'text'
  options?: string[]        // For enum fields
  target_type?: string      // For ref fields: the entry type name to reference
  required?: boolean
}

/**
 * A schema block (content_type = "schema")
 * Stored under types::<name> namespace
 */
export interface Schema {
  block_id: Uuid
  lineage_id: Uuid
  namespace: string
  name: string
  version: number
  fields: SchemaField[]
  content: string
}


// ============================================
// Benchmark Types
// ============================================

/** Result for a single benchmark within a suite. */
export interface BenchmarkResult {
  name: string
  duration_ms: number
  ops: number
  ops_per_sec: number
  metadata: Record<string, unknown>
}

/** Results for a single benchmark suite. */
export interface SuiteResult {
  name: string
  description: string
  duration_ms: number
  benchmarks: BenchmarkResult[]
}

/** Top-level benchmark run results. */
export interface BenchmarkResults {
  started_at: string
  completed_at: string
  total_duration_ms: number
  suites: SuiteResult[]
}
