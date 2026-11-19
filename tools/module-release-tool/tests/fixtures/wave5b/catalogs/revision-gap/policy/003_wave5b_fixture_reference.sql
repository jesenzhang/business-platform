-- Wave 5B-S1 shared fixture: a later revision may reference an earlier one.
-- @bp-owned: wave5b_fixture_policy_t2
CREATE TABLE wave5b_fixture_policy_t2 (
    t2_id BIGINT PRIMARY KEY,
    t1_id BIGINT NOT NULL REFERENCES wave5b_fixture_policy_t1 (t1_id)
);

INSERT INTO wave5b_fixture_policy_t2 (t2_id, t1_id)
VALUES (1, 1);

ALTER TABLE wave5b_fixture_policy_t2
    ADD CONSTRAINT fk_wave5b_fixture_policy_t2_t1 FOREIGN KEY (t1_id)
    REFERENCES wave5b_fixture_policy_t1 (t1_id);

COMMENT ON TABLE wave5b_fixture_policy_t2 IS 'Wave 5B-S1 fixture table';