/*
 * Stylesheet load order is part of the Astryx/Tailwind 3 contract:
 *   1. layers.css        declares reset → tw-preflight → astryx-base → astryx-theme
 *   2. Astryx reset/base must load after the order exists, before app styles
 *   3. self-hosted font faces, then styles.css (layered preflight + shell)
 * Astryx theme CSS is imported by the generated `astryx-themes` module.
 *
 * Fonts are self-hosted through @fontsource so the console makes no runtime
 * request to a font CDN; the theme's own `fonts.css` (a Google Fonts @import)
 * is intentionally not imported — see `astryx-themes.ts`.
 */
import './layers.css'
import '@astryxdesign/core/reset.css'
import '@astryxdesign/core/astryx.css'
import '@fontsource/figtree/400.css'
import '@fontsource/figtree/500.css'
import '@fontsource/figtree/600.css'
import '@fontsource/figtree/700.css'
import './styles.css'

import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { BrowserRouter } from 'react-router-dom'
import { AppRoot } from './app-root'

const queryClient = new QueryClient({ defaultOptions: { queries: { staleTime: 5_000, retry: 1 } } })

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <AppRoot />
      </BrowserRouter>
    </QueryClientProvider>
  </StrictMode>,
)
