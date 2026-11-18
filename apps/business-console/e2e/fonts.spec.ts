import { expect, test } from '@playwright/test'
import { mockConsoleApi } from './fixtures/console'

/**
 * Typography is part of the product surface, not a runtime dependency.
 *
 * The console self-hosts Figtree through @fontsource. This guard fails if the
 * generated Astryx theme module starts pulling a font CDN again (for example
 * after `astryx theme add`), which would reintroduce a third-party request and
 * make text metrics depend on network reachability.
 */

test('page loads with self-hosted Figtree and no font CDN request', async ({ page }) => {
  const externalRequests: string[] = []
  page.on('request', (request) => {
    const url = request.url()
    if (!url.startsWith('http://127.0.0.1') && !url.startsWith('http://localhost') && !url.startsWith('data:')) {
      externalRequests.push(url)
    }
  })

  await mockConsoleApi(page)
  await page.goto('/')
  await expect(page.getByRole('heading', { level: 1, name: 'Good morning, operator.' })).toBeVisible()
  await page.evaluate(() => document.fonts.ready)

  const cdn = externalRequests.filter((url) => /fonts\.(googleapis|gstatic)\.com/.test(url))
  expect(cdn, `font CDN requests must not happen: ${cdn.join(', ')}`).toEqual([])

  const figtreeAvailable = await page.evaluate(() => document.fonts.check('16px Figtree'))
  expect(figtreeAvailable, 'self-hosted Figtree face is registered').toBe(true)

  const headingFamily = await page.evaluate(() => {
    const heading = document.querySelector('h1')
    return heading ? getComputedStyle(heading).fontFamily : ''
  })
  expect(headingFamily).toContain('Figtree')
})
