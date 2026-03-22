# Custom Types and Schemas

yap-orange supports custom type definitions (schemas) that give structure to blocks. Schemas are themselves blocks stored under the `types` namespace. The `yap schema` subcommand manages these definitions, while typed instances are created using regular `yap block create` with `--type` and `--prop` flags.

## Create a Schema

Define a new type by providing its name and field definitions as a JSON array:

```bash
yap schema create person --fields '[
  {"name": "email", "type": "string", "required": true},
  {"name": "role", "type": "enum", "options": ["engineer", "manager", "designer"]},
  {"name": "team", "type": "string"}
]'
```

This creates a block at `types::person` with `content_type = "schema"` and the field definitions stored in its properties.

Each field object supports these keys:
- `name` -- field name (required)
- `type` -- field type: `string`, `number`, `boolean`, `enum`, `reference`, etc.
- `required` -- whether the field is mandatory (default: false)
- `options` -- array of allowed values (for `enum` type)
- `target_type` -- the referenced type name (for `reference` type)

Output:

```
Created schema: person
  Block ID:  019414a0-...
  Lineage:   019414a0-...
  Namespace: types::person
```

## List Schemas

View all defined schemas:

```bash
yap schema list
```

Output:

```
NAME                 VERSION  FIELDS
--------------------------------------------------
person               1        3 fields
project              1        5 fields
task                 1        4 fields
```

## Get a Schema

View the full definition of a specific schema:

```bash
yap schema get person
```

You can pass either the schema name or its full namespace path:

```bash
yap schema get types::person  # also works
```

Output:

```
Schema: person
  Namespace: types::person
  Version:   1
  Lineage:   019414a0-...

Fields:
  - email: string (required)
  - role: enum [engineer, manager, designer]
  - team: string
```

## Resolve a Schema

Schema resolution uses namespace walk-up: it searches `context::types::name`, then `parent::types::name`, up to the root `types::name`. This allows namespace-scoped type overrides.

```bash
# Resolve from a specific namespace context
yap schema resolve person --from research::ml

# Resolve from root (no --from)
yap schema resolve person
```

Output:

```
Resolved: person -> types::person
  Lineage:  019414a0-...
  Version:  1
  Fields:   3 defined
```

If `research::ml::types::person` existed, it would take precedence over the root-level `types::person` when resolving from the `research::ml` namespace.

## Creating Typed Instances

Once a schema exists, create instances using `yap block create` with the `--type` and `--prop` flags:

```bash
yap block create "Alice Smith" \
  --namespace people \
  --name alice \
  --type person \
  --prop '{"email": "alice@example.com", "role": "engineer", "team": "platform"}'
```

The `--type` flag sets the block's `content_type` and `--prop` provides the structured field values as a JSON object.

## Listing by Type

Find all instances of a given type:

```bash
yap block list --content-type person
```

Output:

```
people::alice  Alice Smith
people::bob  Bob Johnson
projects::team::carol  Carol Williams

3 block(s)
```

This queries blocks across all namespaces that have the specified `content_type`, so typed instances do not need to live in any particular namespace.
