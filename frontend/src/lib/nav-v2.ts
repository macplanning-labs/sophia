/**
 * nav-v2.ts — 新UI ナビゲーション設定
 *
 * 【決定】§1-6: 名詞のメニュー。旧UIと並行稼働。
 * - NAV_V2: 新メニュー項目の定義
 * - visibleNavV2: ready と adminOnly で出し分けする純関数
 */

import type React from "react";
import {
  Home,
  Building2,
  Users,
  Briefcase,
  FileInput,
  Receipt,
  Mail,
  BarChart3,
} from "lucide-react";

export interface NavItemV2 {
  /** ナビ項目のキー（一意） */
  key: string;
  /** 表示ラベル（名詞形。助詞なし） */
  label: string;
  /** ページのパス */
  href: string;
  /** lucide-react のアイコンコンポーネント、またはコンポーネント名（文字列） */
  icon: React.ElementType | string;
  /** 管理者限定かどうか（ADMIN ロールのみ表示） */
  adminOnly: boolean;
  /** ページが実装済みかどうか。false の間は表示しない */
  ready: boolean;
}

/**
 * 新UI ナビゲーション項目。
 *
 * 【決定】§1-6 テーブルの順序：
 * 1. ホーム `/home`（全員、3-7 で実装）
 * 2. 取引先 `/parties`（管理者、3-1 で実装）
 * 3. 要員 `/members`（管理者、3-1 で実装）
 * 4. アサイン `/assignments`（管理者、3-2 で実装）
 * 5. 勤務表 `/timesheet-matching`（既存ページ。管理者。新UIのページが1つでも出来るまでは false のまま）
 * 6. 請求書・支払通知 `/billing`（管理者、3-4 で実装）
 * 7. 受信メール `/mail`（管理者、3-5 で実装）
 * 8. 月別状況 `/status`（管理者、3-8 で実装）
 *
 * すべて ready: false で登録（実装フェーズで true に変更）。いまは利用者に見える変化は無い。
 * 勤務表は既存ページだが、新UIのグループに勤務表だけが出ても意味が無いので、最初の新ページ(3-1)と同時に true にする。
 */
export const NAV_V2: NavItemV2[] = [
  {
    key: "home",
    label: "ホーム",
    href: "/home",
    icon: Home,
    adminOnly: false,
    ready: false,
  },
  {
    key: "parties",
    label: "取引先",
    href: "/parties",
    icon: Building2,
    adminOnly: true,
    ready: true,
  },
  {
    key: "members",
    label: "要員",
    href: "/members",
    icon: Users,
    adminOnly: true,
    ready: true,
  },
  {
    key: "assignments",
    label: "アサイン",
    href: "/assignments",
    icon: Briefcase,
    adminOnly: true,
    ready: true,
  },
  {
    key: "timesheet-matching",
    label: "勤務表",
    href: "/timesheet-matching",
    icon: FileInput,
    adminOnly: true,
    ready: true, // 既存ページ。3-1(取引先)と同時に有効化
  },
  {
    key: "billing",
    label: "請求書・支払通知",
    href: "/billing",
    icon: Receipt,
    adminOnly: true,
    ready: true,
  },
  {
    key: "mail",
    label: "受信メール",
    href: "/mail",
    icon: Mail,
    adminOnly: true,
    ready: false,
  },
  {
    key: "status",
    label: "月別状況",
    href: "/status",
    icon: BarChart3,
    adminOnly: true,
    ready: false,
  },
];

/**
 * visibleNavV2 - 新UIナビゲーション項目を出し分け
 *
 * @param isAdmin ログイン中のユーザーが管理者かどうか
 * @param items NAV_V2（テスト時にモック可能にするため引数化）
 * @returns 表示対象の項目のリスト
 *
 * 【決定】§1-6:
 * - ready: true のもののみ表示
 * - adminOnly: true のものは、isAdmin: true の場合のみ表示
 */
export function visibleNavV2(
  isAdmin: boolean,
  items: NavItemV2[] = NAV_V2
): NavItemV2[] {
  return items.filter((item) => {
    // ready: false のものは表示しない
    if (!item.ready) return false;

    // adminOnly: true のものは isAdmin: true の場合のみ表示
    if (item.adminOnly && !isAdmin) return false;

    return true;
  });
}
