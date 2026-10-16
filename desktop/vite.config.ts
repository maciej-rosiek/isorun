import { defineConfig } from 'vite'

export default defineConfig({
  build: {
    ssr: 'src/main.ts',
    outDir: 'dist/app',
    target: 'node22',
    emptyOutDir: true,
    rollupOptions: { external: ['electron'], output: { entryFileNames: 'main.js' } },
  },
  ssr: { noExternal: true },
})
