"use client";

import { useCallback, useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { useQueryClient } from "@tanstack/react-query";
import { fetchSecurity } from "@/lib/api";
import { Key, Smartphone, AlertCircle } from "lucide-react";

interface PasskeyInfo {
  id: number;
  name: string;
  created_at: string;
}

interface SecurityData {
  user: {
    email: string;
    username: string;
    mfa_enabled: boolean;
  };
  totp_active: boolean;
  passkeys: PasskeyInfo[];
  mfa_setup_required?: boolean;
  mfa_required_for_role?: boolean;
}

/** HTMLエラーページが返ってきたときに画面を埋め尽くさない */
function friendlyError(raw: string, fallback: string): string {
  const t = (raw || "").trim();
  if (!t) return fallback;
  if (t.startsWith("<!") || t.startsWith("<html") || t.includes("<!DOCTYPE")) {
    return fallback;
  }
  return t.length > 200 ? `${t.slice(0, 200)}…` : t;
}

export default function SecurityPage() {
  const router = useRouter();
  const queryClient = useQueryClient();
  const [data, setData] = useState<SecurityData | null>(null);
  const [loading, setLoading] = useState(true);
  const [registering, setRegistering] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    const next = await fetchSecurity();
    setData(next);
  }, []);

  useEffect(() => {
    reload()
      .catch((e) => console.error("security fetch:", e))
      .finally(() => setLoading(false));
  }, [reload]);

  const handleRegisterPasskey = async () => {
    setError(null);
    setMessage(null);
    setRegistering(true);
    try {
      const beginRes = await fetch("/api/v1/settings/security/passkey/register/begin", {
        method: "POST",
        credentials: "include",
      });
      if (!beginRes.ok) {
        const text = await beginRes.text();
        throw new Error(friendlyError(text, "パスキー登録を開始できません"));
      }
      const ccr = await beginRes.json();
      // webauthn-rs は { publicKey: ... }、simplewebauthn v13 は optionsJSON に中身を渡す
      const optionsJSON = ccr.publicKey ?? ccr;

      const { startRegistration } = await import("@simplewebauthn/browser");
      const credential = await startRegistration({ optionsJSON });

      const completeRes = await fetch("/api/v1/settings/security/passkey/register/complete", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        credentials: "include",
        body: JSON.stringify(credential),
      });
      if (!completeRes.ok) {
        const text = await completeRes.text();
        throw new Error(friendlyError(text, "パスキー登録に失敗しました"));
      }

      setMessage("パスキーを登録しました。他の画面が使えるようになります。");
      queryClient.clear();
      await reload();
    } catch (e) {
      const msg = e instanceof Error ? e.message : "パスキー登録に失敗しました";
      // ユーザーがダイアログを閉じた場合など
      if (/AbortError|NotAllowedError|取消|canceled|cancelled|timed out|not allowed/i.test(msg)) {
        setError(
          "パスキー登録がキャンセルされたか、時間切れです。もう一度押し、表示された Touch ID / パスキー確認をすぐに承認してください。"
        );
      } else {
        setError(msg);
      }
    } finally {
      setRegistering(false);
    }
  };

  if (loading) {
    return (
      <div className="p-6">
        <div className="animate-pulse space-y-4">
          <div className="h-8 bg-muted rounded w-1/3" />
          <div className="h-40 bg-muted rounded" />
        </div>
      </div>
    );
  }

  if (!data) {
    return (
      <div className="p-6">
        <p className="text-red-400">セキュリティ情報を取得できませんでした</p>
      </div>
    );
  }

  return (
    <div className="p-6 space-y-6">
      {data.mfa_setup_required && (
        <div className="flex items-start gap-3 rounded-lg border border-amber-500/40 bg-amber-500/10 p-4 text-amber-200">
          <AlertCircle className="w-5 h-5 mt-0.5 shrink-0" />
          <div className="text-sm space-y-1">
            <p className="font-semibold">多要素認証（MFA）の登録が必須です</p>
            <p className="text-amber-100/80">
              下の「パスキーを登録」を押し、MacのTouch ID / Windows Hello / セキュリティキーなどで登録してください。
              完了するまで他の画面は利用できません。
            </p>
          </div>
        </div>
      )}

      {message && (
        <div className="rounded-lg border border-emerald-500/40 bg-emerald-500/10 p-3 text-sm text-emerald-200">
          {message}
        </div>
      )}
      {error && (
        <div className="rounded-lg border border-red-500/40 bg-red-500/10 p-3 text-sm text-red-300">
          {error}
        </div>
      )}

      {/* ユーザー情報 */}
      <div className="bg-card border border-border rounded-lg p-5">
        <h2 className="text-sm text-muted-foreground mb-3">アカウント情報</h2>
        <div className="grid grid-cols-3 gap-4">
          <div>
            <p className="text-xs text-muted-foreground">ユーザー名</p>
            <p className="font-medium">{data.user.username}</p>
          </div>
          <div>
            <p className="text-xs text-muted-foreground">メールアドレス</p>
            <p className="font-medium">{data.user.email}</p>
          </div>
          <div>
            <p className="text-xs text-muted-foreground">MFA</p>
            <p className="font-medium">
              {data.user.mfa_enabled ? (
                <span className="text-emerald-400">有効</span>
              ) : (
                <span className="text-muted-foreground">無効</span>
              )}
            </p>
          </div>
        </div>
      </div>

      {/* パスキー（先に表示：現状の必須経路） */}
      <div className="bg-card border border-border rounded-lg p-5">
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            <Key className="w-4 h-4 text-amber-400" />
            <h2 className="text-sm text-muted-foreground">パスキー</h2>
          </div>
          <button
            type="button"
            onClick={handleRegisterPasskey}
            disabled={registering}
            className="px-4 py-2 bg-amber-600 hover:bg-amber-500 disabled:opacity-50 rounded-md text-sm transition-colors"
          >
            {registering ? "登録中..." : "パスキーを登録"}
          </button>
        </div>

        {data.passkeys.length === 0 ? (
          <div className="flex items-center gap-2 text-muted-foreground py-4">
            <AlertCircle className="w-4 h-4" />
            <p className="text-sm">パスキーが登録されていません。「パスキーを登録」を押してください。</p>
          </div>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="text-muted-foreground border-b border-border">
                <th className="text-left py-2 font-medium">名前</th>
                <th className="text-left py-2 font-medium">登録日</th>
              </tr>
            </thead>
            <tbody>
              {data.passkeys.map((pk) => (
                <tr key={pk.id} className="border-b border-border/50">
                  <td className="py-2">{pk.name}</td>
                  <td className="py-2 text-muted-foreground">{pk.created_at}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      {/* TOTP */}
      <div className="bg-card border border-border rounded-lg p-5">
        <div className="flex items-center gap-2 mb-3">
          <Smartphone className="w-4 h-4 text-blue-400" />
          <h2 className="text-sm text-muted-foreground">ワンタイムパスワード（TOTP）</h2>
        </div>
        <div className="flex items-center justify-between">
          <div>
            <p className="font-medium">
              {data.totp_active ? (
                <span className="text-emerald-400">設定済み</span>
              ) : (
                <span className="text-muted-foreground">未設定</span>
              )}
            </p>
            <p className="text-xs text-muted-foreground mt-1">
              現状はパスキー登録を推奨（TOTPセットアップは未整備の場合があります）
            </p>
          </div>
          <button
            type="button"
            onClick={() => router.push("/settings/security/totp/setup")}
            className="px-4 py-2 bg-blue-600 hover:bg-blue-700 rounded-md text-sm transition-colors"
          >
            {data.totp_active ? "再設定" : "設定する"}
          </button>
        </div>
      </div>
    </div>
  );
}
