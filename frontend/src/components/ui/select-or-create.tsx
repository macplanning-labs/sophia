"use client";

// 既存レコードを選ぶか、その場で新規作成するかを切り替える汎用パーツ。
// WS3(案件作成ウィザード)の技術者・作業場所ステップで共通利用する
// (このコードベース初の select-or-create UI。使い回せるよう汎用化した)。

import { FormInput, FormSelect } from "@/components/ui/form-modal";

export interface SelectOrCreateOption {
  value: string;
  label: string;
}

export type SelectOrCreateValue =
  | { kind: "existing"; id: string }
  | { kind: "new"; fields: Record<string, string> };

export interface NewFieldDef {
  key: string;
  label: string;
  type?: "text" | "email" | "select";
  options?: SelectOrCreateOption[];
  required?: boolean;
}

interface SelectOrCreateProps {
  options: SelectOrCreateOption[];
  value: SelectOrCreateValue;
  onChange: (value: SelectOrCreateValue) => void;
  placeholder?: string;
  /** 新規作成モード時に表示する入力欄の定義 */
  newFields: NewFieldDef[];
}

export function emptySelectOrCreate(): SelectOrCreateValue {
  return { kind: "existing", id: "" };
}

export function SelectOrCreate({ options, value, onChange, placeholder = "選択...", newFields }: SelectOrCreateProps) {
  const isNew = value.kind === "new";

  const toggleNew = () => {
    if (isNew) {
      onChange({ kind: "existing", id: "" });
    } else {
      onChange({ kind: "new", fields: Object.fromEntries(newFields.map((f) => [f.key, ""])) });
    }
  };

  const setNewField = (key: string, v: string) =>
    onChange({
      kind: "new",
      fields: { ...(value.kind === "new" ? value.fields : {}), [key]: v },
    });

  if (!isNew) {
    return (
      <div className="flex gap-2">
        <FormSelect
          className="flex-1"
          options={options}
          placeholder={placeholder}
          value={value.kind === "existing" ? value.id : ""}
          onChange={(e) => onChange({ kind: "existing", id: e.target.value })}
        />
        <button
          type="button"
          onClick={toggleNew}
          className="text-xs text-blue-400 hover:text-blue-300 whitespace-nowrap px-2"
        >
          + 新規作成
        </button>
      </div>
    );
  }

  return (
    <div className="space-y-2 border border-border rounded-md p-3 bg-muted/30">
      <div className="flex items-center justify-between">
        <span className="text-xs text-muted-foreground">新規作成</span>
        <button type="button" onClick={toggleNew} className="text-xs text-muted-foreground hover:text-foreground">
          既存から選ぶ
        </button>
      </div>
      {newFields.map((f) =>
        f.type === "select" ? (
          <FormSelect
            key={f.key}
            options={f.options ?? []}
            placeholder={f.label}
            value={value.fields[f.key] ?? ""}
            onChange={(e) => setNewField(f.key, e.target.value)}
          />
        ) : (
          <FormInput
            key={f.key}
            type={f.type ?? "text"}
            placeholder={f.label}
            value={value.fields[f.key] ?? ""}
            onChange={(e) => setNewField(f.key, e.target.value)}
          />
        )
      )}
    </div>
  );
}

/** SelectOrCreateValue をバックエンドの EngineerRef/WorkLocationRef 相当のJSONに変換する。
 *  Existingの場合はidを数値化、Newの場合はfieldsをそのままオブジェクトとして展開する。 */
export function selectOrCreateToRef(
  v: SelectOrCreateValue,
  extraNewFields?: Record<string, unknown>
// eslint-disable-next-line @typescript-eslint/no-explicit-any
): any {
  if (v.kind === "existing") {
    return { kind: "existing", id: Number(v.id) };
  }
  return { kind: "new", ...v.fields, ...extraNewFields };
}
