import { useEffect, useState } from 'react'
import { Link, Outlet, useLocation } from 'react-router-dom'
import { SideNav, SideNavItem } from '@astryxdesign/core/SideNav'
import { Text } from '@astryxdesign/core/Text'
import {
  Building2, CircleUser, FileText, HelpCircle, KeyRound, LayoutDashboard, Link2,
  Menu, Moon, RefreshCw, ScrollText, ShieldAlert, Sun, Wrench,
} from 'lucide-react'
import { Drawer, DrawerHeader } from '@astryxdesign/core/Drawer'
import { Layout, LayoutContent } from '@astryxdesign/core/Layout'
import { IconButton } from '@astryxdesign/core/IconButton'
import { StatusDot } from '@astryxdesign/core/StatusDot'
import { HStack } from '@astryxdesign/core/HStack'
import { useConsoleStore } from './store'

/**
 * Navigation is defined once. `href` values are consumed by Astryx
 * `SideNavItem`; `LinkProvider` (see `app-root.tsx`) turns them into React
 * Router links, so the router remains the only navigation source of truth and
 * no route adapter lives here.
 */
const navItems = [
  { to: '/', label: 'Overview', icon: LayoutDashboard },
  { to: '/documents', label: 'Documents', icon: FileText },
  { to: '/processing', label: 'Processing', icon: RefreshCw },
  { to: '/findings', label: 'Integrity', icon: ShieldAlert },
  { to: '/repairs', label: 'Repairs', icon: Wrench },
  { to: '/audit', label: 'Audit', icon: ScrollText },
  { to: '/identities', label: 'Identities', icon: CircleUser },
  { to: '/roles', label: 'Roles', icon: KeyRound },
  { to: '/bindings', label: 'Bindings', icon: Link2 },
  { to: '/organization', label: 'Organization', icon: Building2 },
  { to: '/explain', label: 'Explain', icon: HelpCircle },
] as const

/**
 * Same matching rule the previous `NavLink`s used: `end` only for the root,
 * otherwise a path segment prefix (`/documents/42` selects Documents).
 */
function useActiveNav() {
  const { pathname } = useLocation()
  return (to: string) => to === '/' ? pathname === '/' : pathname === to || pathname.startsWith(`${to}/`)
}

export function AppShell() {
  const theme = useConsoleStore((state) => state.theme)
  const setTheme = useConsoleStore((state) => state.setTheme)
  const { pathname } = useLocation()
  const [isNavOpen, setIsNavOpen] = useState(false)

  // A drawer that stays open after a route change hides the page the user just
  // asked for. Closing on navigation keeps the previous behaviour without the
  // old `document.body.click()` trick.
  useEffect(() => { setIsNavOpen(false) }, [pathname])

  const themeButton = (
    <IconButton
      label="Toggle theme"
      tooltip={theme === 'light' ? 'Switch to dark theme' : 'Switch to light theme'}
      icon={theme === 'light' ? <Moon size={16} /> : <Sun size={16} />}
      variant="secondary"
      size="sm"
      onClick={() => setTheme(theme === 'light' ? 'dark' : 'light')}
    />
  )

  return (
    <div className={theme === 'dark' ? 'app dark' : 'app'}>
      <aside className="sidebar desktop-sidebar">
        <Navigation />
      </aside>
      <div className="mobile-bar">
        <IconButton
          label="Open navigation"
          icon={<Menu size={18} />}
          variant="secondary"
          size="sm"
          onClick={() => setIsNavOpen(true)}
        />
        <Brand compact />
        <div className="mobile-bar-actions">{themeButton}</div>
      </div>
      <Drawer
        isOpen={isNavOpen}
        onOpenChange={setIsNavOpen}
        label="Navigation"
        side="start"
        width={300}
      >
        <Layout
          header={<DrawerHeader title="Navigation" onOpenChange={setIsNavOpen} />}
          content={(
            <LayoutContent padding={3}>
              <Navigation showBrand={false} onNavigate={() => setIsNavOpen(false)} />
            </LayoutContent>
          )}
        />
      </Drawer>
      <main className="main-content">
        <header className="toolbar">
          <HStack gap={3} vAlign="center">
            <Text type="label" as="span" color="secondary">BUSINESS PLATFORM</Text>
            <Text type="supporting" as="span">Operations Console</Text>
          </HStack>
          <HStack gap={3} vAlign="center">
            <HStack gap={1} vAlign="center">
              <StatusDot variant="success" label="Tenant-scoped session" />
              <Text type="supporting" as="span">Tenant-scoped session</Text>
            </HStack>
            {themeButton}
          </HStack>
        </header>
        <section className="page-content"><Outlet /></section>
      </main>
    </div>
  )
}

function Brand({ compact = false }: { compact?: boolean }) {
  return <Link to="/" className={compact ? 'brand compact' : 'brand'}><span className="brand-mark">B</span><span><strong>Business</strong><small>{compact ? '' : 'Platform'}</small></span></Link>
}

function Navigation({ showBrand = true, onNavigate }: { showBrand?: boolean; onNavigate?: () => void }) {
  const isActive = useActiveNav()
  return (
    <SideNav
      aria-label="Primary navigation"
      header={showBrand ? <Brand /> : undefined}
      footer={(
        <div className="sidebar-footer">
          <StatusDot variant="success" label="API connected" />
          <Text type="supporting" as="span">API connected through Business API</Text>
        </div>
      )}
    >
      {navItems.map((item) => (
        <SideNavItem
          key={item.to}
          label={item.label}
          icon={item.icon}
          href={item.to}
          isSelected={isActive(item.to)}
          onClick={onNavigate}
        />
      ))}
    </SideNav>
  )
}

export function PageHeader({ eyebrow, title, description, action }: { eyebrow?: string; title: string; description?: React.ReactNode; action?: React.ReactNode }) {
  return <div className="page-header"><div>{eyebrow && <div className="eyebrow">{eyebrow}</div>}<h1>{title}</h1>{description && <p>{description}</p>}</div>{action && <div>{action}</div>}</div>
}

export function StatusPill({ value }: { value: string }) {
  return <span className={`status-pill status-${value.replaceAll('_', '-')}`}><span className="status-dot" />{value.replaceAll('_', ' ')}</span>
}

export function Loading() { return <div className="loading"><span className="spinner" /> Loading live data…</div> }
export function Empty({ children = 'No data in this scope yet.' }: { children?: React.ReactNode }) { return <div className="empty">{children}</div> }
export function ErrorState({ error }: { error: Error }) { return <div className="error-state"><strong>Couldn’t load this view.</strong><span>{error.message}</span></div> }
export function IdLink({ id, to }: { id: string; to: string }) { return <Link className="mono link" to={to}>{id.slice(0, 8)}…</Link> }
