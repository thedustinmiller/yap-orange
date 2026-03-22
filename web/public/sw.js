/**
 * Service Worker for yap-orange SPA mode.
 *
 * Intercepts /api/* and /health requests and routes them through the
 * in-WASM Axum router backed by SQLite + OPFS persistence.
 * Non-API requests (static assets) pass through to the network.
 */

import init, {
  init as initApp,
  handle_request,
} from './wasm/yap_server_wasm.js';

let initialized = false;
let initPromise = null;

async function startup() {
  await init();     // Load the WASM module
  await initApp();  // Install OPFS VFS, open SQLite, run migrations, build router
  initialized = true;
  console.log('[yap-sw] WASM backend initialized');
}

self.addEventListener('install', (event) => {
  initPromise = startup();
  event.waitUntil(initPromise);
  self.skipWaiting();
});

self.addEventListener('activate', (event) => {
  event.waitUntil(self.clients.claim());
});

self.addEventListener('fetch', (event) => {
  const url = new URL(event.request.url);

  // Only intercept API requests — let static assets pass through
  if (!url.pathname.startsWith('/api/') && url.pathname !== '/health') {
    return;
  }

  event.respondWith(handleApiRequest(event.request));
});

async function handleApiRequest(request) {
  // Wait for init if still in progress
  if (!initialized && initPromise) {
    try {
      await initPromise;
    } catch (e) {
      return new Response(JSON.stringify({ error: 'WASM init failed: ' + e }), {
        status: 503,
        headers: { 'content-type': 'application/json' },
      });
    }
  }

  if (!initialized) {
    return new Response(JSON.stringify({ error: 'Service worker not initialized' }), {
      status: 503,
      headers: { 'content-type': 'application/json' },
    });
  }

  try {
    const url = new URL(request.url);
    const body = await request.text();
    const resultJson = await handle_request(
      request.method,
      url.pathname + url.search,
      body
    );
    const result = JSON.parse(resultJson);

    return new Response(result.body, {
      status: result.status,
      headers: result.headers,
    });
  } catch (e) {
    console.error('[yap-sw] Request handling error:', e);
    return new Response(JSON.stringify({ error: String(e) }), {
      status: 500,
      headers: { 'content-type': 'application/json' },
    });
  }
}
