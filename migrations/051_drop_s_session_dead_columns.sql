-- 051_drop_s_session_dead_columns.sql — s_sessionの死にカラム整理（品質改善タスクP1-6）
--
-- 3カラムとも参照が完全に無くなったことを確認済み:
-- - mfa_verified: エンジニアセッション作成時に書き込むだけで、どこからも読まれていなかった
--   （マイグレーション050適用と同時にINSERT文からも削除済み）
-- - webauthn_reg_state: 旧パスキー登録フロー（security.rs）がsophia_sessionセッションIDに
--   紐付けて使っていたが、P0-1でs_passkey_login_challengeテーブル方式に切り替えたため無参照化
-- - webauthn_auth_state: 追加当初から一度も参照されていなかった

ALTER TABLE s_session DROP COLUMN IF EXISTS mfa_verified;
ALTER TABLE s_session DROP COLUMN IF EXISTS webauthn_reg_state;
ALTER TABLE s_session DROP COLUMN IF EXISTS webauthn_auth_state;
