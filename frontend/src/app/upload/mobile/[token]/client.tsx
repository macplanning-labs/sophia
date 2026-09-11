"use client";

import { useRef, useState } from "react";
import { uploadMobileReceipt } from "@/lib/api";
import { useDynamicId } from "@/lib/utils";

type UploadState = "idle" | "uploading" | "done" | "error";

export default function MobileUploadPage() {
  const token = useDynamicId();
  const [preview, setPreview] = useState<string | null>(null);
  const [previewIsPdf, setPreviewIsPdf] = useState(false);
  const [state, setState] = useState<UploadState>("idle");
  const [error, setError] = useState<string>("");
  const inputRef = useRef<HTMLInputElement>(null);

  const handleFileChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    const isPdf = file.type === "application/pdf" || /\.pdf$/i.test(file.name);
    setPreviewIsPdf(isPdf);
    setPreview(isPdf ? null : URL.createObjectURL(file));
    setState("uploading");
    setError("");

    try {
      const res = await uploadMobileReceipt(token, file);
      if (res.success) {
        setState("done");
      } else {
        setState("error");
        setError(res.error || "アップロードに失敗しました");
      }
    } catch {
      setState("error");
      setError("通信に失敗しました。電波の良い場所で再度お試しください");
    }
  };

  if (state === "done") {
    return (
      <div className="min-h-screen bg-background flex items-center justify-center p-6">
        <div className="text-center space-y-4">
          <div className="text-5xl">✅</div>
          <h1 className="text-lg font-bold text-foreground">アップロード完了</h1>
          <p className="text-sm text-muted-foreground">PC画面に反映されました。このページは閉じて大丈夫です。</p>
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-background flex flex-col items-center justify-center p-6">
      <div className="w-full max-w-sm space-y-6 text-center">
        <div>
          <h1 className="text-lg font-bold text-foreground">領収書をアップロード</h1>
          <p className="text-xs text-muted-foreground mt-1">カメラで撮影するか、JPEG/PNG/PDFを選択してください</p>
        </div>

        {preview && !previewIsPdf && (
          <img src={preview} alt="プレビュー" className="w-full rounded-lg border border-border object-contain max-h-72" />
        )}
        {previewIsPdf && state !== "idle" && (
          <div className="rounded-lg border border-border bg-muted py-8 text-sm font-medium text-rose-300">PDF</div>
        )}

        {state === "error" && (
          <p className="text-sm text-red-400 bg-red-500/10 border border-red-500/30 rounded-lg p-3">{error}</p>
        )}

        <input
          ref={inputRef}
          type="file"
          accept="image/jpeg,image/png,image/*,application/pdf,.pdf"
          capture="environment"
          onChange={handleFileChange}
          className="hidden"
        />

        <button
          onClick={() => inputRef.current?.click()}
          disabled={state === "uploading"}
          className="w-full py-3 bg-rose-600 hover:bg-rose-500 disabled:opacity-50 text-white text-base font-medium rounded-lg transition-colors"
        >
          {state === "uploading" ? "アップロード中..." : preview || previewIsPdf ? "選び直す" : "📷 撮影 / ファイル選択"}
        </button>

        <p className="text-[11px] text-muted-foreground">
          JPEG/PNG/PDF、10MBまで。このリンクは10分間・1回限り有効です。
        </p>
      </div>
    </div>
  );
}
