"use client";

import { FormField, FormInput, FormSelect, FormTextarea } from "@/components/ui/form-modal";
import { ACCOUNT_NUMBER_FIELD, ACCOUNT_NUMBER_INPUT_PROPS, type MasterFormColumnMeta } from "@/lib/master-form";

/** マスタ定義の1項目を、入力欄にする(マスタ画面・取引先画面で共通)。 */
export function DynamicField({
  col,
  value,
  onChange,
}: {
  col: MasterFormColumnMeta;
  value: string;
  onChange: (v: string) => void;
}) {
  if (col.input_type === "select" || col.input_type === "fk_select") {
    return (
      <FormField label={col.label} required={col.required}>
        <FormSelect
          value={value}
          onChange={(e) => onChange(e.target.value)}
          options={[
            { value: "", label: "— 選択 —" },
            ...(col.options ?? []),
          ]}
        />
      </FormField>
    );
  }

  if (col.input_type === "checkbox") {
    return (
      <FormField label={col.label} required={col.required}>
        <label className="flex items-center gap-2 cursor-pointer">
          <input
            type="checkbox"
            checked={value === "true" || value === "on"}
            onChange={(e) => onChange(e.target.checked ? "true" : "false")}
            className="w-4 h-4 rounded border-border bg-muted text-primary focus:ring-primary/50"
          />
          <span className="text-sm text-foreground">有効</span>
        </label>
      </FormField>
    );
  }

  if (col.input_type === "textarea") {
    return (
      <FormField label={col.label} required={col.required}>
        <FormTextarea value={value} onChange={(e) => onChange(e.target.value)} rows={3} />
      </FormField>
    );
  }

  return (
    <FormField label={col.label} required={col.required}>
      <FormInput
        type={col.input_type === "number" ? "number" : col.input_type === "email" ? "email" : col.input_type === "date" ? "date" : "text"}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        required={col.required}
        {...(col.name === ACCOUNT_NUMBER_FIELD ? ACCOUNT_NUMBER_INPUT_PROPS : {})}
      />
    </FormField>
  );
}
