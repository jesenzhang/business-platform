import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

/**
 * Design-system guard (PLAN-0016 S5).
 *
 * Page content must be rendered by Astryx components, not by page-level global
 * CSS. `src/styles.css` is therefore limited to the console shell. This test
 * fails when a new class rule appears outside that allowlist, which is the usual
 * way a historical `.panel`-style stylesheet starts growing back.
 */

const SHELL_CLASSES = new Set([
  'app',
  'dark',
  'sidebar',
  'desktop-sidebar',
  'brand',
  'compact',
  'brand-mark',
  'main-content',
  'toolbar',
  'page-content',
  'mobile-bar',
  'mobile-bar-actions',
])

const stylesPath = resolve(process.cwd(), 'src/styles.css')
const layersPath = resolve(process.cwd(), 'src/layers.css')

const stripComments = (css: string) => css.replace(/\/\*[\s\S]*?\*\//g, '')

const classSelectors = (css: string) => {
  const matches = stripComments(css).matchAll(/\.([a-zA-Z][\w-]*)/g)
  return [...new Set(Array.from(matches, (match) => match[1]))].sort()
}

describe('console stylesheet boundary', () => {
  it('defines only shell classes', () => {
    const defined = classSelectors(readFileSync(stylesPath, 'utf8'))
    const unexpected = defined.filter((name) => !SHELL_CLASSES.has(name))
    expect(unexpected, `page-level CSS leaked into styles.css: ${unexpected.join(', ')}`).toEqual([])
  })

  it('declares the cascade order before any import', () => {
    const layers = stripComments(readFileSync(layersPath, 'utf8')).trim()
    expect(layers).toBe('@layer reset, tw-preflight, astryx-base, astryx-theme;')
  })

  it('does not load remote stylesheets', () => {
    for (const path of [stylesPath, layersPath]) {
      const css = readFileSync(path, 'utf8')
      expect(css, `${path} must not import a remote stylesheet`).not.toMatch(/@import\s+url\(/i)
      expect(css).not.toMatch(/fonts\.(googleapis|gstatic)\.com/i)
    }
  })
})
