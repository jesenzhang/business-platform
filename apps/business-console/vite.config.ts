import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'

/**
 * The Astryx neutral theme's font stylesheet is a Google Fonts `@import`.
 * The console self-hosts Figtree through @fontsource (see `src/main.tsx`), so
 * the generated theme module's import is redirected to an empty local file.
 * This keeps `astryx doctor` able to verify the generated theme module while the
 * built app makes no third-party font request; `e2e/fonts.spec.ts` guards it.
 */
const fontStylesheetOverride = fileURLToPath(new URL('./src/fonts-override.css', import.meta.url))

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: [{ find: '@astryxdesign/theme-neutral/fonts.css', replacement: fontStylesheetOverride }],
  },
  server: { port: 4173, host: '0.0.0.0' },
  preview: { port: 4173, host: '0.0.0.0' },
  test: {
    environment: 'jsdom',
    setupFiles: './tests/setup.ts',
    globals: true,
    include: ['tests/**/*.test.ts'],
  },
})
