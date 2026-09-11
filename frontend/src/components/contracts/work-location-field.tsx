"use client";

import { FormField, FormInput } from "@/components/ui/form-modal";

/** 作業場所のデフォルト（汎用マスタ廃止後の標準値） */
export const DEFAULT_WORK_LOCATION = "弊社指定場所";

interface Props {
  value: string;
  onChange: (value: string) => void;
  /** 追加の候補（既存マスタ名など） */
  suggestions?: string[];
}

/**
 * 作業場所入力: 「弊社指定場所」をデフォルトとし、自由入力も可。
 * datalist で候補を提示する。
 */
export function WorkLocationField({ value, onChange, suggestions = [] }: Props) {
  const listId = "work-location-suggestions";
  const options = Array.from(
    new Set([DEFAULT_WORK_LOCATION, ...suggestions.filter(Boolean)])
  );

  return (
    <FormField label="作業場所">
      <FormInput
        list={listId}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={DEFAULT_WORK_LOCATION}
      />
      <datalist id={listId}>
        {options.map((name) => (
          <option key={name} value={name} />
        ))}
      </datalist>
      <p className="text-[10px] text-muted-foreground mt-1">
        デフォルトは「{DEFAULT_WORK_LOCATION}」。候補から選ぶか、自由に入力できます。
      </p>
    </FormField>
  );
}
