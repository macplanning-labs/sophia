// 要員追加の入力検証と、送信内容の組み立て(画面から切り出した純関数)。
// 実データの値: 自社社員=EMPLOYEE / パートナー要員=PARTNER。パートナー要員は所属のパートナー(m_engineer.partner_id)を持つ。
// 入口は「自社社員追加」と「パートナー要員追加」で別。パートナー要員は、新規の要員を作る方法と、
// 登録済みの要員をそのパートナーに所属させる方法がある。

import type { MemberRow } from "./types";

export type Classification = "EMPLOYEE" | "PARTNER";
/** パートナー要員の追加方法: 新規に作る / 登録済みの要員を選ぶ */
export type AddMode = "new" | "existing";

export interface AddMemberForm {
  classification: Classification;
  mode: AddMode;
  name: string;
  name_kana: string;
  email: string;
  /** 所属させるパートナーのID(newPartner が無いとき) */
  partner_id: string;
  /** 新しいパートナーを同時に作るときの入力。null なら既存から選ぶ */
  newPartner: { name: string; contact_person: string; email: string } | null;
  /** mode=existing のとき、選んだ登録済みの要員のID */
  existing_id: string;
}

/** 入力の検証。問題があれば、画面に出せる日本語を返す。なければ null */
export function validateAddMember(f: AddMemberForm): string | null {
  if (f.classification === "PARTNER") {
    if (f.newPartner) {
      if (!f.newPartner.name.trim()) return "新しいパートナーの名称を入力してください";
    } else if (!f.partner_id.trim()) {
      return "パートナーを選択してください";
    }
    if (f.mode === "existing") {
      return f.existing_id.trim() ? null : "登録済みの要員を選択してください";
    }
  }
  if (!f.name.trim()) return "氏名を入力してください";
  return null;
}

/** 新規の要員の登録内容。自社社員はパートナーを持たない */
export function buildMemberPayload(f: AddMemberForm, partnerId: string | null) {
  return {
    name: f.name.trim(),
    name_kana: f.name_kana.trim(),
    affiliation_type: f.classification,
    partner_id: f.classification === "PARTNER" ? partnerId ?? undefined : undefined,
    email: f.email.trim(),
  };
}

/**
 * 登録済みの要員を、パートナーに所属させるときの更新内容。
 * マスタの更新は、フォームの全項目を送る作り(送らない項目は空になる)ため、いまの値をすべて引き継ぐ。
 */
export function buildAttachPayload(member: MemberRow, partnerId: string): Record<string, unknown> {
  return {
    name: member.name,
    name_kana: member.name_kana ?? "",
    affiliation_type: member.affiliation_type,
    partner_id: partnerId,
    email: member.email ?? "",
    is_active: member.is_active,
  };
}

/** 新しいパートナーの登録内容(マスタのパートナーに出す項目) */
export function buildPartnerPayload(p: { name: string; contact_person: string; email: string }) {
  return { name: p.name.trim(), contact_person: p.contact_person.trim(), email: p.email.trim() };
}

/**
 * 「登録済みの要員から選ぶ」の候補。有効なパートナー要員のうち、選んだパートナーにまだ所属していない人。
 * 表示用に、いまの所属(別のパートナー名 / 未所属)も付ける。
 */
export function attachCandidates(
  members: MemberRow[],
  partnerId: string,
  partnerNameById: Record<string, string>,
): { value: string; label: string; currentPartnerName: string | null }[] {
  return members
    .filter((m) => m.is_active && m.affiliation_type === "PARTNER" && (m.partner_id ?? "") !== partnerId)
    .map((m) => {
      const current = m.partner_id ? partnerNameById[m.partner_id] ?? m.partner_id : null;
      return {
        value: String(m.id),
        label: `${m.name}(${current ? `所属: ${current}` : "所属なし"})`,
        currentPartnerName: current,
      };
    });
}
