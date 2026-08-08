import * as Dialog from '@radix-ui/react-dialog'
import { Link, NavLink, Outlet } from 'react-router-dom'
import { useConsoleStore } from './store'

const navItems = [
  ['/', 'Overview', '⌂'],
  ['/documents', 'Documents', '▤'],
  ['/processing', 'Processing', '↻'],
  ['/findings', 'Integrity', '◈'],
  ['/repairs', 'Repairs', '✓'],
  ['/audit', 'Audit', '≡'],
] as const

export function AppShell() {
  const theme = useConsoleStore((state) => state.theme)
  const setTheme = useConsoleStore((state) => state.setTheme)
  return (
    <div className={theme === 'dark' ? 'app dark' : 'app'}>
      <aside className="sidebar desktop-sidebar" aria-label="Primary navigation">
        <Brand />
        <Navigation />
        <div className="sidebar-footer">
          <span className="status-dot" /> API connected through Business API
        </div>
      </aside>
      <div className="mobile-bar">
        <Dialog.Root>
          <Dialog.Trigger asChild><button className="icon-button" aria-label="Open navigation">☰</button></Dialog.Trigger>
          <Dialog.Portal>
            <Dialog.Overlay className="dialog-overlay" />
            <Dialog.Content className="mobile-drawer">
              <Dialog.Title className="sr-only">Navigation</Dialog.Title>
              <Brand /><Navigation closeOnClick />
            </Dialog.Content>
          </Dialog.Portal>
        </Dialog.Root>
        <Brand compact />
      </div>
      <main className="main-content">
        <header className="toolbar">
          <div><span className="eyebrow">BUSINESS PLATFORM</span><span className="toolbar-title">Operations Console</span></div>
          <div className="toolbar-actions">
            <span className="secure-label"><span className="status-dot" /> Tenant-scoped session</span>
            <button className="icon-button" onClick={() => setTheme(theme === 'light' ? 'dark' : 'light')} aria-label="Toggle theme">{theme === 'light' ? '☾' : '☀'}</button>
          </div>
        </header>
        <section className="page-content"><Outlet /></section>
      </main>
    </div>
  )
}

function Brand({ compact = false }: { compact?: boolean }) {
  return <Link to="/" className={compact ? 'brand compact' : 'brand'}><span className="brand-mark">B</span><span><strong>Business</strong><small>{compact ? '' : 'Platform'}</small></span></Link>
}

function Navigation({ closeOnClick = false }: { closeOnClick?: boolean }) {
  return <nav className="nav-list">{navItems.map(([to, label, icon]) => <NavLink key={to} to={to} end={to === '/'} onClick={closeOnClick ? () => document.body.click() : undefined} className={({ isActive }) => isActive ? 'nav-item active' : 'nav-item'}><span className="nav-icon">{icon}</span><span>{label}</span></NavLink>)}</nav>
}

export function PageHeader({ eyebrow, title, description, action }: { eyebrow?: string; title: string; description?: React.ReactNode; action?: React.ReactNode }) {
  return <div className="page-header"><div>{eyebrow && <div className="eyebrow">{eyebrow}</div>}<h1>{title}</h1>{description && <p>{description}</p>}</div>{action && <div>{action}</div>}</div>
}

export function StatCard({ label, value, detail, tone = 'blue' }: { label: string; value: string | number; detail?: string; tone?: 'blue' | 'orange' | 'green' | 'purple' }) {
  return <article className={`stat-card tone-${tone}`}><span className="stat-label">{label}</span><strong>{value}</strong>{detail && <span className="stat-detail">{detail}</span>}</article>
}

export function StatusPill({ value }: { value: string }) {
  return <span className={`status-pill status-${value.replaceAll('_', '-')}`}><span className="status-dot" />{value.replaceAll('_', ' ')}</span>
}

export function Loading() { return <div className="loading"><span className="spinner" /> Loading live data…</div> }
export function Empty({ children = 'No data in this scope yet.' }: { children?: React.ReactNode }) { return <div className="empty">{children}</div> }
export function ErrorState({ error }: { error: Error }) { return <div className="error-state"><strong>Couldn’t load this view.</strong><span>{error.message}</span></div> }
export function IdLink({ id, to }: { id: string; to: string }) { return <Link className="mono link" to={to}>{id.slice(0, 8)}…</Link> }
