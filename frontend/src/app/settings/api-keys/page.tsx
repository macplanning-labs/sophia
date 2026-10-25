"use client";

import { useCallback, useEffect, useState } from "react";
import { formatExpiry } from "@/lib/api-key-expiry";
import { Key } from "lucide-react";
import { fetchApiKeys, generateApiKey, revokeApiKey, fetchMastersData, type ApiKeyItem, type GeneratedApiKey } from "@/lib/api";
import { FormField, FormInput, FormSelect } from "@/components/ui/form-modal";
import { ConfirmSheet } from "@/components/ui/confirm-sheet";
import { PageHeader } from "@/components/ui/page-header";

export default function ApiKeysPage() {
  const [clientId, setClientId] = useState<string>("");
  const [clientOptions, setClientOptions] = useState<{ value: number; label: string }[]>([]);
  const [keys, setKeys] = useState<ApiKeyItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [loadingClients, setLoadingClients] = useState(false);
  const [generatedKey, setGeneratedKey] = useState<GeneratedApiKey | null>(null);
  const [keyName, setKeyName] = useState("");
  const [keyScope, setKeyScope] = useState("READ");
  const [keyEnv, setKeyEnv] = useState("test");
  // 有効期限(日数)。空文字=無期限
  const [keyExpiryDays, setKeyExpiryDays] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [revokeKeyId, setRevokeKeyId] = useState<string | null>(null);

  useEffect(() => {
    const loadClients = async () => {
      setLoadingClients(true);
      setError(null);
      try {
        const data = await fetchMastersData("clients");
        const options = data.map((client: { id: number; name: string } | { value: number; label: string }) => {
          const id = "id" in client ? client.id : client.value;
          const name = "name" in client ? client.name : client.label;
          return { value: id, label: name };
        });
        setClientOptions(options);
      } catch (e) {
        setError(e instanceof Error ? e.message : "クライアント一覧の読込に失敗しました");
      } finally {
        setLoadingClients(false);
      }
    };
    loadClients();
  }, []);

  const loadKeys = useCallback(async () => {
    if (!clientId) return;
    setLoading(true);
    try {
      const data = await fetchApiKeys(Number(clientId));
      setKeys(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Error loading keys");
    } finally {
      setLoading(false);
    }
  }, [clientId]);

  useEffect(() => {
    loadKeys();
  }, [loadKeys]);

  const handleGenerateKey = async () => {
    if (!clientId || !keyName) {
      setError("Client ID and key name are required");
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const result = await generateApiKey({
        client_id: Number(clientId),
        name: keyName,
        scope: keyScope,
        environment: keyEnv,
        ...(keyExpiryDays ? { expires_in_days: Number(keyExpiryDays) } : {}),
      });
      setGeneratedKey(result);
      setKeyName("");
      await loadKeys();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Error generating key");
    } finally {
      setLoading(false);
    }
  };

  const handleRevokeKey = async (id: string) => {
    setRevokeKeyId(id);
  };

  const doRevokeKey = async () => {
    if (!revokeKeyId) return;
    const id = revokeKeyId;
    setRevokeKeyId(null);
    setLoading(true);
    try {
      await revokeApiKey(id);
      // 今表示中の生成キーを失効した場合は、「今回のみ表示」の欄も消す
      setGeneratedKey((prev) => (prev && prev.id === id ? null : prev));
      await loadKeys();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Error revoking key");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="p-6 space-y-6 max-w-4xl">
      <PageHeader />

      {error && (
        <div className="rounded-lg border border-red-500/40 bg-red-500/10 p-3 text-sm text-red-300">
          {error}
        </div>
      )}

      {/* 発行 */}
      <div className="bg-card border border-border rounded-lg p-5 space-y-4">
        <div className="flex items-center gap-2">
          <Key className="w-4 h-4 text-amber-400" />
          <h2 className="text-sm text-muted-foreground">APIキーの発行</h2>
        </div>

        <FormField label="クライアント" required>
          <FormSelect
            options={clientOptions.map((c) => ({ value: String(c.value), label: c.label }))}
            placeholder="選択..."
            value={clientId}
            onChange={(e) => setClientId(e.target.value)}
            disabled={loadingClients}
          />
        </FormField>

        <div className="grid grid-cols-2 gap-4">
          <FormField label="キー名">
            <FormInput
              type="text"
              value={keyName}
              onChange={(e) => setKeyName(e.target.value)}
              placeholder="本番API連携"
            />
          </FormField>
          <FormField label="スコープ">
            <FormSelect
              options={[
                { value: "READ", label: "READ" },
                { value: "READ_WRITE", label: "READ_WRITE" },
              ]}
              value={keyScope}
              onChange={(e) => setKeyScope(e.target.value)}
            />
          </FormField>
        </div>

        <FormField label="環境">
          <FormSelect
            options={[
              { value: "live", label: "本番 (sk_live_)" },
              { value: "test", label: "テスト (sk_test_)" },
            ]}
            value={keyEnv}
            onChange={(e) => setKeyEnv(e.target.value)}
          />
        </FormField>

        <FormField label="有効期限">
          <FormSelect
            options={[
              { value: "90", label: "90日" },
              { value: "180", label: "180日" },
              { value: "365", label: "365日" },
            ]}
            placeholder="無期限"
            value={keyExpiryDays}
            onChange={(e) => setKeyExpiryDays(e.target.value)}
            data-testid="api-key-expiry-select"
          />
          <p className="text-xs text-muted-foreground">
            期限を過ぎたキーは使えなくなります。期限の前に、新しいキーを発行して取引先に渡してください。
          </p>
        </FormField>

        <button
          type="button"
          onClick={handleGenerateKey}
          disabled={loading || !clientId}
          data-testid="generate-api-key-btn"
          className="w-full px-4 py-2 bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50 rounded-md text-sm transition-colors"
        >
          {loading ? "生成中..." : "APIキーを生成"}
        </button>
      </div>

      {generatedKey && (
        <div className="rounded-lg border border-emerald-500/40 bg-emerald-500/10 p-5 space-y-3">
          <p className="text-sm text-emerald-200">生成されたAPIキー（今回のみ表示）：</p>
          <div
            data-testid="generated-api-key-value"
            className="p-3 bg-muted border border-border rounded-md font-mono text-sm text-foreground break-all"
          >
            {generatedKey.api_key}
          </div>
          <button
            type="button"
            onClick={() => {
              navigator.clipboard.writeText(generatedKey.api_key);
            }}
            className="px-4 py-2 bg-emerald-600 hover:bg-emerald-500 rounded-md text-sm text-white transition-colors"
          >
            コピー
          </button>
        </div>
      )}

      {/* 一覧 */}
      <div className="bg-card border border-border rounded-lg p-5">
        <h2 className="text-sm text-muted-foreground mb-3">アクティブなキー</h2>
        {keys.length === 0 ? (
          <p className="text-sm text-muted-foreground">キーがありません</p>
        ) : (
          <div
            data-testid="api-key-list"
            className="space-y-2"
          >
            {keys
              .filter((k) => k.is_active)
              .map((key) => (
                <div
                  key={key.id}
                  className="flex justify-between items-center p-4 border border-border rounded-lg"
                >
                  <div>
                    <p className="font-medium">{key.name}</p>
                    <p className="text-sm text-muted-foreground">
                      {key.key_prefix}... | {key.scope} | {formatExpiry(key.expires_at)}
                    </p>
                  </div>
                  <button
                    type="button"
                    onClick={() => handleRevokeKey(key.id)}
                    disabled={loading}
                    data-testid="revoke-api-key-btn"
                    className="px-4 py-2 bg-red-600 hover:bg-red-500 disabled:opacity-50 rounded-md text-sm text-white transition-colors"
                  >
                    失効
                  </button>
                </div>
              ))}
          </div>
        )}

        {keys.some((k) => !k.is_active) && (
          <>
            <h2 className="text-sm text-muted-foreground mt-6 mb-3">失効済みキー</h2>
            <div className="space-y-2">
              {keys
                .filter((k) => !k.is_active)
                .map((key) => (
                  <div
                    key={key.id}
                    className="flex justify-between items-center p-4 border border-border rounded-lg opacity-60"
                  >
                    <div>
                      <p className="font-medium line-through">{key.name}</p>
                      <p className="text-sm text-muted-foreground">
                        {key.key_prefix}... | {key.scope}
                      </p>
                    </div>
                  </div>
                ))}
            </div>
          </>
        )}
      </div>

      <ConfirmSheet
        open={revokeKeyId !== null}
        title="APIキーを失効"
        facts={revokeKeyId && keys.find((k) => k.id === revokeKeyId) ? [
          { label: "キー名", value: keys.find((k) => k.id === revokeKeyId)?.name },
        ] : undefined}
        description="この操作は取り消せません。"
        variant="danger"
        confirmLabel="失効"
        loading={loading}
        onConfirm={doRevokeKey}
        onCancel={() => setRevokeKeyId(null)}
      />
    </div>
  );
}
