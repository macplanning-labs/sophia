# ============================================================
# sophia_locations.inc — パス振り分け（local / staging / production 共通）
# ============================================================
# 前提: upstream backend / frontend と map $real_scheme が定義済み
# include 先: nginx/{local,staging,production}.conf の server {} 内
#
# §14 URL・ルーティング設計:
#   「/api/ → Rust、それ以外 → Next.js」の2本のみ。
#   機能ドメインごとの location は新規追加禁止。

client_max_body_size 50M;

# 外部向け WebAPI（APIキー認証、/api/ プレフィックス外）
location /v1/ {
    proxy_pass http://backend;
    proxy_set_header Host $http_host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $real_scheme;
}

# API（/api/v1/... を含む）→ Rust backend
location /api/ {
    proxy_pass http://backend;
    proxy_set_header Host $http_host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $real_scheme;
}

# それ以外すべて → Next.js frontend（画面パスはここに落ちる）
location / {
    proxy_pass http://frontend;
    proxy_set_header Host $http_host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $real_scheme;
}
