# yap-orange

A notetaking and knowledge store tool.

## Quick Start

Test the app out directly in the browser here [https://thedustinmiller.github.io/yap-orange/#/tutorial](https://thedustinmiller.github.io/yap-orange/#/tutorial). This is a full version of the app, currently the feature sets are identical and import/export are compatible.

The fastest way to test from source is to clone down the repo and start up the WASM SPA.

```bash
cargo install
cd web
npm install
cd ../
cargo xtask run web
```
From there just visit localhost:5173.

## About
There are a few core ideas that make up this project, in no particular order
- Hierarchical note taking, very similar in experience to apps like Logseq or Obsidian
- Content is stored in a database with the tree structure derived on retrieval, rather than storing documents
- Wiki linking and arbitrary graph edges
- inode like metaphor and journaling for getting immutable, content addressible, but linkable content
- Support for user defined custom datatypes with configurable views
- Self describing; the settings page and type registry are themselves built from custom types
- Markdown for text content
- Import/export is easy, content addressible + Merkle based, and flexible
- Multiple equivalent front ends. CLI, OpenAPI/Swagger, Tauri desktop app, and WASM based SPA
- Accessible and keyboard navigable
- Multi editor and arbitrary configuration using Dockview
- Modal editor

## Terminology
For clarity, a quick glossary

### Core Mental Model
- Atom
    - The basic unit of content.
    - Has a content type, content itself, links, and JSON properties.
    - Immutable and content hashed.
- Lineage
    - A pointer to the most recent version of an atom.
    - Since atoms are immutable, 'editing' them actually creates a new one; lineage provides a stable reference.
- Block
    - Describes where in the tree lineages, and hence atoms, live.
    - Each block points to its parent; this is where the tree structure is reconstructed.
    - Uses fractional indexing to order siblings.
- Namespace
    - Implied or virtual in a sense, this is how users navigate the tree easily.
    - Names of blocks in the tree separated by two colons.
    - Lets you bookmark, navigate, and import/export ergonomically despite the indirection under the hood.

So the tree is made out of arrangements of blocks, each block points to a lineage, and a lineage points to an immutable atom, which can point to previous versions. Block to lineage is not a 1:1 relation; multiple blocks can point to the same lineage. The most obvious metaphor is *nix filesystem inodes and journaling.

### Typed content
- Type
    - An atom with a schema
    - Currently NOT enforced
    - However, gives a way of structuring and viewing content
    - Will be built on later
    - Schemas are versioned and immutable
    - Users can define their own compound types
- Entry
    - An instance of a type
    - Data stored as JSON object, no plain text content like an atom
    - Schema version is referenced by atom, so the data structure and view is always stable
- View
    - Several built in types, todo and schema, have custom views
    - Views can be matched to a specific version of the type
    - User defined custom types compose views automatically from each primitive having a view

This is mostly scaffolding for later, lacking querying and scripting, but currently usable for todo lists

## Linking
Type `[[` in the editor to trigger autocomplete.

| Syntax | Meaning |
|--------|---------|
| `[[research::ml::attention]]` | Absolute path |
| `[[./sibling]]` | Child of current namespace |
| `[[../uncle]]` | Sibling of current namespace |
| `[[..]]` | Parent namespace |
| `[["file name.md"]]` | Name with special characters |
| `[[foo::"my doc"::bar]]` | Quoted segment within path |

On saving to the database the path is resolved to a lineage and templated out, looking like `Template: {2}`. It's stored in the db like that, and the clients are responsible for retrieving and displaying the live content on retrieval.


## Typed entry instantiation
The convention used in the cli and web client is to create an entry via `@example_type{"key":"whatever value"}`. The core/server never knows about this: the clients parse that, define that block AS that content type, and set the properties to the JSON object.

## Web UI

The web UI is what most people will likely use most often. It has a few components worth naming or pointing out.
- Panels
    - Outliner: the main text editor that shows a tree of blocks. Currently has nav mode and edit mode, click/enter and esc to move between them.
    - Navigator: similar to the outliner, shows a tree of namespaces. Click to move the outliner there, or ctrl + click to open a new outliner window there.
    - Bookmarks: you can star or favorite namespaces for quick reference; this is a list of those.
    - Links: shows graph links, hard links, and wikilinks/backlinks
    - Properties: shows content type and the raw JSON of the selected block, editable.
    - Graph: Shows visually links or outliner tree.
    - Import/Export: import or export to/from json, with several options for how, what, and where to put data.


## Quick use
Don't want to memorize the syntax for a bunch of different platforms and clients across dev, testing, and build? I don't. Therefore, as much as possible is set up with cargo xtask in verb - noun format. After `cargo install`, run `cargo xtask` and you should see the verbs available. So for the most common dev setup, yap-server running with web ui, you'd just `cargo xtask run server` and `cargo xtask run web` and they'd boot their respective dev servers. Similarly for `build`, `test`, and some administrative `db` commands.


## AI
This project up to 0.1.0 is almost 100% AI generated, by lines of code at least. I tend to take the perspective that AI is a tool and the person must take responsibility for the product. Hence all commits are signed as me, personally, not an agent.

As for policy, be forthcoming about what's AI; otherwise quality of the material is paramount. If English isn't your preferred language, please feel free to write comments/issues/whatever in whichever language you're most comfortable with. I suspect machine translation will preserve more meaning and nuance while being less effort for everyone.

## Project Layout

```
crates/
├── yap-core/       # Core library, shared among all, assumes very little about clients and store
├── yap-store-pg/   # Postgres implementation of the store trait
├── yap-server/     # HTTP OpenAPI server, utoipa + axum
├── yap-desktop/    # Tauri 2 desktop, pg-embed + yap-server
├── yap-store-wasm/ # sqlite-wasm-rs and sqlite-wasm-vfs implementation of store trait
├── yap-server-wasm/# modified server to work in browser
└── yap-cli/        # CLI, connects to http server not using yap-core directly
web/                # Svelte 5, Dockview, CodeMirror, Vite
xtask/              # Convenience scripts
migrations/         # SQL migrations
fixtures/           # Stored exports, used for testing and default configs
```

## Further Reading

- [docs/architecture.md](./docs/architecture.md) - ...
- [docs/api.md](./docs/api.md) - catalogue of http endpoints and client parity
- [docs/export-format.md](./docs/export-format.md) - informal spec for import/export format
- [docs/testing.md](./docs/testing.md) - testing practices and guide
- [CONTRIBUTING.md](./CONTRIBUTING.md) - Contributing guide; thanks for your interest! :)
- [QUICKSTART.md](./QUICKSTART.md) - getting started quick, cheat sheets 

