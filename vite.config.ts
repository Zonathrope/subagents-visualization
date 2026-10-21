import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Dev server for `tauri dev`; the port must match `build.devUrl` in src-tauri/tauri.conf.json.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 3000,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**'] },
  },
})
