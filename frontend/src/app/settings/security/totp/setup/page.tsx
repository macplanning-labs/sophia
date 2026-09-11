"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { useQueryClient } from "@tanstack/react-query";
import { ArrowLeft } from "lucide-react";

function friendlyError(raw: string, fallback: string): string {
  const t = (raw || "").trim();
  if (!t) return fallback;
  if (t.startsWith("<!") || t.startsWith("<html") || t.includes("<!DOCTYPE")) {
    return fallback;
  }
  return t.length > 200 ? `${t.slice(0, 200)}…` : t;
}

export default function TotpSetupPage() {
  const router = useRouter();
  const queryClient = useQueryClient();
  const [loading, setLoading] = useState(true);
  const [qrBase64, setQrBase64] = useState("");
  const [secretBase32, setSecretBase32] = useState("");
  const [secretB64, setSecretB64] = useState("");
  const [code, setCode] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const res = await fetch("/api/v1/settings/security/totp/begin", {
          method: "POST",
          credentials: "include",
        });
        if (!res.ok) {
          const text = await res.text();
          throw new Error(friendlyError(text, "TOTPセットアップを開始できません"));
        }
        const data = await res.json();
        setQrBase64(data.qr_base64);
        setSecretBase32(data.secret_base32);
        setSecretB64(data.secret_b64);
      } catch (e) {
        setError(e instanceof Error ? e.message : "TOTPセットアップを開始できません");
      } finally {
        setLoading(false);
      }
    })();
  }, []);

  const handleConfirm = async () => {
    setError(null);
    setConfirming(true);
    try {
      const res = await fetch("/api/v1/settings/security/totp/confirm", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        credentials: "include",
        body: JSON.stringify({ secret_b64: secretB64, code }),
      });
      if (!res.ok) {
        const text = await res.text();
        throw new Error(friendlyError(text, "確認コードが正しくありません"));
      }
      queryClient.clear();
      router.push("/settings/security");
    } catch (e) {
      setError(e instanceof Error ? e.message : "確認コードが正しくありません");
    } finally {
      setConfirming(false);
    }
  };

  return (
    <div className="p-6 max-w-lg">
      <button
        onClick={() => router.push("/settings/security")}
        className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground mb-4"
      >
        <ArrowLeft className="w-3.5 h-3.5" />
        セキュリティ設定に戻る
      </button>

      {loading ? (
        <p className="text-sm text-muted-foreground">読み込み中...</p>
      ) : (
        <div className="bg-card border border-border rounded-lg p-5 space-y-4">
          <div>
            <p className="text-sm text-muted-foreground mb-2">
              Google Authenticator等のアプリでQRコードを読み取ってください。
            </p>
            {qrBase64 && (
              // eslint-disable-next-line @next/next/no-img-element
              <img
                src={`data:image/png;base64,${qrBase64}`}
                alt="TOTP QRコード"
                className="w-48 h-48 bg-white p-2 rounded"
              />
            )}
          </div>

          {secretBase32 && (
            <div>
              <p className="text-xs text-muted-foreground mb-1">
                QRコードを読み取れない場合、以下のキーを手動入力してください:
              </p>
              <code className="text-xs bg-muted px-2 py-1 rounded break-all">{secretBase32}</code>
            </div>
          )}

          <div>
            <label className="text-xs text-muted-foreground block mb-1">
              アプリに表示された6桁のコードを入力
            </label>
            <input
              type="text"
              inputMode="numeric"
              maxLength={6}
              value={code}
              onChange={(e) => setCode(e.target.value.replace(/\D/g, ""))}
              className="bg-background border border-border rounded-md px-3 py-2 text-sm w-32 tracking-widest"
              placeholder="123456"
            />
          </div>

          {error && <p className="text-xs text-rose-400">{error}</p>}

          <button
            onClick={handleConfirm}
            disabled={code.length !== 6 || confirming || !secretB64}
            className="px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed rounded-md text-sm transition-colors"
          >
            {confirming ? "確認中..." : "確認して有効化"}
          </button>
        </div>
      )}
    </div>
  );
}
