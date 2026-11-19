-- @bp-owned: wave5b_fixture_policy_t1
CREATE TABLE wave5b_fixture_policy_t1 (payload TEXT);

-- FROM platform_users, USING x
INSERT INTO wave5b_fixture_policy_t1 (payload) VALUES ('FROM platform_users, roles USING y');