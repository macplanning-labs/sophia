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

# ── 送り元(クライアントIP)の確定 ──
# Cloudflare のトンネル(cloudflared)は、サーバ本体(本番機 192.0.2.10 / 予備機 192.0.2.11 は例。環境に合わせて書き換える)の上か、
# docker のネットワーク越し(172.16.0.0/12 の中継)で nginx につなぐ。そこから来たときだけ、Cloudflare が入れる
# CF-Connecting-IP(送り手は書き換えられない)を本当の送り元として採用する。それ以外(LAN の別の機械から直接など)は接続元の IP。
# 送り手が自由に書ける X-Forwarded-For の先頭は使わない(偽装で回数制限・ログインの試行ロックをすり抜けられたため)。
set_real_ip_from 127.0.0.1;
set_real_ip_from 192.0.2.10;
set_real_ip_from 192.0.2.11;
set_real_ip_from 172.16.0.0/12;
real_ip_header CF-Connecting-IP;
real_ip_recursive off;

# 外部向け WebAPI（APIキー認証、/api/ プレフィックス外）
location /v1/ {
    proxy_pass http://backend;
    proxy_set_header Host $http_host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $remote_addr;
    proxy_set_header CF-Connecting-IP $remote_addr;
    proxy_set_header X-Forwarded-Proto $real_scheme;
}

# API（/api/v1/... を含む）→ Rust backend
location /api/ {
    proxy_pass http://backend;
    proxy_set_header Host $http_host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $remote_addr;
    proxy_set_header CF-Connecting-IP $remote_addr;
    proxy_set_header X-Forwarded-Proto $real_scheme;
}

# それ以外すべて → Next.js frontend（画面パスはここに落ちる）
location / {
    proxy_pass http://frontend;
    proxy_set_header Host $http_host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $remote_addr;
    proxy_set_header CF-Connecting-IP $remote_addr;
    proxy_set_header X-Forwarded-Proto $real_scheme;
}
