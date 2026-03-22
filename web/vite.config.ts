import { createLogger, defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

// Suppress proxy ECONNREFUSED noise when running in SPA/WASM mode (no backend)
const logger = createLogger()
const _error = logger.error
logger.error = (msg, opts) => {
  if (typeof msg === 'string' && msg.includes('http proxy error')) return
  _error(msg, opts)
}

export default defineConfig({
  customLogger: logger,
  plugins: [svelte()],
  server: {
    port: 5173,
    proxy: {
      '/api': 'http://localhost:3000',
      '/health': 'http://localhost:3000',
    },
    headers: {
      // Required for SharedArrayBuffer (needed by some WASM sqlite builds)
      'Cross-Origin-Opener-Policy': 'same-origin',
      'Cross-Origin-Embedder-Policy': 'require-corp',
    },
  },
  optimizeDeps: {
    exclude: ['yap-server-wasm'],
  },
  build: {
    target: 'esnext',
  },
})
