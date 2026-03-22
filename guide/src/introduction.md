# Introduction

yap-orange is a note-taking system built for people who care about structure. It organizes notes into a flexible hierarchy of namespaces -- like `research::ml::attention` or `projects::yap-orange::design` -- while also supporting graph-style links between any two notes, regardless of where they sit in the tree.

## Why yap-orange?

Most note-taking tools force a choice: either you get a neat folder tree, or you get a flat pool of interlinked pages. yap-orange gives you both, with a few properties that set it apart.

### Links that survive moves

Every note has a stable identity called a **lineage ID**. When you create a link like `[[research::ml::attention]]`, yap-orange resolves it to the target's lineage ID and stores that. If you later move the note to `archive::ml::attention`, every link pointing to it still works. No broken references, no manual cleanup.

### Immutable content history

Edits don't overwrite content. Each save creates a new **atom** -- an immutable snapshot of the note's content at that point in time. The note's lineage pointer advances to the latest atom, but every previous version is preserved. You get full history without any extra version-control ceremony.

### Flexible hierarchy

The namespace tree is built from **blocks** that reference lineages, arranged by parent-child relationships. There are no fixed "notebooks" or "folders" -- any block can have children, and you can restructure the tree freely. Paths like `research::ml::attention` are computed dynamically from the parent chain, not stored as strings.

### Graph relationships

Beyond the tree, **edges** let you create typed, non-hierarchical relationships between notes. Backlinks are tracked automatically, so you can always see what links to a given note.

## Three ways to run it

yap-orange uses the same data model everywhere, but offers three deployment modes to fit different workflows:

- **Web + Server** -- A Rust (Axum) API server backed by PostgreSQL, with a Svelte 5 frontend. Good for teams or anyone who wants a traditional client-server setup.
- **Desktop App** -- A Tauri 2 application that embeds its own PostgreSQL instance. No Docker, no external database -- just launch and go.
- **Browser SPA** -- The full server compiled to WebAssembly, running entirely in your browser with SQLite and OPFS for persistence. No server needed at all.

All three modes share the same core library (`yap-core`), the same API surface, and the same Svelte frontend. Notes created in one mode can be exported and imported into another.

A **CLI** is also available for scripting and automation. It talks to the server over HTTP.

## Who is this for?

yap-orange is built for developers, researchers, and knowledge workers who want:

- Structured notes that can be reorganized without breaking links
- Full content history without manual snapshots
- A system that works offline (desktop or browser SPA) or as a shared server
- Programmable access through a CLI and HTTP API

## What this guide covers

This guide walks through:

1. **Installation** -- Setting up yap-orange in each deployment mode
2. **Core concepts** -- The data model (atoms, lineages, blocks, edges) and how they fit together
3. **Web UI** -- Navigating the outliner, editor, graph view, and settings
4. **CLI** -- Managing notes, running queries, and scripting with the command-line tool

If you just want to try it quickly, the Browser SPA mode requires no server at all -- skip ahead to the [Installation](./installation.md) chapter.
