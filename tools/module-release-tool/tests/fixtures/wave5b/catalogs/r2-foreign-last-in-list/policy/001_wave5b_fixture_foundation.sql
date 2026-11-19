-- @bp-owned: wave5b_fixture_policy_t1
CREATE TABLE wave5b_fixture_policy_t1 (id BIGINT);
INSERT INTO wave5b_fixture_policy_t1 (id) SELECT b.id FROM roles a, role_permissions b, platform_users p;