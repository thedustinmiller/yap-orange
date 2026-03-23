<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { EditorView, keymap, tooltips } from '@codemirror/view';
  import { EditorState, Prec } from '@codemirror/state';
  import { defaultKeymap, history, historyKeymap, insertNewlineAndIndent } from '@codemirror/commands';
  import { markdown } from '@codemirror/lang-markdown';
  import { completionStatus } from '@codemirror/autocomplete';
  import { appState, enterNavigationMode, navigateToLink } from './appState.svelte';
  import { yapTheme } from './editor/theme';
  import { wikiLinks } from './editor/wikilinks';
  import { wikiLinkCompletion } from './editor/completion';

  let {
    initialContent,
    blockId,
    initialCursorPosition = 'end',
    onSave,
    onNavigateToBlock,
    onCreateBlock,
    onIndent,
    onOutdent,
  }: {
    initialContent: string;
    blockId: string;
    initialCursorPosition?: 'start' | 'end';
    onSave: (content: string) => void;
    onNavigateToBlock: (direction: 'prev' | 'next') => void;
    onCreateBlock?: () => void;
    onIndent?: () => void;
    onOutdent?: () => void;
  } = $props();

  let containerEl: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined;
  let savedExplicitly = false;

  function saveAndExit(v: EditorView) {
    savedExplicitly = true;
    appState.hasUnsavedChanges = false;
    onSave(v.state.doc.toString());
    enterNavigationMode(blockId);
  }

  onMount(() => {
    if (!containerEl) return;

    const appKeybindings = Prec.highest(
      keymap.of([
        {
          key: 'Escape',
          run: (v) => {
            saveAndExit(v);
            return true;
          },
        },
        {
          key: 'Mod-Enter',
          run: (v) => {
            saveAndExit(v);
            return true;
          },
        },
        {
          key: 'Enter',
          run: (v) => {
            // Don't steal Enter from autocomplete
            if (completionStatus(v.state) === 'active') return false;
            if (onCreateBlock) {
              savedExplicitly = true;
              onSave(v.state.doc.toString());
              onCreateBlock();
              return true;
            }
            return false;
          },
        },
        {
          key: 'Shift-Enter',
          run: (v) => {
            // Insert newline (what Enter normally does in CM6)
            return insertNewlineAndIndent(v);
          },
        },
        {
          key: 'Tab',
          run: (v) => {
            if (onIndent) {
              savedExplicitly = true;
              onSave(v.state.doc.toString());
              onIndent();
              return true;
            }
            return false;
          },
        },
        {
          key: 'Shift-Tab',
          run: (v) => {
            if (onOutdent) {
              savedExplicitly = true;
              onSave(v.state.doc.toString());
              onOutdent();
              return true;
            }
            return false;
          },
        },
        {
          key: 'ArrowUp',
          run: (v) => {
            const line = v.state.doc.lineAt(v.state.selection.main.head);
            if (line.number === 1) {
              savedExplicitly = true;
              onSave(v.state.doc.toString());
              onNavigateToBlock('prev');
              return true;
            }
            return false; // Let CM6 handle normal cursor movement
          },
        },
        {
          key: 'ArrowDown',
          run: (v) => {
            const line = v.state.doc.lineAt(v.state.selection.main.head);
            if (line.number === v.state.doc.lines) {
              savedExplicitly = true;
              onSave(v.state.doc.toString());
              onNavigateToBlock('next');
              return true;
            }
            return false; // Let CM6 handle normal cursor movement
          },
        },
      ]),
    );

    const state = EditorState.create({
      doc: initialContent,
      extensions: [
        appKeybindings,
        yapTheme,
        history(),
        keymap.of([...defaultKeymap, ...historyKeymap]),
        markdown(),
        wikiLinks((path) => {
          // Save before navigating so content isn't lost
          savedExplicitly = true;
          if (view) {
            onSave(view.state.doc.toString());
          }
          navigateToLink(path);
        }),
        wikiLinkCompletion(),
        tooltips({ position: 'fixed' }),
        EditorView.lineWrapping,
        EditorView.contentAttributes.of({ 'aria-label': 'Block content editor' }),
        // Track doc changes for dirty-state warning
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            appState.hasUnsavedChanges = true;
          }
          // Auto-save on blur — save content but do NOT transition modes
          // (the thing that caused the blur handles its own transition)
          if (update.focusChanged && !update.view.hasFocus && !savedExplicitly) {
            appState.hasUnsavedChanges = false;
            onSave(update.view.state.doc.toString());
          }
        }),
      ],
    });

    view = new EditorView({ state, parent: containerEl });

    // Focus and place cursor based on hint
    view.focus();
    const pos = initialCursorPosition === 'start' ? 0 : view.state.doc.length;
    view.dispatch({
      selection: { anchor: pos },
    });
  });

  onDestroy(() => {
    appState.hasUnsavedChanges = false;
    view?.destroy();
    view = undefined;
  });
</script>

<div class="block-editor">
  <div class="editor-wrapper">
    <div class="editor-container" bind:this={containerEl}></div>
  </div>
</div>

<style>
  .block-editor {
    flex: 1;
    min-width: 0;
  }

  .editor-wrapper {
    border: 1px solid var(--accent-color);
    border-radius: 3px;
    display: flex;
    flex-direction: column;
  }

  .editor-wrapper:focus-within {
    border-color: var(--accent-bright);
    box-shadow: 0 0 0 2px var(--accent-glow);
  }

  .editor-container {
    overflow: visible;
  }

  /* Wiki link widget styling (injected by CM6 into the editor) */
  :global(.cm-wiki-link) {
    color: var(--link-color);
    cursor: pointer;
    border-bottom: 1px solid transparent;
    transition: border-color 0.1s;
    padding: 0 1px;
  }

  :global(.cm-wiki-link:hover) {
    border-bottom-color: var(--link-color);
  }

  /* Mark decoration when cursor is inside a wiki link */
  :global(.cm-wiki-link-editing) {
    color: var(--link-color);
  }

  /* Embed widget styling (injected by CM6 into the editor) */
  :global(.cm-embed-widget) {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    background: var(--bg-tertiary, #24283b);
    color: var(--text-muted, #7a85b8);
    border-radius: 3px;
    padding: 0 5px;
    font-size: 0.9em;
    cursor: pointer;
    vertical-align: baseline;
  }

  :global(.cm-embed-widget::before) {
    content: '\229E'; /* ⊞ */
    font-size: 0.85em;
    opacity: 0.6;
  }

  :global(.cm-embed-widget:hover) {
    background: var(--bg-active, #292e42);
    color: var(--text-primary, #ccc);
  }
</style>
