-- Wave 5B-S1 shared fixture: a policy-owned table plus its unique index.
-- @bp-owned: wave5b_fixture_policy_t1
CREATE TABLE wave5b_fixture_policy_t1 (
    t1_id BIGINT PRIMARY KEY,
    stable_key TEXT NOT NULL
);

CREATE UNIQUE INDEX ux_wave5b_fixture_policy_t1_key
    ON wave5b_fixture_policy_t1 (stable_key);