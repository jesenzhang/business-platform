-- @bp-owned: wave5b_fixture_policy_t1
CREATE TABLE wave5b_fixture_policy_t1 (id BIGINT);
ALTER TABLE wave5b_fixture_policy_t1 ATTACH PARTITION platform_users DEFAULT;