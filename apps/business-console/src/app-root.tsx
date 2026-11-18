import { Theme } from '@astryxdesign/core/theme'
import App from './App'
import { defaultThemeSlug, themes } from './astryx-themes'
import { useConsoleStore } from './store'

/**
 * Single design-system boundary for the console.
 *
 * The light/dark mode is the console's own Zustand state
 * (`business-console-theme` in localStorage); Astryx only consumes it, so the
 * legacy `.app.dark` shell and Astryx components can never disagree.
 *
 * The boundary is the document root on purpose: Astryx portals (dialog, toast,
 * tooltip) render outside the React tree that mounts them, and only a root
 * `Theme` synchronises `data-astryx-theme`/`data-theme` onto `<html>` so those
 * portals inherit the token set.
 *
 * Legacy surfaces keep their pre-Astryx typography through the demotion rules
 * in `styles.css` (reset layer); Astryx component typography lives in the later
 * `astryx-base` layer, so migrated components still win.
 */
export function AppRoot() {
  const theme = useConsoleStore((state) => state.theme)
  return (
    <Theme theme={themes[defaultThemeSlug]} mode={theme}>
      <App />
    </Theme>
  )
}
