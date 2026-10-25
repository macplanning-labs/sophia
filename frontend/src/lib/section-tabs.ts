/**
 * section-tabs.ts — サイドバーの項目を「セクション」にまとめ、上部のタブで切り替える仕組み
 * (DEMO-000184 設定、DEMO-000185 勤務表、サイドバー整理 DEMO-000188)
 *
 * サイドバーの似た項目を1つのボタンにまとめ、押すと SENN の設定画面のように、上部のタブで画面を切り替える。
 * 各タブの URL は従来のまま(ブックマーク・通知ベル・メールのリンク・コマンド検索・テストを壊さない)。
 * 判定はここの純粋関数に集約し、サイドバーとタブの帯の両方が同じものを使う。
 *
 * 見せるかどうかの判定は canSeeTab の1か所だけ。いまは「管理者かどうか」(adminOnly)で決める。
 * ロールと組織に基づく閲覧範囲(DEMO-000173)で、画面側に権限の一覧が渡るようになったら、ここを差し替える。
 */

export type SectionTab = {
  key: string;
  label: string;
  href: string;
  /** true の場合、管理者にだけ表示する */
  adminOnly?: boolean;
};

export type SectionKey = "home" | "mail" | "contracts" | "parties" | "timesheets" | "closing" | "staff" | "settings";

export type Section = {
  key: SectionKey;
  /** サイドバーのボタンの名前(タブの帯の aria-label にも使う) */
  label: string;
  tabs: readonly SectionTab[];
};

/** ホーム: 新しい「要対応」と、従来のダッシュボード(契約から一括作成・EDI取込・メールチェックはここにしかない) */
export const HOME_SECTION: Section = {
  key: "home",
  label: "ホーム",
  tabs: [
    { key: "todo", label: "要対応", href: "/home", adminOnly: true },
    { key: "dashboard", label: "ダッシュボード", href: "/" },
  ],
};

/** メール: 受信と、Sophia が送ったメールの記録(DEMO-000196)。設定は各画面の右上の ⚙ から */
export const MAIL_SECTION: Section = {
  key: "mail",
  label: "メール",
  tabs: [
    { key: "received", label: "受信", href: "/received-emails", adminOnly: true },
    { key: "sent", label: "送信", href: "/sent-emails", adminOnly: true },
  ],
};

/** 契約: アサイン(受注・発注契約を同時に作る)と、契約の個別の編集・延長 */
export const CONTRACTS_SECTION: Section = {
  key: "contracts",
  label: "契約",
  tabs: [
    { key: "assignments", label: "アサイン", href: "/assignments", adminOnly: true },
    { key: "client", label: "受注契約", href: "/client-contracts" },
    { key: "partner", label: "発注契約", href: "/partner-contracts" },
  ],
};

/** 要員(取引先は 設定 > 取引先 に移した。2026-10-09) */
export const PARTIES_SECTION: Section = {
  key: "parties",
  label: "要員",
  tabs: [
    { key: "members", label: "要員", href: "/members", adminOnly: true },
  ],
};

/** 勤務表: 新UIの「勤務表」と旧「勤務表の取り込み」は同じ画面(/timesheet-matching)だった */
export const TIMESHEETS_SECTION: Section = {
  key: "timesheets",
  label: "勤務表",
  tabs: [
    { key: "matching", label: "取り込み・承認", href: "/timesheet-matching", adminOnly: true },
    { key: "reports", label: "稼働報告", href: "/timesheets" },
    { key: "self", label: "自己申告", href: "/my-timesheet" },
  ],
};

/**
 * 月の締め: 月末の作業を1か所に集める。
 * 支払通知書の発行と請求書の一括発行は、月次確定(/settlement)にしかない。
 * /billing は支払通知をまだ扱わないので「請求書の確定」と呼ぶ。
 */
export const CLOSING_SECTION: Section = {
  key: "closing",
  label: "月の締め",
  tabs: [
    { key: "status", label: "月別状況", href: "/status", adminOnly: true },
    { key: "settlement", label: "月次確定", href: "/settlement" },
    { key: "billing", label: "請求書の確定", href: "/billing", adminOnly: true },
    { key: "invoices", label: "請求書一覧", href: "/invoices" },
    { key: "peppol", label: "Peppolログ", href: "/peppol", adminOnly: true },
  ],
};

/** 社員・給与: 一般社員が開いても、給与・経費はサーバ側で本人の分だけが返る */
export const STAFF_SECTION: Section = {
  key: "staff",
  label: "社員・給与",
  tabs: [
    { key: "payroll", label: "給与", href: "/payroll" },
    { key: "employees", label: "社員", href: "/employees" },
    { key: "expenses", label: "経費", href: "/expenses" },
  ],
};

/** 設定: 従来のサイドバーの並び(ユーザー管理 → APIキー → 組織・ロール → セキュリティ → マスタメンテ)を、使用頻度の順に整理した */
export const SETTINGS_SECTION: Section = {
  key: "settings",
  label: "設定",
  tabs: [
    { key: "users", label: "ユーザー管理", href: "/users", adminOnly: true },
    { key: "org", label: "組織・ロール", href: "/settings/org", adminOnly: true },
    { key: "security", label: "セキュリティ", href: "/settings/security" },
    { key: "api-keys", label: "APIキー", href: "/settings/api-keys", adminOnly: true },
    // 外部連携(通知の送り先・書類の保存先。DEMO-000215)
    { key: "integrations", label: "外部連携", href: "/settings/integrations", adminOnly: true },
    { key: "masters", label: "マスタメンテ", href: "/masters", adminOnly: true },
    // クライアント・パートナー(マスタメンテの「クライアント」「パートナー」の代わり。担当の登録・変更もここ)
    { key: "parties", label: "取引先", href: "/parties", adminOnly: true },
  ],
};

export const SECTIONS: readonly Section[] = [
  HOME_SECTION,
  MAIL_SECTION,
  CONTRACTS_SECTION,
  PARTIES_SECTION,
  TIMESHEETS_SECTION,
  CLOSING_SECTION,
  STAFF_SECTION,
  SETTINGS_SECTION,
];

/** そのユーザーにタブを見せるか(見せる・見せないの判定はここだけ) */
export function canSeeTab(tab: SectionTab, isAdmin: boolean): boolean {
  return !tab.adminOnly || isAdmin;
}

/** そのユーザーに見えるタブ。1つも無いセクションは、サイドバーに出さない */
export function visibleTabs(section: Section, isAdmin: boolean): SectionTab[] {
  return section.tabs.filter((t) => canSeeTab(t, isAdmin));
}

/** パスがタブの href 配下か(`/users` は `/users/12` にも一致、`/usersX` には一致しない。`/` は `/` だけ) */
export function matchesHref(pathname: string, href: string): boolean {
  return pathname === href || pathname.startsWith(`${href}/`);
}

/** セクションのどのタブにも属さないパスは undefined。権限での出し分けはしない(権限が無い人が開いた場合も、そのセクションの中として扱う) */
export function activeTab(section: Section, pathname: string): SectionTab | undefined {
  return section.tabs.find((t) => matchesHref(pathname, t.href));
}

/** パスが、そのセクションの画面か(サイドバーのボタンを選択中にする条件) */
export function isInSection(section: Section, pathname: string): boolean {
  return activeTab(section, pathname) !== undefined;
}

/** パスが属するセクション(どれにも属さなければ undefined。タブの帯を出す条件) */
export function sectionOfPath(pathname: string): Section | undefined {
  return SECTIONS.find((s) => isInSection(s, pathname));
}

/** サイドバーのボタンを押したときの行き先: そのユーザーに見える最初のタブ */
export function landingHref(section: Section, isAdmin: boolean): string {
  return (visibleTabs(section, isAdmin)[0] ?? section.tabs[0]).href;
}
