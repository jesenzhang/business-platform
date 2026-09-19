// --- PLAN-0013 Identity & Authorization management pages (Stage 9) ---

import { useState, type ReactNode } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  addUnitMember, createOrganizationUnit, createRole, createRoleBinding, createTenantMembership,
  explainDecision, listAdminUsers, listOrganizationUnits, listPermissions, listRoleBindings,
  listRoles, listTenantMemberships, listUnitMembers, removeUnitMember, revokeRoleBinding,
  setRolePermissions, suspendMembership, reactivateMembership, updateOrganizationUnit, updateRole,
} from './api'
import type { OrganizationUnitView } from './contracts'
import { Empty, ErrorState, Loading, PageHeader, StatusPill } from './components'

const date = (value: string) => new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value))

function MutationError({ isError, error }: { isError: boolean; error: Error | null }) {
  return isError ? <div className="inline-error">{error?.message ?? 'Request failed.'}</div> : null
}

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
    <div className="upload-panel panel"><div><h2>Attach a user to this tenant</h2><p>Memberships are permissioned, audited, and idempotent server-side.</p></div><div className="upload-control"><input className="text-input" placeholder="Existing user id" value={attachUserId} onChange={(event) => setAttachUserId(event.target.value)} /><button className="primary-button" disabled={!attachUserId || create.isPending} onClick={() => create.mutate()}>{create.isPending ? 'Attaching…' : 'Attach membership'}</button></div><MutationError {...create} /></div>
    <div className="detail-grid">
      <article className="panel"><div className="panel-heading"><div><h2>Tenant memberships</h2><p>{membershipsQuery.data.items.length} members in this page</p></div></div>
        {membershipsQuery.data.items.length ? <div className="table-wrap"><table><thead><tr><th>User</th><th>Status</th><th>Source</th><th>Joined</th><th>Action</th></tr></thead><tbody>{membershipsQuery.data.items.map((row) => <tr key={row.membership_id}><td className="mono">{row.user_id.slice(0, 12)}…</td><td><StatusPill value={row.status} /></td><td>{row.source}</td><td>{date(row.joined_at)}</td><td><button className="secondary-button" disabled={status.isPending} onClick={() => status.mutate({ userId: row.user_id, version: row.version, suspended: row.status === 'active' })}>{row.status === 'active' ? 'Suspend' : 'Reactivate'}</button></td></tr>)}</tbody></table></div> : <Empty>No memberships yet.</Empty>}
      </article>
      <article className="panel"><div className="panel-heading"><div><h2>Platform users</h2><p>Provisioned deterministically from external identities.</p></div></div>
        {usersQuery.data.items.length ? <div className="table-wrap"><table><thead><tr><th>User</th><th>Status</th><th>Created</th></tr></thead><tbody>{usersQuery.data.items.map((user) => <tr key={user.user_id}><td className="mono">{user.user_id.slice(0, 12)}…</td><td><StatusPill value={user.status} /></td><td>{date(user.created_at)}</td></tr>)}</tbody></table></div> : <Empty>No platform users yet.</Empty>}
      </article>
    </div>
    <MutationError {...status} />
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
    <article className="panel"><div className="panel-heading"><div><h2>Create a tenant role</h2><p>Tick the permission keys this role should carry, then create. Permission sets are replaced transactionally, never appended blindly.</p></div></div>
      <div className="upload-control"><input className="text-input" placeholder="stable-key" value={form.stable_key} onChange={(event) => setForm({ ...form, stable_key: event.target.value })} /><input className="text-input" placeholder="Display name" value={form.display_name} onChange={(event) => setForm({ ...form, display_name: event.target.value })} /><button className="primary-button" disabled={!form.stable_key || !form.display_name || create.isPending} onClick={() => create.mutate()}>{create.isPending ? 'Creating…' : 'Create role'}</button></div>
      <div className="chip-grid">{catalog.map((permission) => <label key={permission.key} className="chip"><input type="checkbox" checked={selected.includes(permission.key)} onChange={(event) => toggleSelected(permission.key, event.target.checked)} /><span>{permission.key}</span>{permission.reserved && <small>reserved</small>}</label>)}</div>
      <MutationError {...create} />
    </article>
    <article className="panel"><div className="panel-heading"><div><h2>Registered roles</h2><p>{rolesQuery.data.length} roles visible in this tenant</p></div></div>
      {rolesQuery.data.length ? <div className="table-wrap"><table><thead><tr><th>Role</th><th>Status</th><th>Permissions</th><th>Actions</th></tr></thead><tbody>{rolesQuery.data.map((role) => <tr key={role.role_id}><td><span className="table-primary">{role.display_name}</span><span className="table-secondary mono">{role.stable_key}{role.system ? ' · system' : ''}</span></td><td><StatusPill value={role.status} /></td><td>{role.permission_keys.length ? <span className="mono">{role.permission_keys.join(', ')}</span> : <span className="subtle">none</span>}</td><td><div className="button-row compact">{!role.system && <button className="secondary-button" onClick={() => editRoleId === role.role_id ? closeEdit() : openEdit(role.role_id, role.permission_keys)}>{editRoleId === role.role_id ? 'Cancel' : 'Edit permissions'}</button>}{!role.system && role.status === 'active' && <button className="secondary-button" disabled={disable.isPending} onClick={() => disable.mutate({ roleId: role.role_id, version: role.version })}>Disable</button>}{role.system && <span className="subtle">immutable</span>}</div></td></tr>)}</tbody></table></div> : <Empty>No roles are registered in this tenant.</Empty>}
      {editing && <div className="panel nested-panel"><div className="panel-heading"><div><h2>Permissions for {editing.display_name}</h2><p>Set replace at optimistic version v{editing.version}. Unchecking a key you currently hold can lock you out — the self-escalation guard prevents the reverse.</p></div></div>
        <div className="chip-grid">{catalog.map((permission) => <label key={permission.key} className="chip"><input type="checkbox" checked={editSelected.includes(permission.key)} onChange={(event) => toggleEditSelected(permission.key, event.target.checked)} /><span>{permission.key}</span></label>)}</div>
        <div className="button-row"><button className="primary-button" disabled={setPermissions.isPending} onClick={() => setPermissions.mutate({ roleId: editing.role_id, keys: editSelected, version: editing.version })}>Save permissions</button></div>
        <MutationError {...setPermissions} />
      </div>}
      <MutationError {...disable} />
    </article>
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
  return <>
    <PageHeader eyebrow="POLICY MANAGEMENT" title="Role bindings" description="A binding grants a role over a bounded scope. Suspension, expiry, and revocation take effect on the next request — an unexpired token never wins." />
    <article className="panel"><div className="panel-heading"><div><h2>Create a binding</h2><p>The domain rejects self-escalation: you cannot grant yourself IAM-management authority unless you already hold it via a system bootstrap role.</p></div></div>
      <div className="upload-control">
        <input className="text-input" placeholder="User id" value={form.user_id} onChange={(event) => setForm({ ...form, user_id: event.target.value })} />
        <select className="text-input" value={form.role_id} onChange={(event) => setForm({ ...form, role_id: event.target.value })}><option value="">Choose a role…</option>{rolesQuery.data.filter((role) => role.status === 'active').map((role) => <option key={role.role_id} value={role.role_id}>{role.display_name}</option>)}</select>
        <select className="text-input" value={form.scope} onChange={(event) => setForm({ ...form, scope: event.target.value })}><option value="tenant">Whole tenant</option><option value="org_unit">Organization unit</option><option value="resource_type">Resource kind</option></select>
        {form.scope === 'org_unit' && <input className="text-input" placeholder="Org unit id" value={form.org_unit_id} onChange={(event) => setForm({ ...form, org_unit_id: event.target.value })} />}
        {form.scope === 'resource_type' && <input className="text-input" placeholder="Resource kind" value={form.resource_kind} onChange={(event) => setForm({ ...form, resource_kind: event.target.value })} />}
        <button className="primary-button" disabled={!form.user_id || !form.role_id || !scopeOk || create.isPending} onClick={() => create.mutate()}>{create.isPending ? 'Binding…' : 'Bind role'}</button>
      </div><MutationError {...create} />
    </article>
    <article className="panel"><div className="panel-heading"><div><h2>Bindings</h2><p>{bindingsQuery.data.length} bindings in this tenant</p></div></div>
      {bindingsQuery.data.length ? <div className="table-wrap"><table><thead><tr><th>User</th><th>Role</th><th>Scope</th><th>Status</th><th>Window</th><th>Action</th></tr></thead><tbody>{bindingsQuery.data.map((binding) => <tr key={binding.binding_id}><td className="mono">{binding.user_id.slice(0, 12)}…</td><td className="mono">{binding.role_id.slice(0, 12)}…</td><td className="mono">{binding.scope.scope === 'tenant' ? 'tenant' : binding.scope.scope === 'org_unit' ? `org_unit ${binding.scope.org_unit_id.slice(0, 8)}…` : binding.scope.resource_kind}</td><td><StatusPill value={binding.status} /></td><td>{date(binding.effective_at)}{binding.expires_at ? ` → ${date(binding.expires_at)}` : ''}</td><td>{binding.status === 'active' && <button className="secondary-button" disabled={revoke.isPending} onClick={() => revoke.mutate({ bindingId: binding.binding_id, version: binding.version })}>Revoke</button>}</td></tr>)}</tbody></table></div> : <Empty>No bindings exist in this tenant.</Empty>}
      <MutationError {...revoke} />
    </article>
  </>
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
  const renderUnit = (unit: OrganizationUnitView, depth: number): ReactNode => <>
    <tr key={unit.unit_id}><td><span className="table-primary" style={{ paddingLeft: `${depth * 16}px` }}>{unit.name}</span><span className="table-secondary">{unit.unit_type}</span></td><td><StatusPill value={unit.status} /></td><td><div className="button-row compact"><button className="secondary-button" onClick={() => setSelectedUnit(unit.unit_id)}>Members</button>{unit.status === 'active' && <button className="secondary-button" disabled={disable.isPending} onClick={() => disable.mutate({ unitId: unit.unit_id, version: unit.version })}>Disable</button>}</div></td></tr>
    {unitsQuery.data.filter((child) => child.parent_id === unit.unit_id).map((child) => renderUnit(child, depth + 1))}
  </>
  return <>
    <PageHeader eyebrow="ORGANIZATION" title="Organization units" description="Authorization-only organization: units scope resource grants. Memberships here never become business facts." />
    <article className="panel"><div className="panel-heading"><div><h2>Unit tree</h2><p>{unitsQuery.data.length} units in this tenant</p></div></div>
      <div className="upload-control"><input className="text-input" placeholder="Unit name" value={unitForm.name} onChange={(event) => setUnitForm({ ...unitForm, name: event.target.value })} /><select className="text-input" value={unitForm.unit_type} onChange={(event) => setUnitForm({ ...unitForm, unit_type: event.target.value })}><option value="company">company</option><option value="department">department</option><option value="team">team</option></select><select className="text-input" value={unitForm.parent_id} onChange={(event) => setUnitForm({ ...unitForm, parent_id: event.target.value })}><option value="">No parent (root)</option>{roots.map((unit) => <option key={unit.unit_id} value={unit.unit_id}>{unit.name}</option>)}</select><button className="primary-button" disabled={!unitForm.name || create.isPending} onClick={() => create.mutate()}>{create.isPending ? 'Creating…' : 'Create unit'}</button></div>
      {unitsQuery.data.length ? <div className="table-wrap"><table><thead><tr><th>Unit</th><th>Status</th><th>Actions</th></tr></thead><tbody>{roots.map((unit) => renderUnit(unit, 0))}</tbody></table></div> : <Empty>No organization units yet.</Empty>}
      <MutationError {...create} /><MutationError {...disable} />
    </article>
    {selectedUnit && <article className="panel"><div className="panel-heading"><div><h2>Members of {unitsQuery.data.find((unit) => unit.unit_id === selectedUnit)?.name ?? `${selectedUnit.slice(0, 12)}…`}</h2><p>Add or remove tenant members with idempotent, audited operations.</p></div><button className="secondary-button" onClick={() => setSelectedUnit(null)}>Close</button></div>
      {membersQuery.isPending ? <Loading /> : membersQuery.isError ? <ErrorState error={membersQuery.error} /> : <>
        <div className="upload-control"><input className="text-input" placeholder="Tenant user id" value={memberForm.user_id} onChange={(event) => setMemberForm({ ...memberForm, user_id: event.target.value })} /><select className="text-input" value={memberForm.membership_type} onChange={(event) => setMemberForm({ ...memberForm, membership_type: event.target.value })}><option value="member">member</option><option value="leader">leader</option></select><button className="primary-button" disabled={!memberForm.user_id || addMember.isPending} onClick={() => addMember.mutate({ unitId: selectedUnit, userId: memberForm.user_id, membershipType: memberForm.membership_type })}>Add member</button></div>
        {membersQuery.data.length ? <div className="table-wrap"><table><thead><tr><th>User</th><th>Type</th><th>Status</th><th>Joined</th><th>Action</th></tr></thead><tbody>{membersQuery.data.map((member) => <tr key={member.membership_id}><td className="mono">{member.user_id.slice(0, 12)}…</td><td>{member.membership_type}</td><td><StatusPill value={member.status} /></td><td>{date(member.joined_at)}</td><td>{member.status === 'active' && <button className="secondary-button" disabled={removeMember.isPending} onClick={() => removeMember.mutate({ unitId: selectedUnit, userId: member.user_id, membershipType: member.membership_type, version: member.version })}>Remove</button>}</td></tr>)}</tbody></table></div> : <Empty>This unit has no members.</Empty>}
        <MutationError {...addMember} /><MutationError {...removeMember} />
      </>}
    </article>}
  </>
}

export function ExplainPage() {
  const [form, setForm] = useState({ user_id: '', permission: 'audit.read' })
  const explain = useMutation({ mutationFn: () => explainDecision({ user_id: form.user_id, permission: form.permission }) })
  return <>
    <PageHeader eyebrow="AUTHORIZATION" title="Explain a decision" description="Ask the evaluator, not the database: the same default-DENY engine answers with bounded, redacted reasons for every binding it considered." />
    <article className="panel"><div className="panel-heading"><div><h2>Evaluate one permission</h2><p>Requires the policy.explain permission. The response never contains raw claims, tokens, or store internals.</p></div></div>
      <div className="upload-control"><input className="text-input" placeholder="User id" value={form.user_id} onChange={(event) => setForm({ ...form, user_id: event.target.value })} /><input className="text-input" placeholder="permission.key" value={form.permission} onChange={(event) => setForm({ ...form, permission: event.target.value })} /><button className="primary-button" disabled={!form.user_id || !form.permission || explain.isPending} onClick={() => explain.mutate()}>{explain.isPending ? 'Evaluating…' : 'Explain'}</button></div>
      <MutationError {...explain} />
      {explain.data && <div className="detail-grid" style={{ marginTop: 16 }}>
        <article className="panel detail-card"><span className="eyebrow">DECISION</span><StatusPill value={explain.data.allowed ? 'allow' : 'deny'} /><dl><dt>Reason</dt><dd className="mono">{explain.data.reason}</dd>{explain.data.matched_binding && <><dt>Matched binding</dt><dd className="mono">{explain.data.matched_binding}</dd></>}{explain.data.matched_scope && <><dt>Scope</dt><dd className="mono">{JSON.stringify(explain.data.matched_scope)}</dd></>}<dt>Policy reference</dt><dd className="mono">{explain.data.policy_reference}</dd></dl></article>
        <article className="panel"><div className="panel-heading"><div><h2>Bindings considered</h2><p>{explain.data.evaluations.length} candidate evaluations</p></div></div>
          {explain.data.evaluations.length ? <div className="table-wrap"><table><thead><tr><th>Binding</th><th>Active</th><th>In validity</th><th>Grants key</th><th>Outcome</th></tr></thead><tbody>{explain.data.evaluations.map((evaluation) => <tr key={evaluation.binding_id}><td className="mono">{evaluation.binding_id.slice(0, 12)}…</td><td>{evaluation.binding_active ? 'yes' : 'no'}</td><td>{evaluation.within_validity ? 'yes' : 'no'}</td><td>{evaluation.role_grants_permission ? 'yes' : 'no'}</td><td><StatusPill value={evaluation.outcome} /></td></tr>)}</tbody></table></div> : <Empty>No candidate binding existed for this subject and tenant.</Empty>}
        </article>
      </div>}
    </article>
  </>
}
