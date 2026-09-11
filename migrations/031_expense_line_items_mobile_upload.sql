-- ============================================================
-- 031_expense_line_items_mobile_upload.sql
-- 経費申請の複数明細対応 + スマホカメラ連携アップロード機能
-- ============================================================
-- これまで t_expense_request は「1行=1申請=1明細」のフラットな構造だったが、
-- 交通費の複数チケットなど1申請に複数の明細（日付・経路・金額・領収書）を
-- 持たせたいという要件のため、ヘッダー(t_expense_request)+明細(t_expense_request_item)
-- の親子構造に変更する。
--
-- 領収書画像はファイルシステムではなくPostgresのBYTEAに保存する
-- （MINISFORUM/NAS間でのコンテナ再作成時にファイルシステムの永続化が
-- 保証されないため。DBに入れておけば既存のバックアップ運用にそのまま乗る）。

-- ── 1. 明細テーブルを新設 ──
CREATE TABLE t_expense_request_item (
    id                  BIGSERIAL PRIMARY KEY,
    expense_request_id  BIGINT NOT NULL REFERENCES t_expense_request(id) ON DELETE CASCADE,
    expense_date        DATE NOT NULL,
    category            VARCHAR(30) NOT NULL DEFAULT 'OTHER',
    description         TEXT NOT NULL DEFAULT '',
    amount              INTEGER NOT NULL DEFAULT 0,
    receipt_image       BYTEA,
    receipt_mime        VARCHAR(50),
    display_order       INTEGER NOT NULL DEFAULT 0,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_expense_request_item_request ON t_expense_request_item(expense_request_id);

-- ── 2. 既存の1行1明細データを明細テーブルへ移行 ──
INSERT INTO t_expense_request_item
    (expense_request_id, expense_date, category, description, amount, display_order, created_at, updated_at)
SELECT id, expense_date, category, description, amount, 0, created_at, updated_at
FROM t_expense_request;

-- ── 3. ヘッダーテーブルから明細列を削除し、合計金額列を追加 ──
ALTER TABLE t_expense_request
    DROP COLUMN expense_date,
    DROP COLUMN category,
    DROP COLUMN amount,
    DROP COLUMN description,
    DROP COLUMN receipt_file,
    ADD COLUMN total_amount INTEGER NOT NULL DEFAULT 0;

UPDATE t_expense_request er
SET total_amount = COALESCE(
    (SELECT SUM(amount) FROM t_expense_request_item WHERE expense_request_id = er.id), 0
);

COMMENT ON TABLE t_expense_request IS '経費申請ヘッダー（明細はt_expense_request_item）';
COMMENT ON COLUMN t_expense_request.total_amount IS '明細金額の合計（明細の追加・更新・削除時にアプリ側で再計算する非正規化列）';

-- ── 4. スマホ連携アップロード用の一時トークンテーブル ──
-- アップロードされた画像はこのテーブルに一時保存せず、アップロード処理内で直接
-- t_expense_request_item.receipt_image に書き込む（使い捨てトークンなので中間状態を
-- 増やさずシンプルにする）。このテーブルはトークンの有効性（期限・使用済み）管理のみ担う。
CREATE TABLE s_mobile_upload_token (
    token                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    expense_request_item_id BIGINT NOT NULL REFERENCES t_expense_request_item(id) ON DELETE CASCADE,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at              TIMESTAMPTZ NOT NULL,
    used_at                 TIMESTAMPTZ
);

CREATE INDEX idx_mobile_upload_token_item ON s_mobile_upload_token(expense_request_item_id);

COMMENT ON TABLE s_mobile_upload_token IS 'スマホから経費明細の領収書写真をアップロードするための一時トークン。有効期限10分・使い切り（used_atが立ったら再利用不可）。画像自体はt_expense_request_item.receipt_imageに直接書き込む';
