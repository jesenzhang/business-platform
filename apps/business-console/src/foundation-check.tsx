import { useState } from 'react'
import { Button } from '@astryxdesign/core/Button'
import { Card } from '@astryxdesign/core/Card'
import { Heading } from '@astryxdesign/core/Heading'
import { Table } from '@astryxdesign/core/Table'
import { Text } from '@astryxdesign/core/Text'
import { TextInput } from '@astryxdesign/core/TextInput'
import { VStack } from '@astryxdesign/core/VStack'

/**
 * Foundation Check — dev-only fixture for cascade-layer verification.
 *
 * It is reachable only through the `import.meta.env.DEV` route in `App.tsx`,
 * because a broken layer order fails silently and identically on every page.
 * The Playwright spec `e2e/foundation.spec.ts` asserts computed styles here, so
 * an unlayered reset that zeroes component padding fails the build review.
 */
export default function FoundationCheck() {
  const [email, setEmail] = useState('')
  const rows: Array<Record<string, unknown>> = [
    { name: 'Foundation', status: 'ok' },
    { name: 'Cascade layers', status: 'ok' },
  ]
  return (
    <section data-foundation-check aria-label="Astryx foundation check">
      <VStack gap={4}>
        <Heading level={1}>Astryx foundation check</Heading>
        <Text>Primitives render with design-system padding, borders and surfaces.</Text>
        <section aria-label="Buttons">
          <Button label="Primary action" variant="primary" />
          <Button label="Secondary action" variant="secondary" />
          <Button label="Disabled action" isDisabled />
        </section>
        <section aria-label="Inputs">
          <TextInput
            label="Email"
            placeholder="you@example.com"
            value={email}
            onChange={setEmail}
          />
        </section>
        <Card>
          <VStack gap={1}>
            <Heading level={2}>Card surface</Heading>
            <Text>A card keeps its own background, border and internal spacing.</Text>
          </VStack>
        </Card>
        <Card variant="muted">
          <Heading level={2}>Muted card surface</Heading>
        </Card>
        <Table
          data={rows}
          columns={[
            { key: 'name', header: 'Name' },
            { key: 'status', header: 'Status' },
          ]}
        />
      </VStack>
    </section>
  )
}
