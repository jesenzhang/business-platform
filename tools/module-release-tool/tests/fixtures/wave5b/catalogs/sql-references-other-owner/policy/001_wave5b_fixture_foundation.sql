-- @bp-owned: wave5b_fixture_policy_t3
CREATE TABLE wave5b_fixture_policy_t3 (
    t3_id BIGINT NOT NULL REFERENCES platform_users (t1_id)
);