import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'
import { initApi } from './lib/api'
import { initWasmWorker } from './sw-register'

// In a Tauri window, initApi() discovers the embedded server's dynamic port
// and sets BASE_URL before any API calls are made. In a plain browser this
// is a no-op and the Vite proxy handles routing.
await initApi()

// In SPA mode (no backend server), start the WASM worker that runs the
// Axum router + SQLite with OPFS persistence in a Dedicated Web Worker.
// In Tauri or dev-with-server mode, this is a no-op.
await initWasmWorker()

const app = mount(App, {
  target: document.getElementById('app')!,
})

export default app
