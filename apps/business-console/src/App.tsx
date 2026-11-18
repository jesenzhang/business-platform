import { lazy } from 'react'
import { Navigate, Route, Routes } from 'react-router-dom'
import { AppShell } from './components'

/**
 * Routes are code-split per module:
 *   - the dashboard owns ECharts, so it loads only on `/`;
 *   - the document/processing/governance pages share one chunk;
 *   - the identity & authorization pages share another.
 * `AppShell` keeps its `<Suspense>` around the outlet, so the shell never
 * disappears while a route chunk loads.
 */
const DashboardPage = lazy(() => import('./dashboard-page').then((module) => ({ default: module.DashboardPage })))
const DocumentsPage = lazy(() => import('./pages').then((module) => ({ default: module.DocumentsPage })))
const DocumentDetailPage = lazy(() => import('./pages').then((module) => ({ default: module.DocumentDetailPage })))
const ProcessingPage = lazy(() => import('./pages').then((module) => ({ default: module.ProcessingPage })))
const CandidatePage = lazy(() => import('./pages').then((module) => ({ default: module.CandidatePage })))
const FindingsPage = lazy(() => import('./pages').then((module) => ({ default: module.FindingsPage })))
const RepairsPage = lazy(() => import('./pages').then((module) => ({ default: module.RepairsPage })))
const AuditPage = lazy(() => import('./pages').then((module) => ({ default: module.AuditPage })))
const IdentitiesPage = lazy(() => import('./iam-pages').then((module) => ({ default: module.IdentitiesPage })))
const RolesPage = lazy(() => import('./iam-pages').then((module) => ({ default: module.RolesPage })))
const BindingsPage = lazy(() => import('./iam-pages').then((module) => ({ default: module.BindingsPage })))
const OrganizationPage = lazy(() => import('./iam-pages').then((module) => ({ default: module.OrganizationPage })))
const ExplainPage = lazy(() => import('./iam-pages').then((module) => ({ default: module.ExplainPage })))

// Dev-only foundation fixture (PLAN-0016 S1). `import.meta.env.DEV` is a
// compile-time constant, so a production build resolves this to null and the
// route plus its lazy chunk never enter the bundle.
const FoundationCheck = import.meta.env.DEV ? lazy(() => import('./foundation-check')) : null

export default function App() {
  return <Routes>
    <Route element={<AppShell />}>
      <Route index element={<DashboardPage />} />
      <Route path="documents" element={<DocumentsPage />} />
      <Route path="documents/:documentId" element={<DocumentDetailPage />} />
      <Route path="processing" element={<ProcessingPage />} />
      <Route path="processing/:jobId" element={<ProcessingPage />} />
      <Route path="processing/:jobId/candidate" element={<CandidatePage />} />
      <Route path="findings" element={<FindingsPage />} />
      <Route path="repairs" element={<RepairsPage />} />
      <Route path="audit" element={<AuditPage />} />
      <Route path="identities" element={<IdentitiesPage />} />
      <Route path="roles" element={<RolesPage />} />
      <Route path="bindings" element={<BindingsPage />} />
      <Route path="organization" element={<OrganizationPage />} />
      <Route path="explain" element={<ExplainPage />} />
      {FoundationCheck && <Route path="__foundation-check" element={<FoundationCheck />} />}
      <Route path="*" element={<Navigate to="/" replace />} />
    </Route>
  </Routes>
}
