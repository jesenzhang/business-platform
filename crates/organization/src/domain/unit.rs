//! `OrganizationUnit` — a node of the tenant-scoped authorization tree.
//!
//! Invariants: parent is in the same tenant, never self, tree is acyclic and
//! bounded in depth. Disabled units never accept new members and can never
//! authorize an org-scoped policy decision (enforced by consumers).

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::error::OrganizationDomainError;
use super::version::AggregateVersion;

/// Field cap for unit display names.
pub const MAX_UNIT_NAME_LEN: usize = 200;

/// Hard depth bound for the unit tree. Authorization scoping and cycle
/// checks are O(depth); the cap keeps both bounded.
pub const MAX_UNIT_DEPTH: usize = 64;

/// Kind of organization unit. Kept deliberately coarse (authorization
/// scoping, not HR taxonomy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrganizationUnitType {
    /// Top-level company/legal entity.
    Company,
    /// Department under a company or department.
    Department,
    /// Team under any unit.
    Team,
}

impl OrganizationUnitType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Company => "company",
            Self::Department => "department",
            Self::Team => "team",
        }
    }
}

/// Unit lifecycle status. Disabled units are inert, not deleted: membership
/// history and policy references remain resolvable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrganizationUnitStatus {
    /// The unit participates in scoping and accepts new members.
    Active,
    /// The unit is switched off.
    Disabled,
}

impl OrganizationUnitStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }
}

fn validate_name(name: &str) -> Result<(), OrganizationDomainError> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_UNIT_NAME_LEN || name.chars().any(char::is_control)
    {
        return Err(OrganizationDomainError::InvalidName {
            max: MAX_UNIT_NAME_LEN,
        });
    }
    Ok(())
}

/// The organization unit aggregate. Fields are private; adapters restore
/// state only through [`OrganizationUnit::rehydrate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrganizationUnit {
    unit_id: Uuid,
    tenant_id: Uuid,
    parent_id: Option<Uuid>,
    unit_type: OrganizationUnitType,
    name: String,
    status: OrganizationUnitStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    version: AggregateVersion,
}

/// Raw persisted state accepted by [`OrganizationUnit::rehydrate`].
#[derive(Debug, Clone)]
pub struct RehydrateOrganizationUnit {
    /// Unit id.
    pub unit_id: Uuid,
    /// Tenant boundary.
    pub tenant_id: Uuid,
    /// Parent unit (same tenant), `None` for roots.
    pub parent_id: Option<Uuid>,
    /// Unit kind.
    pub unit_type: OrganizationUnitType,
    /// Display name.
    pub name: String,
    /// Persisted status.
    pub status: OrganizationUnitStatus,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last mutation timestamp.
    pub updated_at: DateTime<Utc>,
    /// Persisted aggregate version.
    pub version: i64,
}

impl OrganizationUnit {
    /// Create a unit. Parent existence/tenant/cycle validation is the
    /// application + store job; self-parenthood is rejected here.
    pub fn create(
        unit_id: Uuid,
        tenant_id: Uuid,
        parent_id: Option<Uuid>,
        unit_type: OrganizationUnitType,
        name: &str,
        now: DateTime<Utc>,
    ) -> Result<Self, OrganizationDomainError> {
        if unit_id.is_nil() || tenant_id.is_nil() {
            return Err(OrganizationDomainError::InvalidIdentity);
        }
        if parent_id == Some(unit_id) {
            return Err(OrganizationDomainError::InvalidIdentity);
        }
        validate_name(name)?;
        Ok(Self {
            unit_id,
            tenant_id,
            parent_id,
            unit_type,
            name: name.trim().to_string(),
            status: OrganizationUnitStatus::Active,
            created_at: now,
            updated_at: now,
            version: AggregateVersion::initial(),
        })
    }

    /// Restore a persisted unit after full validation.
    pub fn rehydrate(state: RehydrateOrganizationUnit) -> Result<Self, OrganizationDomainError> {
        let RehydrateOrganizationUnit {
            unit_id,
            tenant_id,
            parent_id,
            unit_type,
            name,
            status,
            created_at,
            updated_at,
            version,
        } = state;
        if unit_id.is_nil() || tenant_id.is_nil() {
            return Err(OrganizationDomainError::InvalidIdentity);
        }
        if parent_id == Some(unit_id) {
            return Err(OrganizationDomainError::InvalidIdentity);
        }
        validate_name(&name)?;
        let version =
            AggregateVersion::new(version).map_err(|_| OrganizationDomainError::InvalidVersion)?;
        if updated_at < created_at {
            return Err(OrganizationDomainError::InvalidTransition {
                operation: "rehydrate-timestamps",
                status: status.as_str(),
            });
        }
        Ok(Self {
            unit_id,
            tenant_id,
            parent_id,
            unit_type,
            name: name.trim().to_string(),
            status,
            created_at,
            updated_at,
            version,
        })
    }

    #[must_use]
    pub const fn unit_id(&self) -> Uuid {
        self.unit_id
    }

    #[must_use]
    pub const fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    #[must_use]
    pub const fn parent_id(&self) -> Option<Uuid> {
        self.parent_id
    }

    #[must_use]
    pub const fn unit_type(&self) -> OrganizationUnitType {
        self.unit_type
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn status(&self) -> OrganizationUnitStatus {
        self.status
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self.status, OrganizationUnitStatus::Active)
    }

    #[must_use]
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    #[must_use]
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    #[must_use]
    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    /// Apply a management update (rename / retyping / status change). At
    /// least one field must be present; version increments once per applied
    /// mutation.
    pub fn update(
        &mut self,
        name: Option<&str>,
        unit_type: Option<OrganizationUnitType>,
        status: Option<OrganizationUnitStatus>,
        now: DateTime<Utc>,
    ) -> Result<(), OrganizationDomainError> {
        if name.is_none() && unit_type.is_none() && status.is_none() {
            return Ok(());
        }
        if let Some(name) = name {
            validate_name(name)?;
            self.name = name.trim().to_string();
        }
        if let Some(unit_type) = unit_type {
            self.unit_type = unit_type;
        }
        if let Some(status) = status {
            self.status = status;
        }
        self.touch(now)
    }

    /// Reparent the unit. The caller (application + store) must have already
    /// proven same-tenant, existence, non-cycle, and depth safety for
    /// `new_parent`.
    pub fn reparent(
        &mut self,
        new_parent: Option<Uuid>,
        now: DateTime<Utc>,
    ) -> Result<(), OrganizationDomainError> {
        if new_parent == Some(self.unit_id) {
            return Err(OrganizationDomainError::InvalidIdentity);
        }
        self.parent_id = new_parent;
        self.touch(now)
    }

    fn touch(&mut self, now: DateTime<Utc>) -> Result<(), OrganizationDomainError> {
        if now > self.updated_at {
            self.updated_at = now;
        }
        self.version = self
            .version
            .increment()
            .map_err(|_| OrganizationDomainError::InvalidVersion)?;
        Ok(())
    }
}

/// Tree-shape validation over a tenant's complete `(unit_id, parent_id)`
/// snapshot: rejects self-parenthood, unknown parents, cycles, and trees
/// deeper than [`MAX_UNIT_DEPTH`] after the prospective placement of
/// `placed_unit` under `placed_parent`.
///
/// Returns `Ok(())` when the placement keeps the tenant tree valid.
pub fn validate_tree_placement(
    snapshot: &[(Uuid, Option<Uuid>)],
    placed_unit: Uuid,
    placed_parent: Option<Uuid>,
) -> Result<(), OrganizationDomainError> {
    // Apply the prospective placement to the parent lookup.
    let parent_of = |unit: Uuid| -> Option<Uuid> {
        if unit == placed_unit {
            return placed_parent;
        }
        snapshot
            .iter()
            .find(|(id, _)| *id == unit)
            .and_then(|(_, parent)| *parent)
    };
    let known = |unit: Uuid| unit == placed_unit || snapshot.iter().any(|(id, _)| *id == unit);

    if let Some(parent) = parent_of(placed_unit) {
        if parent == placed_unit {
            return Err(OrganizationDomainError::InvalidIdentity);
        }
        if !known(parent) {
            return Err(OrganizationDomainError::InvalidIdentity);
        }
    }

    // Walk every node's ancestor chain: bounded traversal ⇒ a revisit means
    // a cycle; exceeding the depth bound fails closed.
    for unit_id in snapshot
        .iter()
        .map(|(id, _)| *id)
        .chain(std::iter::once(placed_unit))
    {
        let mut cursor = unit_id;
        let mut steps = 0usize;
        let mut visited = std::collections::BTreeSet::new();
        while let Some(parent) = parent_of(cursor) {
            steps += 1;
            if steps > MAX_UNIT_DEPTH || !visited.insert(parent) {
                return Err(OrganizationDomainError::InvalidIdentity);
            }
            cursor = parent;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(seconds, 0)
            .single()
            .unwrap_or_else(|| unreachable!())
    }

    fn id(byte: u8) -> Uuid {
        Uuid::from_bytes([byte; 16])
    }

    #[test]
    fn create_rejects_nil_self_parent_and_bad_names() {
        assert!(OrganizationUnit::create(
            Uuid::nil(),
            id(2),
            None,
            OrganizationUnitType::Company,
            "A",
            ts(1)
        )
        .is_err());
        assert!(OrganizationUnit::create(
            id(1),
            id(2),
            Some(id(1)),
            OrganizationUnitType::Company,
            "A",
            ts(1)
        )
        .is_err());
        assert!(OrganizationUnit::create(
            id(1),
            id(2),
            None,
            OrganizationUnitType::Company,
            "  ",
            ts(1)
        )
        .is_err());
        assert!(OrganizationUnit::create(
            id(1),
            id(2),
            None,
            OrganizationUnitType::Company,
            &"n".repeat(MAX_UNIT_NAME_LEN + 1),
            ts(1)
        )
        .is_err());
        assert!(OrganizationUnit::create(
            id(1),
            id(2),
            None,
            OrganizationUnitType::Company,
            "bad\u{0}name",
            ts(1)
        )
        .is_err());
    }

    #[test]
    fn update_and_reparent_increment_versions_and_revalidate() {
        let mut unit = OrganizationUnit::create(
            id(1),
            id(2),
            None,
            OrganizationUnitType::Company,
            "Acme",
            ts(100),
        )
        .unwrap_or_else(|_| unreachable!());
        assert_eq!(unit.version().value(), 1);

        // Empty update is a no-op without version churn.
        unit.update(None, None, None, ts(150))
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(unit.version().value(), 1);

        unit.update(
            Some("Acme Corp"),
            Some(OrganizationUnitType::Department),
            None,
            ts(200),
        )
        .unwrap_or_else(|_| unreachable!());
        assert_eq!(unit.name(), "Acme Corp");
        assert_eq!(unit.unit_type(), OrganizationUnitType::Department);
        assert_eq!(unit.version().value(), 2);

        assert!(unit.update(Some(""), None, None, ts(300)).is_err());
        assert_eq!(unit.version().value(), 2);

        unit.reparent(Some(id(9)), ts(300))
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(unit.parent_id(), Some(id(9)));
        assert_eq!(unit.version().value(), 3);
        assert!(unit.reparent(Some(id(1)), ts(400)).is_err());
    }

    #[test]
    fn placement_rejects_cycles_unknown_parents_and_depth() {
        // Linear chain: 1 -> 2 -> 3 (1 is root).
        let snapshot = vec![(id(1), None), (id(2), Some(id(1))), (id(3), Some(id(2)))];
        // Legal move: place 2 under 3 → 1 -> 3 -> 2 is still a tree? That
        // would make 2 a descendant of 3 while 2 was 3's ancestor: cycle.
        assert!(validate_tree_placement(&snapshot, id(2), Some(id(3))).is_err());
        // Unknown parent.
        assert!(validate_tree_placement(&snapshot, id(2), Some(id(8))).is_err());
        // Self parent.
        assert!(validate_tree_placement(&snapshot, id(2), Some(id(2))).is_err());
        // Legal re-parenting: 3 under root 1.
        assert!(validate_tree_placement(&snapshot, id(3), Some(id(1))).is_ok());
        // Detach to root.
        assert!(validate_tree_placement(&snapshot, id(2), None).is_ok());

        // Depth: chain longer than the cap is rejected.
        let deep: Vec<(Uuid, Option<Uuid>)> = (0..=MAX_UNIT_DEPTH)
            .map(|index| {
                (
                    id(u8::try_from(index + 1).unwrap_or(u8::MAX)),
                    if index == 0 {
                        None
                    } else {
                        Some(id(u8::try_from(index).unwrap_or(u8::MAX)))
                    },
                )
            })
            .collect();
        assert!(validate_tree_placement(&deep, id(2), None).is_ok());
        assert!(validate_tree_placement(
            &deep,
            id(1),
            Some(id(u8::try_from(MAX_UNIT_DEPTH + 1).unwrap_or(u8::MAX)))
        )
        .is_err());
    }
}
