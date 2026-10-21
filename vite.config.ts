import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Dev server for `tauri dev`; the port must match `build.devUrl` in src-tauri/tauri.conf.json.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 3000,
    strictPort: true,
    // Never watch the Rust side: a release build rewrites files under src-tauri/target, and on
    // Windows the watcher crashes on them (EBUSY). A regex matches both path separators; the
    // glob form didn't match Windows paths.
    watch: { ignored: [/[\\/]src-tauri[\\/]/] },
  },
})
