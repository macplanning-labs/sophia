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
  | "Globe"
  | "Building2"
  | "Receipt"
  | "Send"
  | "FileInput"
  | "FileOutput"
  | "Inbox"
  | "Wallet"
  | "Coins"
  | "UserCog";

export interface PageTitleMeta {
  /** pathname がこのプレフィックスで始まる（"/" は完全一致） */
  prefix: string;
  title: string;
  subtitle?: string;
  /** Lucide アイコン名（PageHeader 由来の画面） */
  lucideIcon?: LucideIconName;
  /** アイコン色（Tailwind クラス） */
  iconColor?: string;
}

/** より具体的なパスを先に置く */
const RULES: PageTitleMeta[] = [
  // 一番上の帯にタイトルが出ていなかった画面(本文に二重に出していた)。DEMO-000197
  {
    prefix: "/home",
    title: "ホーム",
    subtitle: "いま手当てが必要なものを、種類ごとに数えて並べる。カードを選ぶと、下に一覧が出る",
    lucideIcon: "Gauge",
    iconColor: "text-sky-400",
  },
  {
    prefix: "/status",
    title: "月別状況",
    subtitle: "アサイン(要員×案件)ごとに、受注書から支払通知までの進み具合を見る",
    lucideIcon: "CalendarCheck",
    iconColor: "text-sky-400",
  },
  {
    prefix: "/my-timesheet",
    title: "稼働報告（自己申告）",
    subtitle: "受注契約が無い社員向けの月次稼働報告。見慣れた勤務表と同じ列構成で入力できる",
    lucideIcon: "Clock",
    iconColor: "text-sky-400",
  },
  {
    prefix: "/tasks",
    title: "タスク",
    subtitle: "月次タスクの進捗管理",
    lucideIcon: "FileCheck",
    iconColor: "text-teal-400",
  },
  {
    prefix: "/notices",
    title: "支払通知",
    subtitle: "パートナーへの支払通知書",
    lucideIcon: "ReceiptText",
    iconColor: "text-amber-400",
  },
  {
    prefix: "/received-orders",
    title: "受注書",
    subtitle: "取引先から受けた注文書(受注)",
    lucideIcon: "FileText",
    iconColor: "text-emerald-400",
  },
  {
    prefix: "/orders",
    title: "発注書",
    subtitle: "パートナーへの注文書(発注)",
    lucideIcon: "FileText",
    iconColor: "text-violet-400",
  },
  {
    prefix: "/settings/integrations",
    title: "外部連携",
    subtitle: "通知の送り先、書類の保存先、メールなど、Sophia が外とつながる先を確かめて、変えられます",
    lucideIcon: "Globe",
    iconColor: "text-cyan-400",
  },
  {
    prefix: "/settings/api-keys",
    title: "APIキー管理",
    subtitle: "取引先向け WebAPI キーの発行・失効",
    lucideIcon: "KeyRound",
    iconColor: "text-amber-400",
  },
  {
    prefix: "/settings/org",
    title: "組織・ロール",
    subtitle: "組織の木・ロールと権限・ユーザーへの割り当てと変更履歴",
    lucideIcon: "Building2",
    iconColor: "text-indigo-400",
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
    lucideIcon: "FileOutput",
    iconColor: "text-blue-400",
  },
  {
    prefix: "/client-contracts",
    title: "受注契約",
    subtitle: "行をクリックして契約を編集",
    lucideIcon: "FileInput",
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
    prefix: "/sent-emails",
    title: "送信メール",
    subtitle: "Sophia が送ったメールの記録(新しい順)",
    lucideIcon: "Send",
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
    prefix: "/timesheet-matching",
    title: "勤務表の取り込み",
    subtitle: "届いた勤務表を受注に結び付けて、稼働報告に取り込む",
    lucideIcon: "Inbox",
    iconColor: "text-sky-400",
  },
  {
    prefix: "/timesheets",
    title: "稼働報告",
    subtitle: "エンジニアの稼働時間報告一覧",
    lucideIcon: "Clock",
    iconColor: "text-amber-400",
  },
  {
    prefix: "/billing",
    title: "請求書の確定",
    subtitle: "請求書の確認と確定",
    lucideIcon: "Receipt",
    iconColor: "text-blue-400",
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
    subtitle: "行をクリックして案件詳細を開く。新規作成はウィザードから",
    lucideIcon: "Briefcase",
    iconColor: "text-emerald-400",
  },
  {
    prefix: "/payroll",
    title: "給与",
    subtitle: "月次給与計算結果一覧",
    lucideIcon: "Wallet",
    iconColor: "text-yellow-400",
  },
  {
    prefix: "/employees",
    title: "社員一覧",
    subtitle: "社員マスタ一覧",
    lucideIcon: "UserCircle",
    iconColor: "text-pink-400",
  },
  {
    prefix: "/members",
    title: "要員",
    subtitle: "自社社員とパートナー要員の管理",
    lucideIcon: "Users",
    iconColor: "text-blue-400",
  },
  {
    prefix: "/expenses",
    title: "経費",
    subtitle: "経費精算申請一覧",
    lucideIcon: "Coins",
    iconColor: "text-rose-400",
  },
  {
    prefix: "/users",
    title: "ユーザー管理",
    subtitle: "システムユーザー一覧",
    lucideIcon: "UserCog",
    iconColor: "text-indigo-400",
  },
  {
    prefix: "/parties",
    title: "取引先",
    subtitle: "クライアント・パートナーの管理",
    lucideIcon: "Building2",
    iconColor: "text-blue-400",
  },
  {
    prefix: "/assignments",
    title: "アサイン",
    subtitle: "要員を案件へ編成し、利益を確認しながらアサインを作成",
    lucideIcon: "Briefcase",
    iconColor: "text-teal-400",
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
