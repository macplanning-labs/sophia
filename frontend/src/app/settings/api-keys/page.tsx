"use client";

import { useCallback, useEffect, useState } from "react";
import { Key } from "lucide-react";
import { fetchApiKeys, generateApiKey, revokeApiKey, fetchMastersData, type ApiKeyItem, type GeneratedApiKey } from "@/lib/api";
import { FormField, FormSelect } from "@/components/ui/form-modal";

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
  const [error, setError] = useState<string | null>(null);

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
    if (!confirm("Are you sure you want to revoke this key?")) return;

    setLoading(true);
    try {
      await revokeApiKey(id);
      await loadKeys();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Error revoking key");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="p-8 max-w-4xl">
      <div className="flex items-center gap-3 mb-8">
        <Key className="w-8 h-8" />
        <h1 className="text-3xl font-bold">APIキー管理</h1>
      </div>

      {error && (
        <div className="mb-4 p-4 bg-red-50 text-red-700 rounded-lg">
          {error}
        </div>
      )}

      <div className="mb-8 p-4 bg-gray-50 rounded-lg">
        <div className="mb-4">
          <FormField label="クライアント" required>
            <FormSelect
              options={clientOptions.map((c) => ({ value: String(c.value), label: c.label }))}
              placeholder="選択..."
              value={clientId}
              onChange={(e) => setClientId(e.target.value)}
              disabled={loadingClients}
            />
          </FormField>
        </div>

        <div className="grid grid-cols-2 gap-4 mb-4">
          <div>
            <label className="block text-sm font-medium mb-2">キー名</label>
            <input
              type="text"
              value={keyName}
              onChange={(e) => setKeyName(e.target.value)}
              className="w-full px-3 py-2 border rounded-lg"
              placeholder="本番API連携"
            />
          </div>
          <div>
            <label className="block text-sm font-medium mb-2">スコープ</label>
            <select
              value={keyScope}
              onChange={(e) => setKeyScope(e.target.value)}
              className="w-full px-3 py-2 border rounded-lg"
            >
              <option value="READ">READ</option>
              <option value="READ_WRITE">READ_WRITE</option>
            </select>
          </div>
        </div>

        <div className="mb-4">
          <label className="block text-sm font-medium mb-2">環境</label>
          <select
            value={keyEnv}
            onChange={(e) => setKeyEnv(e.target.value)}
            className="w-full px-3 py-2 border rounded-lg"
          >
            <option value="live">本番 (sk_live_)</option>
            <option value="test">テスト (sk_test_)</option>
          </select>
        </div>

        <button
          onClick={handleGenerateKey}
          disabled={loading || !clientId}
          data-testid="generate-api-key-btn"
          className="w-full px-4 py-2 bg-blue-500 text-white rounded-lg hover:bg-blue-600 disabled:opacity-50"
        >
          {loading ? "生成中..." : "APIキーを生成"}
        </button>
      </div>

      {generatedKey && (
        <div className="mb-8 p-4 bg-green-50 border-2 border-green-200 rounded-lg">
          <p className="text-sm text-gray-600 mb-2">
            生成されたAPIキー（今回のみ表示）：
          </p>
          <div
            data-testid="generated-api-key-value"
            className="mb-4 p-3 bg-green-100 font-mono text-sm break-all"
          >
            {generatedKey.api_key}
          </div>
          <button
            onClick={() => {
              navigator.clipboard.writeText(generatedKey.api_key);
            }}
            className="px-4 py-2 bg-green-600 text-white rounded-lg hover:bg-green-700"
          >
            コピー
          </button>
        </div>
      )}

      <div>
        <h2 className="text-xl font-bold mb-4">アクティブなキー</h2>
        {keys.length === 0 ? (
          <p className="text-gray-500">キーがありません</p>
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
                  className="flex justify-between items-center p-4 border rounded-lg"
                >
                  <div>
                    <p className="font-medium">{key.name}</p>
                    <p className="text-sm text-gray-500">
                      {key.key_prefix}... | {key.scope}
                    </p>
                  </div>
                  <button
                    onClick={() => handleRevokeKey(key.id)}
                    disabled={loading}
                    data-testid="revoke-api-key-btn"
                    className="px-4 py-2 bg-red-500 text-white rounded-lg hover:bg-red-600 disabled:opacity-50"
                  >
                    失効
                  </button>
                </div>
              ))}
          </div>
        )}

        {keys.some((k) => !k.is_active) && (
          <>
            <h2 className="text-xl font-bold mt-8 mb-4">失効済みキー</h2>
            <div className="space-y-2">
              {keys
                .filter((k) => !k.is_active)
                .map((key) => (
                  <div
                    key={key.id}
                    className="flex justify-between items-center p-4 border border-gray-300 rounded-lg opacity-60"
                  >
                    <div>
                      <p className="font-medium line-through">{key.name}</p>
                      <p className="text-sm text-gray-500">
                        {key.key_prefix}... | {key.scope}
                      </p>
                    </div>
                  </div>
                ))}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
