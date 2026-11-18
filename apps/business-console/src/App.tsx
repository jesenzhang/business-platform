import { lazy, Suspense } from 'react'
import { Navigate, Route, Routes } from 'react-router-dom'
import { AppShell } from './components'
import { BindingsPage, ExplainPage, IdentitiesPage, OrganizationPage, RolesPage } from './iam-pages'
import { AuditPage, CandidatePage, DashboardPage, DocumentDetailPage, DocumentsPage, FindingsPage, ProcessingPage, RepairsPage } from './pages'

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
      {FoundationCheck && (
        <Route
          path="__foundation-check"
          element={<Suspense fallback={null}><FoundationCheck /></Suspense>}
        />
      )}
      <Route path="*" element={<Navigate to="/" replace />} />
    </Route>
  </Routes>
}
