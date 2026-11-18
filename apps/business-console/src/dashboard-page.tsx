import { useEffect, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import ReactECharts from 'echarts-for-react'
import { Badge } from '@astryxdesign/core/Badge'
import { Card } from '@astryxdesign/core/Card'
import { Grid } from '@astryxdesign/core/Grid'
import { HStack } from '@astryxdesign/core/HStack'
import { Heading } from '@astryxdesign/core/Heading'
import { Text } from '@astryxdesign/core/Text'
import { VStack } from '@astryxdesign/core/VStack'
import { Link } from '@astryxdesign/core/Link'
import { getOverview } from './api'
import { Empty, ErrorState, Loading, PageHeader } from './components'
import { AuditList, JobsTable } from './pages'
import { useConsoleStore } from './store'

/**
 * ECharts paints on a canvas, so it cannot read CSS custom properties: each
 * colour has to be a concrete value. Resolve the design-system tokens through a
 * hidden probe (which lets the browser evaluate `light-dark()` for the active
 * scheme) instead of hard-coding light and dark palettes in the page.
 */
function resolveChartColors() {
  const probe = document.createElement('span')
  probe.style.cssText = 'position:absolute;visibility:hidden;display:block'
  document.body.appendChild(probe)
  const read = (token: string, fallback: string) => {
    probe.style.color = `var(${token})`
    const value = getComputedStyle(probe).color
    return value && value !== 'rgba(0, 0, 0, 0)' ? value : fallback
  }
  const colors = {
    axis: read('--color-text-secondary', '#6e6e73'),
    grid: read('--color-border', 'rgba(0, 0, 0, 0.08)'),
  }
  probe.remove()
  return colors
}

/**
 * Dashboard lives in its own module because it is the only route that needs
 * ECharts. Route-level code splitting then keeps the chart engine out of every
 * other page's first load.
 */
export function DashboardPage() {
  const query = useQuery({ queryKey: ['overview'], queryFn: getOverview, refetchInterval: 10_000 })
  const theme = useConsoleStore((state) => state.theme)
  const [chartColors, setChartColors] = useState(resolveChartColors)
  useEffect(() => { setChartColors(resolveChartColors()) }, [theme])
  if (query.isPending) return <Loading />
  if (query.isError) return <ErrorState error={query.error} />
  const overview = query.data
  const statuses = Object.entries(overview.processing_by_status)
  const metrics = [
    { label: 'Documents', value: overview.document_total, detail: `+${overview.document_created_today} in the last day` },
    { label: 'Review queue', value: overview.review_pending, detail: 'Candidates awaiting a decision' },
    { label: 'Open findings', value: overview.unresolved_findings, detail: 'Integrity signals needing attention' },
    { label: 'Succeeded jobs', value: overview.processing_by_status.succeeded ?? 0, detail: 'Durable execution history' },
  ]
  const chartOption = {
    // Categorical status colours stay stable across modes: they carry the
    // chart's business meaning. Only the chrome follows the theme tokens.
    color: ['#0071e3', '#34c759', '#ff9f0a', '#af52de', '#ff375f'],
    tooltip: { trigger: 'axis' },
    grid: { left: 8, right: 16, top: 12, bottom: 24, containLabel: true },
    xAxis: { type: 'category', data: statuses.map(([key]) => key.replaceAll('_', ' ')), axisLabel: { color: chartColors.axis }, axisLine: { lineStyle: { color: chartColors.grid } } },
    yAxis: { type: 'value', minInterval: 1, splitLine: { lineStyle: { color: chartColors.grid } }, axisLabel: { color: chartColors.axis } },
    series: [{ type: 'bar', barWidth: 24, data: statuses.map(([, count]) => count), itemStyle: { borderRadius: [6, 6, 0, 0] } }],
  }
  return <>
    <PageHeader eyebrow="LIVE OPERATIONS" title="Good morning, operator." description="A calm view of document intelligence, processing health, and governance signals." action={<Badge variant="green" label="Live" />} />
    <VStack gap={4}>
      <Grid columns={{ minWidth: 150, max: 4 }} gap={3}>
        {metrics.map((metric) => (
          <Card key={metric.label} padding={5}>
            <VStack gap={2}>
              <Text type="supporting">{metric.label}</Text>
              <Text type="display-2" hasTabularNumbers>{metric.value}</Text>
              <Text type="supporting">{metric.detail}</Text>
            </VStack>
          </Card>
        ))}
      </Grid>
      <Grid columns={{ minWidth: 380, max: 2 }} gap={3}>
        <Card padding={5}>
          <VStack gap={4}>
            <HStack gap={4} hAlign="between" vAlign="start">
              <VStack gap={1}>
                <Heading level={2}>Processing flow</Heading>
                <Text type="supporting">Current durable job states</Text>
              </VStack>
              <Link href="/processing">View jobs →</Link>
            </HStack>
            {statuses.length
              ? <ReactECharts option={chartOption} style={{ height: 260 }} />
              : <Empty>No processing jobs in this scope yet.</Empty>}
          </VStack>
        </Card>
        <Card padding={5}>
          <VStack gap={4}>
            <HStack gap={4} hAlign="between" vAlign="start">
              <VStack gap={1}>
                <Heading level={2}>Recent activity</Heading>
                <Text type="supporting">Tenant-scoped audit trail</Text>
              </VStack>
              <Link href="/audit">Open audit →</Link>
            </HStack>
            <AuditList events={overview.recent_audit_events} />
          </VStack>
        </Card>
      </Grid>
      <Card padding={5}>
        <VStack gap={4}>
          <HStack gap={4} hAlign="between" vAlign="start">
            <VStack gap={1}>
              <Heading level={2}>Recently moving</Heading>
              <Text type="supporting">Jobs that changed state most recently</Text>
            </VStack>
            <Link href="/processing">All processing →</Link>
          </HStack>
          <JobsTable jobs={overview.recent_jobs} />
        </VStack>
      </Card>
    </VStack>
  </>
}
