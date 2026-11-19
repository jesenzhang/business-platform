-- @bp-owned: wave5b_fixture_policy_t1
CREATE TABLE wave5b_fixture_policy_t1 (id BIGINT);
INSERT INTO wave5b_fixture_policy_t1 (id) SELECT a.id FROM roles a, platform_users p, role_permissions b;