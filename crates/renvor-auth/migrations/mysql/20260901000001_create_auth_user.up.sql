CREATE TABLE rv_auth_user (id BINARY(16) PRIMARY KEY, email VARCHAR(320) NOT NULL UNIQUE, email_verified_at DATETIME(6), created_at DATETIME(6) NOT NULL, updated_at DATETIME(6) NOT NULL);
