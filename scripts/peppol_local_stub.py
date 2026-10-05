#!/usr/bin/env python3
"""
scripts/peppol_local_stub.py — Peppolプロバイダー役のローカルテストスタブ（開発専用）

実際のPeppol認定プロバイダーをまだ契約していない状態で、Sophiaのoutbound送信
（infrastructure::peppol_client::PeppolClient）とinbound受信（/api/v1/peppol/inbound）を
ローカルだけで疎通確認するためのツール。本番コード・デプロイには含まれない。
外部ライブラリ依存なし（標準ライブラリのみ）。

使い方:
  1. スタブを起動:
       python3 scripts/peppol_local_stub.py --port 9000

  2. Sophia側 .env.local に以下を追加してdev.shを再起動:
       PEPPOL_API_BASE_URL=http://localhost:9000
       PEPPOL_API_KEY=test-key
       PEPPOL_OWN_PARTICIPANT_ID=0221:sophia-test

  3. 【請求書/支払通知書の送信テスト】
     マスタメンテでテスト対象クライアント/パートナーの「Peppol参加者ID」を
     適当な値（例: 0221:test-client）に設定した上で、Sophia画面から
     請求書/支払通知書詳細の「Peppol送信」ボタンを押す。
     → 下記で受信内容を確認できる:
       curl http://localhost:9000/received

  4. 【発注書の受信テスト（仮想会社→Sophia）】
     事前にm_clientの登録番号 or Peppol参加者IDを、テストしたい値に設定しておく。
     その上でスタブに「発注書を送らせる」よう指示する:
       curl -X POST http://localhost:9000/simulate/send-order \\
         -H "Content-Type: application/json" \\
         -d '{
               "sophia_base_url": "http://localhost:8111",
               "sender_registration_no": "T1234567890123",
               "work_start": "2026-09-01",
               "work_end": "2026-09-30",
               "project_name": "Peppol疎通テスト案件",
               "lines": [
                 {"item_name": "テストエンジニア", "quantity": 1.0, "unit_price": 700000, "line_amount": 700000}
               ]
             }'
     → Sophia側で受注書（t_received_order）が新規作成されることを確認する
       （画面の「案件」または /api/v1/received-orders 等で確認）。
"""

import json
import sys
import uuid
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

RECEIVED = []  # Sophiaから送信されてきた文書を保持（テスト確認用）


class Handler(BaseHTTPRequestHandler):
    def _send_json(self, status, payload):
        body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _read_json(self):
        length = int(self.headers.get("Content-Length", 0) or 0)
        raw = self.rfile.read(length) if length else b"{}"
        return json.loads(raw or b"{}")

    def do_POST(self):
        if self.path in ("/v1/invoices", "/v1/self-billing-invoices"):
            payload = self._read_json()
            message_id = str(uuid.uuid4())
            RECEIVED.append({"path": self.path, "payload": payload, "message_id": message_id})
            print(f"[stub] 受信: {self.path} -> message_id={message_id}", file=sys.stderr)
            self._send_json(200, {"message_id": message_id, "status": "SENT"})
            return

        if self.path == "/simulate/send-order":
            body = self._read_json()
            sophia_base_url = body.get("sophia_base_url", "http://localhost:8111").rstrip("/")
            order_payload = {
                "document_type": "ORDER",
                "peppol_message_id": str(uuid.uuid4()),
                "sender_participant_id": body.get("sender_participant_id", ""),
                "sender_registration_no": body.get("sender_registration_no", ""),
                "work_start": body.get("work_start"),
                "work_end": body.get("work_end"),
                "project_name": body.get("project_name", "テスト発注（Peppolスタブ）"),
                "lines": body.get("lines", []),
            }
            req = urllib.request.Request(
                f"{sophia_base_url}/api/v1/peppol/inbound",
                data=json.dumps(order_payload).encode("utf-8"),
                headers={"Content-Type": "application/json"},
                method="POST",
            )
            try:
                with urllib.request.urlopen(req, timeout=10) as resp:
                    result = json.loads(resp.read())
                self._send_json(200, {"sent_to_sophia": True, "sophia_response": result, "order_payload": order_payload})
            except Exception as e:  # noqa: BLE001 — テストツールのため簡潔なエラー返却で十分
                self._send_json(502, {"sent_to_sophia": False, "error": str(e), "order_payload": order_payload})
            return

        self._send_json(404, {"error": "not found"})

    def do_GET(self):
        if self.path == "/received":
            self._send_json(200, RECEIVED)
            return
        if self.path == "/received/latest":
            self._send_json(200, RECEIVED[-1] if RECEIVED else {})
            return
        self._send_json(404, {"error": "not found"})

    def log_message(self, fmt, *args):
        sys.stderr.write("[stub] " + (fmt % args) + "\n")


def main():
    port = 9000
    if "--port" in sys.argv:
        port = int(sys.argv[sys.argv.index("--port") + 1])
    server = ThreadingHTTPServer(("0.0.0.0", port), Handler)
    print(f"[stub] Peppolローカルテストスタブ起動: http://localhost:{port}")
    print(f"[stub]  受信確認:             GET  http://localhost:{port}/received")
    print(f"[stub]  発注書送信シミュレーション: POST http://localhost:{port}/simulate/send-order")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\n[stub] 停止しました")


if __name__ == "__main__":
    main()
