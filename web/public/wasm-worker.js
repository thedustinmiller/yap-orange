/**
 * Dedicated Worker for the yap-orange WASM backend.
 *
 * Runs the Axum router + SQLite inside a Dedicated Web Worker so that
 * the OPFS SyncAccessHandle API is available (it requires a dedicated
 * worker context — not available in Service Workers or the main thread).
 *
 * Communication protocol:
 *   Main thread sends:  { id, method, url, body }
 *   Worker responds:    { id, result }  or  { id, error }
 *   Worker sends:       { type: 'ready' }  when initialization is complete
 */

import init, {
  init as initApp,
  handle_request,
  factory_reset,
} from './wasm/yap_server_wasm.js';

let initialized = false;

async function startup() {
  try {
    await init();      // Load the WASM module
    await initApp();   // Install OPFS VFS, open SQLite, run migrations, build router
    initialized = true;
    self.postMessage({ type: 'ready' });
    console.log('[yap-worker] WASM backend initialized');
  } catch (e) {
    console.error('[yap-worker] Init failed:', e);
    self.postMessage({ type: 'error', error: String(e) });
  }
}

// Start initialization immediately
startup();

// Handle requests from the main thread
self.onmessage = async (event) => {
  const { id, method, url, body, type } = event.data;

  // Factory reset: clear all data, re-migrate, re-bootstrap with seeds
  if (type === 'factory_reset') {
    try {
      await factory_reset();
      self.postMessage({ id, result: 'ok' });
    } catch (e) {
      console.error('[yap-worker] Factory reset error:', e);
      self.postMessage({ id, error: String(e) });
    }
    return;
  }

  if (!initialized) {
    self.postMessage({
      id,
      error: 'WASM backend not yet initialized',
    });
    return;
  }

  try {
    const resultJson = await handle_request(method, url, body || '');
    self.postMessage({ id, result: resultJson });
  } catch (e) {
    console.error('[yap-worker] Request error:', e);
    self.postMessage({ id, error: String(e) });
  }
};
