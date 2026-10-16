import { defineConfig } from 'vite'
import { gamedataProxy } from './gamedata-proxy'
import { engineFiles } from './engine-files'
import { keepShell } from './shell'

export default defineConfig({
  plugins: [gamedataProxy(), engineFiles(), keepShell()],
  server: { port: 5173 },
  preview: { port: 4173 },
  build: { target: 'es2022', sourcemap: true },
})
