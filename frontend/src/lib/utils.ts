import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"
import { usePathname } from "next/navigation"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

/**
 * 動的ルート([id]配下)の実際のIDをURLパスから取得する。
 *
 * このアプリはNext.js静的エクスポート + サーバー側フォールバック
 * (`/xxx/123` → `/xxx/_/index.html`)で動的ルートを実現しているため、
 * `generateStaticParams`はプレースホルダー`"_"`のみを返す。
 * そのため`useParams()`はビルド時の`"_"`を返してしまい、実際のIDは
 * 取得できない。実IDは常にブラウザの実パスから読み取る必要がある。
 */
export function useDynamicId(): string {
  const pathname = usePathname()
  const segments = pathname.split("/").filter(Boolean)
  return decodeURIComponent(segments[segments.length - 1] ?? "")
}
