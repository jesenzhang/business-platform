-- @bp-owned: wave5b_fixture_iam_t2
CREATE TABLE wave5b_fixture_iam_t2 (
    t2_id BIGINT PRIMARY KEY REFERENCES platform_users (t2_id)
);