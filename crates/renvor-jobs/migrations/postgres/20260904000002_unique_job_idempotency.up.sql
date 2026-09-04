CREATE UNIQUE INDEX ux_job_idempotency ON rv_job (queue, idempotency_key);
