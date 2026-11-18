// --- PLAN-0013 Identity & Authorization management pages (Stage 9) ---

import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  addUnitMember, createOrganizationUnit, createRole, createRoleBinding, createTenantMembership,
  explainDecision, listAdminUsers, listOrganizationUnits, listPermissions, listRoleBindings,
  listRoles, listTenantMemberships, listUnitMembers, removeUnitMember, revokeRoleBinding,
  setRolePermissions, suspendMembership, reactivateMembership, updateOrganizationUnit, updateRole,
} from './api'
import type {
  AdminUser, ExplainEvaluation, MembershipView, OrganizationMemberView, OrganizationUnitView,
  RoleBindingView, RoleView,
} from './contracts'
import { Badge } from '@astryxdesign/core/Badge'
import { Banner } from '@astryxdesign/core/Banner'
import { Button } from '@astryxdesign/core/Button'
import { Card } from '@astryxdesign/core/Card'
import { CheckboxInput } from '@astryxdesign/core/CheckboxInput'
import { EmptyState } from '@astryxdesign/core/EmptyState'
import { Grid } from '@astryxdesign/core/Grid'
import { HStack } from '@astryxdesign/core/HStack'
import { Heading } from '@astryxdesign/core/Heading'
import { MetadataList, MetadataListItem } from '@astryxdesign/core/MetadataList'
import { Selector } from '@astryxdesign/core/Selector'
import { Table, proportional } from '@astryxdesign/core/Table'
import { Text } from '@astryxdesign/core/Text'
import { TextInput } from '@astryxdesign/core/TextInput'
import { VStack } from '@astryxdesign/core/VStack'
import { ErrorState, Loading, PageHeader, StatusPill } from './components'

const date = (value: string) => new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value))

function MutationError({ isError, error }: { isError: boolean; error: Error | null }) {
  return isError ? <Banner status="error" title="Request failed" description={error?.message ?? 'Request failed.'} /> : null
}

/** Identity codes are opaque; show a bounded prefix, never the whole value. */
const shortId = (value: string) => `${value.slice(0, 12)}…`

type MembershipRow = MembershipView & Record<string, unknown>
type UserRow = AdminUser & Record<string, unknown>
type RoleRow = RoleView & Record<string, unknown>
type BindingRow = RoleBindingView & Record<string, unknown>
type UnitRow = OrganizationUnitView & { depth: number } & Record<string, unknown>
type UnitMemberRow = OrganizationMemberView & Record<string, unknown>
type EvaluationRow = ExplainEvaluation & Record<string, unknown>

export function IdentitiesPage() {
  const queryClient = useQueryClient()
  const usersQuery = useQuery({ queryKey: ['iam-users'], queryFn: () => listAdminUsers(50) })
  const membershipsQuery = useQuery({ queryKey: ['iam-memberships'], queryFn: () => listTenantMemberships(50) })
  const [attachUserId, setAttachUserId] = useState('')
  const create = useMutation({
    mutationFn: () => createTenantMembership({ user_id: attachUserId }),
    onSuccess: () => { setAttachUserId(''); void queryClient.invalidateQueries({ queryKey: ['iam-memberships'] }) },
  })
  const status = useMutation({
    mutationFn: ({ userId, version, suspended }: { userId: string; version: number; suspended: boolean }) => suspended ? suspendMembership(userId, version) : reactivateMembership(userId, version),
    onSuccess: () => { void queryClient.invalidateQueries({ queryKey: ['iam-memberships'] }); void queryClient.invalidateQueries({ queryKey: ['iam-users'] }) },
  })
  if (usersQuery.isPending || membershipsQuery.isPending) return <Loading />
  if (usersQuery.isError) return <ErrorState error={usersQuery.error} />
  if (membershipsQuery.isError) return <ErrorState error={membershipsQuery.error} />
  return <>
    <PageHeader eyebrow="IDENTITY MANAGEMENT" title="Identities" description="Platform users and tenant memberships. Authentication proves who you are; membership decides whether this tenant can see you at all." />
    <VStack gap={4}>
      <Card padding={5}>
        <VStack gap={4}>
          <VStack gap={1}>
            <Heading level={2}>Attach a user to this tenant</Heading>
            <Text type="supporting">Memberships are permissioned, audited, and idempotent server-side.</Text>
          </VStack>
          <HStack gap={3} vAlign="end" wrap="wrap">
            <TextInput label="Existing user id" value={attachUserId} onChange={setAttachUserId} placeholder="Existing user id" />
            <Button
              label={create.isPending ? 'Attaching…' : 'Attach membership'}
              variant="primary"
              isDisabled={!attachUserId || create.isPending}
              isLoading={create.isPending}
              onClick={() => create.mutate()}
            />
          </HStack>
          <MutationError {...create} />
        </VStack>
      </Card>
      <Grid columns={{ minWidth: 340, max: 2 }} gap={4}>
        <Card padding={5}>
          <VStack gap={4}>
            <VStack gap={1}>
              <Heading level={2}>Tenant memberships</Heading>
              <Text type="supporting">{membershipsQuery.data.items.length} members in this page</Text>
            </VStack>
            {membershipsQuery.data.items.length ? (
              <Table<MembershipRow>
                data={membershipsQuery.data.items as MembershipRow[]}
                idKey="membership_id"
                density="compact"
                columns={[
                  { key: 'user_id', header: 'User', renderCell: (row) => <Text type="code" as="span">{shortId(row.user_id)}</Text> },
                  { key: 'status', header: 'Status', renderCell: (row) => <StatusPill value={row.status} /> },
                  { key: 'source', header: 'Source' },
                  { key: 'joined_at', header: 'Joined', renderCell: (row) => <Text as="span" type="supporting">{date(row.joined_at)}</Text> },
                  {
                    key: 'membership_id',
                    header: 'Action',
                    renderCell: (row) => (
                      <Button
                        label={row.status === 'active' ? 'Suspend' : 'Reactivate'}
                        variant="secondary"
                        size="sm"
                        isDisabled={status.isPending}
                        onClick={() => status.mutate({ userId: row.user_id, version: row.version, suspended: row.status === 'active' })}
                      />
                    ),
                  },
                ]}
              />
            ) : <EmptyState isCompact title="No memberships yet." />}
          </VStack>
        </Card>
        <Card padding={5}>
          <VStack gap={4}>
            <VStack gap={1}>
              <Heading level={2}>Platform users</Heading>
              <Text type="supporting">Provisioned deterministically from external identities.</Text>
            </VStack>
            {usersQuery.data.items.length ? (
              <Table<UserRow>
                data={usersQuery.data.items as UserRow[]}
                idKey="user_id"
                density="compact"
                columns={[
                  { key: 'user_id', header: 'User', renderCell: (row) => <Text type="code" as="span">{shortId(row.user_id)}</Text> },
                  { key: 'status', header: 'Status', renderCell: (row) => <StatusPill value={row.status} /> },
                  { key: 'created_at', header: 'Created', renderCell: (row) => <Text as="span" type="supporting">{date(row.created_at)}</Text> },
                ]}
              />
            ) : <EmptyState isCompact title="No platform users yet." />}
          </VStack>
        </Card>
      </Grid>
      <MutationError {...status} />
    </VStack>
  </>
}

export function RolesPage() {
  const queryClient = useQueryClient()
  const rolesQuery = useQuery({ queryKey: ['iam-roles'], queryFn: listRoles })
  const permissionsQuery = useQuery({ queryKey: ['iam-permissions'], queryFn: listPermissions })
  const [form, setForm] = useState({ stable_key: '', display_name: '' })
  const [selected, setSelected] = useState<string[]>([])
  const [editRoleId, setEditRoleId] = useState<string | null>(null)
  const [editSelected, setEditSelected] = useState<string[]>([])
  const create = useMutation({
    mutationFn: () => createRole({ stable_key: form.stable_key, display_name: form.display_name, permission_keys: selected }),
    onSuccess: () => { setForm({ stable_key: '', display_name: '' }); setSelected([]); void queryClient.invalidateQueries({ queryKey: ['iam-roles'] }) },
  })
  const setPermissions = useMutation({
    mutationFn: ({ roleId, keys, version }: { roleId: string; keys: string[]; version: number }) => setRolePermissions(roleId, keys, version),
    onSuccess: () => { setEditRoleId(null); setEditSelected([]); void queryClient.invalidateQueries({ queryKey: ['iam-roles'] }) },
  })
  const disable = useMutation({
    mutationFn: ({ roleId, version }: { roleId: string; version: number }) => updateRole(roleId, { status: 'disabled', expected_version: version }),
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ['iam-roles'] }),
  })
  if (rolesQuery.isPending || permissionsQuery.isPending) return <Loading />
  if (rolesQuery.isError) return <ErrorState error={rolesQuery.error} />
  if (permissionsQuery.isError) return <ErrorState error={permissionsQuery.error} />
  const catalog = permissionsQuery.data.filter((permission) => permission.active)
  const editing = editRoleId ? rolesQuery.data.find((role) => role.role_id === editRoleId) : null
  const toggleSelected = (permissionKey: string, checked: boolean) => setSelected((previous) => checked ? Array.from(new Set([...previous, permissionKey])) : previous.filter((key) => key !== permissionKey))
  const toggleEditSelected = (permissionKey: string, checked: boolean) => setEditSelected((previous) => checked ? Array.from(new Set([...previous, permissionKey])) : previous.filter((key) => key !== permissionKey))
  const closeEdit = () => { setEditRoleId(null); setEditSelected([]) }
  const openEdit = (roleId: string, permissionKeys: string[]) => { setEditRoleId(roleId); setEditSelected(permissionKeys) }
  return <>
    <PageHeader eyebrow="POLICY MANAGEMENT" title="Roles & permissions" description="Roles grant permission keys inside the policy context. System roles are immutable by contract; every mutation is permissioned, versioned, and audited." />
    <VStack gap={4}>
      <Card padding={5}>
        <VStack gap={4}>
          <VStack gap={1}>
            <Heading level={2}>Create a tenant role</Heading>
            <Text type="supporting">Tick the permission keys this role should carry, then create. Permission sets are replaced transactionally, never appended blindly.</Text>
          </VStack>
          <HStack gap={3} vAlign="end" wrap="wrap">
            <TextInput label="Stable key" value={form.stable_key} onChange={(value) => setForm({ ...form, stable_key: value })} placeholder="stable-key" />
            <TextInput label="Display name" value={form.display_name} onChange={(value) => setForm({ ...form, display_name: value })} placeholder="Display name" />
            <Button
              label={create.isPending ? 'Creating…' : 'Create role'}
              variant="primary"
              isDisabled={!form.stable_key || !form.display_name || create.isPending}
              isLoading={create.isPending}
              onClick={() => create.mutate()}
            />
          </HStack>
          <Grid columns={{ minWidth: 180, max: 4 }} gap={2}>
            {catalog.map((permission) => (
              <CheckboxInput
                key={permission.key}
                label={permission.reserved ? `${permission.key} (reserved)` : permission.key}
                value={selected.includes(permission.key)}
                onChange={(checked) => toggleSelected(permission.key, checked)}
              />
            ))}
          </Grid>
          <MutationError {...create} />
        </VStack>
      </Card>
      <Card padding={5}>
        <VStack gap={4}>
          <VStack gap={1}>
            <Heading level={2}>Registered roles</Heading>
            <Text type="supporting">{rolesQuery.data.length} roles visible in this tenant</Text>
          </VStack>
          {rolesQuery.data.length ? (
            <Table<RoleRow>
              data={rolesQuery.data as RoleRow[]}
              idKey="role_id"
              density="compact"
              columns={[
                {
                  key: 'display_name',
                  header: 'Role',
                  width: proportional(2),
                  renderCell: (row) => (
                    <VStack gap={0.5}>
                      <Text weight="semibold">{row.display_name}</Text>
                      <Text type="code" as="span">{`${row.stable_key}${row.system ? ' · system' : ''}`}</Text>
                    </VStack>
                  ),
                },
                { key: 'status', header: 'Status', renderCell: (row) => <StatusPill value={row.status} /> },
                {
                  key: 'permission_keys',
                  header: 'Permissions',
                  width: proportional(2),
                  renderCell: (row) => row.permission_keys.length
                    ? <Text type="code" as="span">{row.permission_keys.join(', ')}</Text>
                    : <Text type="supporting">none</Text>,
                },
                {
                  key: 'role_id',
                  header: 'Actions',
                  renderCell: (row) => (
                    <HStack gap={2} wrap="wrap">
                      {!row.system && (
                        <Button
                          label={editRoleId === row.role_id ? 'Cancel' : 'Edit permissions'}
                          variant="secondary"
                          size="sm"
                          onClick={() => editRoleId === row.role_id ? closeEdit() : openEdit(row.role_id, row.permission_keys)}
                        />
                      )}
                      {!row.system && row.status === 'active' && (
                        <Button
                          label="Disable"
                          variant="secondary"
                          size="sm"
                          isDisabled={disable.isPending}
                          onClick={() => disable.mutate({ roleId: row.role_id, version: row.version })}
                        />
                      )}
                      {row.system && <Text type="supporting">immutable</Text>}
                    </HStack>
                  ),
                },
              ]}
            />
          ) : <EmptyState isCompact title="No roles are registered in this tenant." />}

          <MutationError {...disable} />
        </VStack>
      </Card>
      {editing && (
        <Card padding={5}>
          <VStack gap={4}>
            <VStack gap={1}>
              <Heading level={2}>Permissions for {editing.display_name}</Heading>
              <Text type="supporting">Set replace at optimistic version v{editing.version}. Unchecking a key you currently hold can lock you out — the self-escalation guard prevents the reverse.</Text>
            </VStack>
            <Grid columns={{ minWidth: 180, max: 4 }} gap={2}>
              {catalog.map((permission) => (
                <CheckboxInput
                  key={permission.key}
                  label={permission.key}
                  value={editSelected.includes(permission.key)}
                  onChange={(checked) => toggleEditSelected(permission.key, checked)}
                />
              ))}
            </Grid>
            <HStack gap={2} hAlign="end">
              <Button
                label="Save permissions"
                variant="primary"
                isDisabled={setPermissions.isPending}
                isLoading={setPermissions.isPending}
                onClick={() => setPermissions.mutate({ roleId: editing.role_id, keys: editSelected, version: editing.version })}
              />
            </HStack>
            <MutationError {...setPermissions} />
          </VStack>
        </Card>
      )}
    </VStack>
  </>
}

export function BindingsPage() {
  const queryClient = useQueryClient()
  const bindingsQuery = useQuery({ queryKey: ['iam-bindings'], queryFn: () => listRoleBindings() })
  const rolesQuery = useQuery({ queryKey: ['iam-roles'], queryFn: listRoles })
  const [form, setForm] = useState({ user_id: '', role_id: '', scope: 'tenant', org_unit_id: '', resource_kind: '' })
  const create = useMutation({
    mutationFn: () => createRoleBinding({ user_id: form.user_id, role_id: form.role_id, scope: form.scope === 'org_unit' ? { scope: 'org_unit', org_unit_id: form.org_unit_id, include_subtree: false } : form.scope === 'resource_type' ? { scope: 'resource_type', resource_kind: form.resource_kind } : { scope: 'tenant' } }),
    onSuccess: () => { setForm({ user_id: '', role_id: '', scope: 'tenant', org_unit_id: '', resource_kind: '' }); void queryClient.invalidateQueries({ queryKey: ['iam-bindings'] }) },
  })
  const revoke = useMutation({
    mutationFn: ({ bindingId, version }: { bindingId: string; version: number }) => revokeRoleBinding(bindingId, version),
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ['iam-bindings'] }),
  })
  if (bindingsQuery.isPending || rolesQuery.isPending) return <Loading />
  if (bindingsQuery.isError) return <ErrorState error={bindingsQuery.error} />
  if (rolesQuery.isError) return <ErrorState error={rolesQuery.error} />
  const scopeOk = form.scope === 'tenant' || (form.scope === 'org_unit' && Boolean(form.org_unit_id)) || (form.scope === 'resource_type' && Boolean(form.resource_kind))
  const activeRoles = rolesQuery.data.filter((role) => role.status === 'active').map((role) => ({ value: role.role_id, label: role.display_name }))
  return <>
    <PageHeader eyebrow="POLICY MANAGEMENT" title="Role bindings" description="A binding grants a role over a bounded scope. Suspension, expiry, and revocation take effect on the next request — an unexpired token never wins." />
    <VStack gap={4}>
      <Card padding={5}>
        <VStack gap={4}>
          <VStack gap={1}>
            <Heading level={2}>Create a binding</Heading>
            <Text type="supporting">The domain rejects self-escalation: you cannot grant yourself IAM-management authority unless you already hold it via a system bootstrap role.</Text>
          </VStack>
          <HStack gap={3} vAlign="end" wrap="wrap">
            <TextInput label="User id" value={form.user_id} onChange={(value) => setForm({ ...form, user_id: value })} placeholder="User id" />
            <Selector
              label="Role"
              aria-label="Role"
              options={activeRoles}
              value={form.role_id}
              onChange={(value) => setForm({ ...form, role_id: value })}
              placeholder="Choose a role…"
            />
            <Selector
              label="Scope"
              aria-label="Scope"
              options={[
                { value: 'tenant', label: 'Whole tenant' },
                { value: 'org_unit', label: 'Organization unit' },
                { value: 'resource_type', label: 'Resource kind' },
              ]}
              value={form.scope}
              onChange={(value) => setForm({ ...form, scope: value })}
            />
            {form.scope === 'org_unit' && <TextInput label="Org unit id" value={form.org_unit_id} onChange={(value) => setForm({ ...form, org_unit_id: value })} placeholder="Org unit id" />}
            {form.scope === 'resource_type' && <TextInput label="Resource kind" value={form.resource_kind} onChange={(value) => setForm({ ...form, resource_kind: value })} placeholder="Resource kind" />}
            <Button
              label={create.isPending ? 'Binding…' : 'Bind role'}
              variant="primary"
              isDisabled={!form.user_id || !form.role_id || !scopeOk || create.isPending}
              isLoading={create.isPending}
              onClick={() => create.mutate()}
            />
          </HStack>
          <MutationError {...create} />
        </VStack>
      </Card>
      <Card padding={5}>
        <VStack gap={4}>
          <VStack gap={1}>
            <Heading level={2}>Bindings</Heading>
            <Text type="supporting">{bindingsQuery.data.length} bindings in this tenant</Text>
          </VStack>
          {bindingsQuery.data.length ? (
            <Table<BindingRow>
              data={bindingsQuery.data as BindingRow[]}
              idKey="binding_id"
              density="compact"
              columns={[
                { key: 'user_id', header: 'User', renderCell: (row) => <Text type="code" as="span">{shortId(row.user_id)}</Text> },
                { key: 'role_id', header: 'Role', renderCell: (row) => <Text type="code" as="span">{shortId(row.role_id)}</Text> },
                {
                  key: 'scope',
                  header: 'Scope',
                  renderCell: (row) => (
                    <Text type="code" as="span">
                      {row.scope.scope === 'tenant' ? 'tenant' : row.scope.scope === 'org_unit' ? `org_unit ${shortId(row.scope.org_unit_id)}` : row.scope.resource_kind}
                    </Text>
                  ),
                },
                { key: 'status', header: 'Status', renderCell: (row) => <StatusPill value={row.status} /> },
                {
                  key: 'effective_at',
                  header: 'Window',
                  renderCell: (row) => <Text as="span" type="supporting">{`${date(row.effective_at)}${row.expires_at ? ` → ${date(row.expires_at)}` : ''}`}</Text>,
                },
                {
                  key: 'binding_id',
                  header: 'Action',
                  renderCell: (row) => row.status === 'active'
                    ? <Button label="Revoke" variant="secondary" size="sm" isDisabled={revoke.isPending} onClick={() => revoke.mutate({ bindingId: row.binding_id, version: row.version })} />
                    : null,
                },
              ]}
            />
          ) : <EmptyState isCompact title="No bindings exist in this tenant." />}
          <MutationError {...revoke} />
        </VStack>
      </Card>
    </VStack>
  </>
}

function flattenUnits(units: OrganizationUnitView[]) {
  const byParent = new Map<string | null, OrganizationUnitView[]>()
  for (const unit of units) {
    const key = unit.parent_id ?? null
    byParent.set(key, [...(byParent.get(key) ?? []), unit])
  }
  const rows: Array<{ unit: OrganizationUnitView; depth: number }> = []
  const walk = (parent: string | null, depth: number) => {
    for (const unit of byParent.get(parent) ?? []) {
      rows.push({ unit, depth })
      walk(unit.unit_id, depth + 1)
    }
  }
  walk(null, 0)
  return rows
}

export function OrganizationPage() {
  const queryClient = useQueryClient()
  const unitsQuery = useQuery({ queryKey: ['iam-units'], queryFn: listOrganizationUnits })
  const [selectedUnit, setSelectedUnit] = useState<string | null>(null)
  const [unitForm, setUnitForm] = useState({ name: '', unit_type: 'department', parent_id: '' })
  const [memberForm, setMemberForm] = useState({ user_id: '', membership_type: 'member' })
  const membersQuery = useQuery({ queryKey: ['iam-unit-members', selectedUnit], queryFn: () => listUnitMembers(selectedUnit ?? ''), enabled: Boolean(selectedUnit) })
  const create = useMutation({
    mutationFn: () => createOrganizationUnit({ name: unitForm.name, unit_type: unitForm.unit_type, parent_id: unitForm.parent_id || null }),
    onSuccess: () => { setUnitForm({ name: '', unit_type: 'department', parent_id: '' }); void queryClient.invalidateQueries({ queryKey: ['iam-units'] }) },
  })
  const disable = useMutation({
    mutationFn: ({ unitId, version }: { unitId: string; version: number }) => updateOrganizationUnit(unitId, { status: 'disabled', expected_version: version }),
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ['iam-units'] }),
  })
  const addMember = useMutation({
    mutationFn: ({ unitId, userId, membershipType }: { unitId: string; userId: string; membershipType: string }) => addUnitMember(unitId, userId, membershipType),
    onSuccess: () => { setMemberForm({ user_id: '', membership_type: 'member' }); void queryClient.invalidateQueries({ queryKey: ['iam-unit-members', selectedUnit] }) },
  })
  const removeMember = useMutation({
    mutationFn: ({ unitId, userId, membershipType, version }: { unitId: string; userId: string; membershipType: string; version: number }) => removeUnitMember(unitId, userId, membershipType, version),
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ['iam-unit-members', selectedUnit] }),
  })
  if (unitsQuery.isPending) return <Loading />
  if (unitsQuery.isError) return <ErrorState error={unitsQuery.error} />
  const roots = unitsQuery.data.filter((unit) => !unit.parent_id)
  const unitRows: UnitRow[] = flattenUnits(unitsQuery.data).map(({ unit, depth }) => ({ ...unit, depth }))
  const selectedUnitName = unitsQuery.data.find((unit) => unit.unit_id === selectedUnit)?.name
  return <>
    <PageHeader eyebrow="ORGANIZATION" title="Organization units" description="Authorization-only organization: units scope resource grants. Memberships here never become business facts." />
    <VStack gap={4}>
      <Card padding={5}>
        <VStack gap={4}>
          <VStack gap={1}>
            <Heading level={2}>Unit tree</Heading>
            <Text type="supporting">{unitsQuery.data.length} units in this tenant</Text>
          </VStack>
          <HStack gap={3} vAlign="end" wrap="wrap">
            <TextInput label="Unit name" value={unitForm.name} onChange={(value) => setUnitForm({ ...unitForm, name: value })} placeholder="Unit name" />
            <Selector
              label="Unit type"
              aria-label="Unit type"
              options={['company', 'department', 'team']}
              value={unitForm.unit_type}
              onChange={(value) => setUnitForm({ ...unitForm, unit_type: value })}
            />
            <Selector
              label="Parent unit"
              aria-label="Parent unit"
              options={[{ value: 'none', label: 'No parent (root)' }, ...roots.map((unit) => ({ value: unit.unit_id, label: unit.name }))]}
              value={unitForm.parent_id || 'none'}
              onChange={(value) => setUnitForm({ ...unitForm, parent_id: value === 'none' ? '' : value })}
            />
            <Button
              label={create.isPending ? 'Creating…' : 'Create unit'}
              variant="primary"
              isDisabled={!unitForm.name || create.isPending}
              isLoading={create.isPending}
              onClick={() => create.mutate()}
            />
          </HStack>
          {unitRows.length ? (
            <Table<UnitRow>
              data={unitRows}
              idKey="unit_id"
              density="compact"
              columns={[
                {
                  key: 'name',
                  header: 'Unit',
                  width: proportional(2),
                  renderCell: (row) => (
                    <VStack gap={0.5}>
                      <Text weight="semibold">{`${'— '.repeat(row.depth)}${row.name}`}</Text>
                      <Text type="supporting">{row.unit_type}</Text>
                    </VStack>
                  ),
                },
                { key: 'status', header: 'Status', renderCell: (row) => <StatusPill value={row.status} /> },
                {
                  key: 'unit_id',
                  header: 'Actions',
                  renderCell: (row) => (
                    <HStack gap={2} wrap="wrap">
                      <Button label="Members" variant="secondary" size="sm" onClick={() => setSelectedUnit(row.unit_id)} />
                      {row.status === 'active' && (
                        <Button label="Disable" variant="secondary" size="sm" isDisabled={disable.isPending} onClick={() => disable.mutate({ unitId: row.unit_id, version: row.version })} />
                      )}
                    </HStack>
                  ),
                },
              ]}
            />
          ) : <EmptyState isCompact title="No organization units yet." />}
          <MutationError {...create} />
          <MutationError {...disable} />
        </VStack>
      </Card>
      {selectedUnit && (
        <Card padding={5}>
          <VStack gap={4}>
            <HStack gap={3} hAlign="between" vAlign="start" wrap="wrap">
              <VStack gap={1}>
                <Heading level={2}>Members of {selectedUnitName ?? shortId(selectedUnit)}</Heading>
                <Text type="supporting">Add or remove tenant members with idempotent, audited operations.</Text>
              </VStack>
              <Button label="Close" variant="secondary" size="sm" onClick={() => setSelectedUnit(null)} />
            </HStack>
            {membersQuery.isPending ? <Loading /> : membersQuery.isError ? <ErrorState error={membersQuery.error} /> : (
              <>
                <HStack gap={3} vAlign="end" wrap="wrap">
                  <TextInput label="Tenant user id" value={memberForm.user_id} onChange={(value) => setMemberForm({ ...memberForm, user_id: value })} placeholder="Tenant user id" />
                  <Selector
                    label="Membership type"
                    aria-label="Membership type"
                    options={['member', 'leader']}
                    value={memberForm.membership_type}
                    onChange={(value) => setMemberForm({ ...memberForm, membership_type: value })}
                  />
                  <Button
                    label="Add member"
                    variant="primary"
                    isDisabled={!memberForm.user_id || addMember.isPending}
                    isLoading={addMember.isPending}
                    onClick={() => addMember.mutate({ unitId: selectedUnit, userId: memberForm.user_id, membershipType: memberForm.membership_type })}
                  />
                </HStack>
                {membersQuery.data.length ? (
                  <Table<UnitMemberRow>
                    data={membersQuery.data as UnitMemberRow[]}
                    idKey="membership_id"
                    density="compact"
                    columns={[
                      { key: 'user_id', header: 'User', renderCell: (row) => <Text type="code" as="span">{shortId(row.user_id)}</Text> },
                      { key: 'membership_type', header: 'Type' },
                      { key: 'status', header: 'Status', renderCell: (row) => <StatusPill value={row.status} /> },
                      { key: 'joined_at', header: 'Joined', renderCell: (row) => <Text as="span" type="supporting">{date(row.joined_at)}</Text> },
                      {
                        key: 'membership_id',
                        header: 'Action',
                        renderCell: (row) => row.status === 'active'
                          ? <Button label="Remove" variant="secondary" size="sm" isDisabled={removeMember.isPending} onClick={() => removeMember.mutate({ unitId: selectedUnit, userId: row.user_id, membershipType: row.membership_type, version: row.version })} />
                          : null,
                      },
                    ]}
                  />
                ) : <EmptyState isCompact title="This unit has no members." />}
                <MutationError {...addMember} />
                <MutationError {...removeMember} />
              </>
            )}
          </VStack>
        </Card>
      )}
    </VStack>
  </>
}

export function ExplainPage() {
  const [form, setForm] = useState({ user_id: '', permission: 'audit.read' })
  const explain = useMutation({ mutationFn: () => explainDecision({ user_id: form.user_id, permission: form.permission }) })
  return <>
    <PageHeader eyebrow="AUTHORIZATION" title="Explain a decision" description="Ask the evaluator, not the database: the same default-DENY engine answers with bounded, redacted reasons for every binding it considered." />
    <VStack gap={4}>
      <Card padding={5}>
        <VStack gap={4}>
          <VStack gap={1}>
            <Heading level={2}>Evaluate one permission</Heading>
            <Text type="supporting">Requires the policy.explain permission. The response never contains raw claims, tokens, or store internals.</Text>
          </VStack>
          <HStack gap={3} vAlign="end" wrap="wrap">
            <TextInput label="User id" value={form.user_id} onChange={(value) => setForm({ ...form, user_id: value })} placeholder="User id" />
            <TextInput label="Permission key" value={form.permission} onChange={(value) => setForm({ ...form, permission: value })} placeholder="permission.key" />
            <Button
              label={explain.isPending ? 'Evaluating…' : 'Explain'}
              variant="primary"
              isDisabled={!form.user_id || !form.permission || explain.isPending}
              isLoading={explain.isPending}
              onClick={() => explain.mutate()}
            />
          </HStack>
          <MutationError {...explain} />
        </VStack>
      </Card>
      {explain.data && (
        <Grid columns={{ minWidth: 340, max: 2 }} gap={4}>
          <Card padding={5}>
            <VStack gap={4}>
              <Text type="label" color="secondary">DECISION</Text>
              <HStack gap={2}>
                <StatusPill value={explain.data.allowed ? 'allow' : 'deny'} />
                <Badge variant={explain.data.allowed ? 'success' : 'error'} label={explain.data.allowed ? 'allowed' : 'denied'} />
              </HStack>
              <MetadataList>
                <MetadataListItem label="Reason"><Text type="code" as="span">{explain.data.reason}</Text></MetadataListItem>
                {explain.data.matched_binding && <MetadataListItem label="Matched binding"><Text type="code" as="span">{explain.data.matched_binding}</Text></MetadataListItem>}
                {explain.data.matched_scope && <MetadataListItem label="Scope"><Text type="code" as="span">{JSON.stringify(explain.data.matched_scope)}</Text></MetadataListItem>}
                <MetadataListItem label="Policy reference"><Text type="code" as="span">{explain.data.policy_reference}</Text></MetadataListItem>
              </MetadataList>
            </VStack>
          </Card>
          <Card padding={5}>
            <VStack gap={4}>
              <VStack gap={1}>
                <Heading level={2}>Bindings considered</Heading>
                <Text type="supporting">{explain.data.evaluations.length} candidate evaluations</Text>
              </VStack>
              {explain.data.evaluations.length ? (
                <Table<EvaluationRow>
                  data={explain.data.evaluations as EvaluationRow[]}
                  idKey="binding_id"
                  density="compact"
                  columns={[
                    { key: 'binding_id', header: 'Binding', renderCell: (row) => <Text type="code" as="span">{shortId(row.binding_id)}</Text> },
                    { key: 'binding_active', header: 'Active', renderCell: (row) => (row.binding_active ? 'yes' : 'no') },
                    { key: 'within_validity', header: 'In validity', renderCell: (row) => (row.within_validity ? 'yes' : 'no') },
                    { key: 'role_grants_permission', header: 'Grants key', renderCell: (row) => (row.role_grants_permission ? 'yes' : 'no') },
                    { key: 'outcome', header: 'Outcome', renderCell: (row) => <StatusPill value={row.outcome} /> },
                  ]}
                />
              ) : <EmptyState isCompact title="No candidate binding existed for this subject and tenant." />}
            </VStack>
          </Card>
        </Grid>
      )}
    </VStack>
  </>
}
