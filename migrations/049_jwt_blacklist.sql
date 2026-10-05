-- 049_jwt_blacklist.sql — 社員/管理者JWTの失効管理（Step3）

CREATE TABLE s_jwt_blacklist (
    jti         UUID PRIMARY KEY,
    expires_at  TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_jwt_blacklist_expires ON s_jwt_blacklist(expires_at);
