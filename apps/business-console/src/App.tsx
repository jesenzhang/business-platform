import { Navigate, Route, Routes } from 'react-router-dom'
import { AppShell } from './components'
import { AuditPage, CandidatePage, DashboardPage, DocumentDetailPage, DocumentsPage, FindingsPage, ProcessingPage, RepairsPage } from './pages'

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
      <Route path="*" element={<Navigate to="/" replace />} />
    </Route>
  </Routes>
}
