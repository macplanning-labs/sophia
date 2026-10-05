"use client";

/**
 * 稼働報告アップロード共通フロー（admin / portal 共用）
 *
 * - ステップ①: uploadUrl へ POST → プレビュー
 * - ステップ②: confirmUrl へ POST → DB 登録
 *
 * ページ固有 UI（招待・日次タブ・一覧）は呼び出し側に残す。
 */

import { useCallback, useRef, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { apiUpload } from "@/lib/api";
import type { TimesheetPreviewData } from "@/components/TimesheetPreview";

const ACCEPTED_EXTS = ["xlsx", "xlsm", "pdf"] as const;
const MAX_BYTES = 50 * 1024 * 1024;

export type UseTimesheetUploadFlowOptions = {
  uploadUrl: string;
  confirmUrl: string;
  /** 登録成功時に invalidate する queryKey 一覧 */
  invalidateKeys?: (readonly unknown[])[];
  /** true: 解析成功時に toast（portal 向け） */
  toastOnParseSuccess?: boolean;
  /** true: 拡張子・サイズをクライアント側で検証（portal 向け） */
  validateFile?: boolean;
  /** true: 解析エラーを uploadError state に載せる（admin 向け） */
  trackUploadError?: boolean;
  /** 確定登録成功後の追加処理（例: アップロードパネルを閉じる） */
  onConfirmSuccess?: () => void;
};

export function useTimesheetUploadFlow(options: UseTimesheetUploadFlowOptions) {
  const {
    uploadUrl,
    confirmUrl,
    invalidateKeys = [],
    toastOnParseSuccess = false,
    validateFile = false,
    trackUploadError = false,
    onConfirmSuccess,
  } = options;

  const queryClient = useQueryClient();
  const fileInputRef = useRef<HTMLInputElement>(null);

  const [preview, setPreview] = useState<TimesheetPreviewData | null>(null);
  const [uploadFile, setUploadFile] = useState<File | null>(null);
  const [excelBuffer, setExcelBuffer] = useState<ArrayBuffer | null>(null);
  const [pdfUrl, setPdfUrl] = useState<string | null>(null);
  const [dragOver, setDragOver] = useState(false);
  const [uploadError, setUploadError] = useState<string | null>(null);

  const clearPreviewState = useCallback(() => {
    setPreview(null);
    setUploadFile(null);
    setExcelBuffer(null);
    setUploadError(null);
    setPdfUrl((prev) => {
      if (prev) URL.revokeObjectURL(prev);
      return null;
    });
  }, []);

  const uploadMutation = useMutation({
    mutationFn: async (file: File) => {
      const fd = new FormData();
      fd.append("file", file);
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      return apiUpload<{ status: string; preview?: any; error?: string }>(uploadUrl, fd);
    },
    onSuccess: (data) => {
      if (data.status === "ok") {
        setPreview(data.preview);
        if (trackUploadError) setUploadError(null);
        if (toastOnParseSuccess) {
          toast.success("ファイルを解析しました。内容を確認してください。");
        }
      } else {
        const msg = data.error || "解析エラー";
        setPreview(null);
        if (trackUploadError) setUploadError(msg);
        else toast.error(msg);
      }
    },
    onError: (err: Error) => {
      const msg = err.message || "アップロードに失敗しました";
      if (trackUploadError) setUploadError(msg);
      else toast.error(msg);
    },
  });

  const confirmMutation = useMutation({
    mutationFn: async (file: File) => {
      const fd = new FormData();
      fd.append("file", file);
      let data: { status: string; error?: string };
      try {
        data = await apiUpload<{ status: string; error?: string }>(confirmUrl, fd);
      } catch (err) {
        // サーバーがJSON以外（HTMLエラーページ等）を返した場合、res.json()のSyntaxErrorを
        // そのまま見せず分かりやすいメッセージにする。SessionExpiredError等はそのまま再送出する。
        if (err instanceof SyntaxError) {
          throw new Error("登録に失敗しました。サーバー応答が不正です。");
        }
        throw err;
      }
      if (data.status !== "ok") {
        throw new Error(data.error || "登録に失敗しました");
      }
      return data;
    },
    onSuccess: () => {
      toast.success("稼働報告を登録しました");
      for (const key of invalidateKeys) {
        queryClient.invalidateQueries({ queryKey: [...key] });
      }
      clearPreviewState();
      onConfirmSuccess?.();
    },
    onError: (err: Error) => {
      const msg = err.message || "登録に失敗しました";
      if (trackUploadError) setUploadError(msg);
      toast.error(msg);
    },
  });

  const processFile = useCallback(
    async (file: File) => {
      if (validateFile) {
        const ext = file.name.split(".").pop()?.toLowerCase();
        if (!ext || !(ACCEPTED_EXTS as readonly string[]).includes(ext)) {
          toast.error("対応ファイル形式: .xlsx / .xlsm / .pdf");
          return;
        }
        if (file.size > MAX_BYTES) {
          toast.error("ファイルサイズが50MBを超えています");
          return;
        }
      }

      setUploadFile(file);
      setPreview(null);
      if (trackUploadError) setUploadError(null);

      const isPdf =
        file.name.toLowerCase().endsWith(".pdf") || file.type === "application/pdf";
      if (isPdf) {
        setExcelBuffer(null);
        setPdfUrl((prev) => {
          if (prev) URL.revokeObjectURL(prev);
          return URL.createObjectURL(file);
        });
      } else {
        setPdfUrl((prev) => {
          if (prev) URL.revokeObjectURL(prev);
          return null;
        });
        const buf = await file.arrayBuffer();
        setExcelBuffer(buf);
      }

      uploadMutation.mutate(file);
    },
    [uploadMutation, validateFile, trackUploadError],
  );

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setDragOver(false);
      const file = e.dataTransfer.files?.[0];
      if (file) processFile(file);
    },
    [processFile],
  );

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setDragOver(true);
  }, []);

  const handleDragLeave = useCallback(() => setDragOver(false), []);

  const handleFileChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (file) processFile(file);
    },
    [processFile],
  );

  const handleConfirm = useCallback(() => {
    if (!uploadFile) return;
    confirmMutation.mutate(uploadFile);
  }, [uploadFile, confirmMutation]);

  const handleCancel = useCallback(() => {
    clearPreviewState();
  }, [clearPreviewState]);

  return {
    fileInputRef,
    preview,
    uploadFile,
    excelBuffer,
    pdfUrl,
    dragOver,
    uploadError,
    setUploadError,
    isParsing: uploadMutation.isPending,
    isConfirming: confirmMutation.isPending,
    processFile,
    handleDrop,
    handleDragOver,
    handleDragLeave,
    handleFileChange,
    handleConfirm,
    handleCancel,
    clearPreviewState,
  };
}
