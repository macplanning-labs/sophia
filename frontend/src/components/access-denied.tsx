// components/access-denied.tsx — 権限不足時の画面
//
// Admin専用ページに一般社員が直接URLアクセスした場合などに表示する。
// 従来はAPIが403を返し fetchJson が401/403を同一視してログイン画面へ強制リダイレクトしていたため、
// 有効なセッションを持つユーザーから見ると「勝手にログアウトされた」ように見えてしまっていた。
// このコンポーネントはページ側で useCurrentUser().isAdmin を先に判定して表示することで、
// そもそもAPIを呼ばずに、意味の分かるメッセージを出す。

import { ShieldAlert } from "lucide-react";
import Link from "next/link";

export function AccessDenied({
  message = "この画面を表示するには管理者権限が必要です。",
}: {
  message?: string;
}) {
  return (
    <div className="flex flex-col items-center justify-center h-[60vh] gap-3 text-center px-6">
      <ShieldAlert className="w-10 h-10 text-amber-400" />
      <h1 className="text-lg font-bold text-foreground">アクセス権限がありません</h1>
      <p className="text-sm text-muted-foreground max-w-sm">{message}</p>
      <Link
        href="/"
        className="mt-2 px-4 py-2 text-xs font-medium rounded-md border border-border text-foreground hover:bg-accent transition-colors"
      >
        ダッシュボードに戻る
      </Link>
    </div>
  );
}
