import adapter from '@sveltejs/adapter-static'
import { sveltekit } from '@sveltejs/kit/vite'
import { defineConfig } from 'vite'

/**
 * The renderer: a SvelteKit app, prerendered to static files that the main
 * process serves at erisreader://app/ (see src/main/app-scheme.ts).
 */
export default defineConfig({
  plugins: [
    sveltekit({
      adapter: adapter({ pages: 'out/renderer' }),
      csp: {
        mode: 'hash',
        directives: {
          'default-src': ['self'],
          // pdf.js decodes some images with WebAssembly.
          'script-src': ['self', 'wasm-unsafe-eval'],
          'worker-src': ['self'],
          // Svelte transitions set inline styles.
          'style-src': ['self', 'unsafe-inline'],
          'font-src': ['self', 'data:'],
          'img-src': ['self', 'data:', 'blob:'],
          'connect-src': ['self'],
        },
      },
    }),
  ],
  server: { host: '127.0.0.1', port: 5176, strictPort: true },
})
