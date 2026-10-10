import { builtinModules } from 'node:module'
import { defineConfig } from 'vite'

/** The main process, as one Node build in out/main. Dependencies stay in node_modules. */
export default defineConfig({
  build: {
    ssr: true,
    target: 'node22',
    outDir: 'out/main',
    emptyOutDir: true,
    sourcemap: true,
    rolldownOptions: {
      input: { index: 'src/main/index.ts' },
      external: ['electron', ...builtinModules, ...builtinModules.map((name) => `node:${name}`)],
      output: { format: 'es', entryFileNames: '[name].js', chunkFileNames: 'chunks/[name]-[hash].js' },
    },
  },
})
