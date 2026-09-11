/** AppHeader 左に表示するルート連動タイトル・アイコン・説明 */

export type LucideIconName =
  | "Gauge"
  | "CalendarCheck"
  | "Mail"
  | "Database"
  | "ShieldCheck"
  | "KeyRound"
  | "Briefcase"
  | "FileText"
  | "FileCheck"
  | "Clock"
  | "Calculator"
  | "UserCircle"
  | "ReceiptText"
  | "Users"
  | "Globe";

export interface PageTitleMeta {
  /** pathname がこのプレフィックスで始まる（"/" は完全一致） */
  prefix: string;
  title: string;
  subtitle?: string;
  /** Lucide アイコン名（PageHeader 由来の画面） */
  lucideIcon?: LucideIconName;
  /** 絵文字アイコン（旧 h1 由来の画面） */
  emoji?: string;
  /** アイコン色（Tailwind クラス） */
  iconColor?: string;
}

/** より具体的なパスを先に置く */
const RULES: PageTitleMeta[] = [
  {
    prefix: "/settings/api-keys",
    title: "APIキー管理",
    subtitle: "取引先向け WebAPI キーの発行・失効",
    lucideIcon: "KeyRound",
    iconColor: "text-amber-400",
  },
  {
    prefix: "/settings/security/totp",
    title: "TOTP設定",
    subtitle: "ワンタイムパスワード（認証アプリ）の登録",
    lucideIcon: "KeyRound",
    iconColor: "text-emerald-400",
  },
  {
    prefix: "/settings/security",
    title: "セキュリティ設定",
    subtitle: "パスキー・MFA などアカウントのセキュリティ",
    lucideIcon: "ShieldCheck",
    iconColor: "text-emerald-400",
  },
  {
    prefix: "/partner-contracts",
    title: "発注契約",
    subtitle: "行をクリックして契約を編集",
    emoji: "📄",
    iconColor: "text-blue-400",
  },
  {
    prefix: "/client-contracts",
    title: "受注契約",
    subtitle: "行をクリックして契約を編集",
    emoji: "📋",
    iconColor: "text-emerald-400",
  },
  {
    prefix: "/received-emails",
    title: "受信メール",
    subtitle: "メール自動取込パイプラインで自動処理できなかったメールを確認する",
    lucideIcon: "Mail",
    iconColor: "text-violet-400",
  },
  {
    prefix: "/peppol",
    title: "Peppolログ",
    subtitle: "JP PINT準拠のデジタルインボイス送受信履歴を確認する",
    lucideIcon: "Globe",
    iconColor: "text-indigo-400",
  },
  {
    prefix: "/timesheets",
    title: "稼働報告",
    subtitle: "エンジニアの稼働時間報告一覧",
    emoji: "⏱",
    iconColor: "text-amber-400",
  },
  {
    prefix: "/settlement",
    title: "月次売上・支払 確定",
    subtitle: "請求書発行 × 支払通知書発行 — 一元管理ダッシュボード",
    lucideIcon: "CalendarCheck",
    iconColor: "text-amber-400",
  },
  {
    prefix: "/projects",
    title: "案件",
    subtitle: "行をクリックして案件情報を編集。新規作成はウィザードから",
    emoji: "📁",
    iconColor: "text-emerald-400",
  },
  {
    prefix: "/payroll",
    title: "給与",
    subtitle: "月次給与計算結果一覧",
    emoji: "💰",
    iconColor: "text-yellow-400",
  },
  {
    prefix: "/employees",
    title: "社員一覧",
    subtitle: "社員マスタ一覧",
    emoji: "👤",
    iconColor: "text-pink-400",
  },
  {
    prefix: "/expenses",
    title: "経費",
    subtitle: "経費精算申請一覧",
    emoji: "🧾",
    iconColor: "text-rose-400",
  },
  {
    prefix: "/users",
    title: "ユーザー管理",
    subtitle: "システムユーザー一覧",
    emoji: "👥",
    iconColor: "text-indigo-400",
  },
  {
    prefix: "/masters",
    title: "マスタメンテナンス",
    subtitle: "各マスタデータの閲覧・編集を行います",
    lucideIcon: "Database",
    iconColor: "text-sky-400",
  },
  {
    prefix: "/",
    title: "パイプライン ダッシュボード",
    subtitle: "プロジェクトを展開し、進捗行をクリックして注文書を確認・作成",
    lucideIcon: "Gauge",
    iconColor: "text-sky-400",
  },
];

export function resolvePageMeta(pathname: string | null): PageTitleMeta | null {
  if (!pathname) return null;
  for (const rule of RULES) {
    if (rule.prefix === "/") {
      if (pathname === "/") return rule;
      continue;
    }
    if (pathname === rule.prefix || pathname.startsWith(`${rule.prefix}/`)) {
      return rule;
    }
  }
  return null;
}

/** @deprecated resolvePageMeta を使用 */
export function resolvePageTitle(pathname: string | null): string | null {
  return resolvePageMeta(pathname)?.title ?? null;
}
