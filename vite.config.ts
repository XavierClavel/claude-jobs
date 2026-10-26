import react from '@vitejs/plugin-react'
import type { Plugin } from 'vite'
import { defineConfig } from 'vitest/config'

// `tauri dev` does not apply the CSP of `tauri.conf.json`; the same policy on the dev server keeps an external
// source from slipping in unnoticed. The two extras are what Vite itself needs: its reload socket and inline scripts.
function devCsp(): Plugin {
  return {
    name: 'dev-csp',
    configureServer(server) {
      server.middlewares.use((_req, res, next) => {
        res.setHeader(
          'Content-Security-Policy',
          "default-src 'self'; connect-src 'self' ipc: http://ipc.localhost ws://localhost:5174; img-src 'self' data:; style-src 'self' 'unsafe-inline'; font-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval'",
        )
        next()
      })
    },
  }
}

export default defineConfig({
  plugins: [react(), devCsp()],
  clearScreen: false,
  server: { port: 5174, strictPort: true, watch: { ignored: ['**/src-tauri/**'] } },
  build: { target: 'safari16.4', cssTarget: 'safari16.4' },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
  },
})
