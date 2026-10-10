import { defineConfig } from 'vite'

/**
 * The preload, as one CommonJS file in out/preload: a sandboxed preload is
 * a script, not a module, and can require nothing but Electron.
 */
export default defineConfig({
  build: {
    ssr: true,
    target: 'chrome140',
    outDir: 'out/preload',
    emptyOutDir: true,
    sourcemap: true,
    rolldownOptions: {
      input: { index: 'src/preload/index.ts' },
      external: ['electron'],
      output: { format: 'cjs', entryFileNames: '[name].cjs' },
    },
  },
})
