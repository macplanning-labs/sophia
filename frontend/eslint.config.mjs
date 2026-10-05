import { defineConfig, globalIgnores } from "eslint/config";
import nextVitals from "eslint-config-next/core-web-vitals";
import nextTs from "eslint-config-next/typescript";

const eslintConfig = defineConfig([
  ...nextVitals,
  ...nextTs,
  // Override default ignores of eslint-config-next.
  globalIgnores([
    // Default ignores of eslint-config-next:
    ".next/**",
    "out/**",
    "build/**",
    "next-env.d.ts",
  ]),
  // window.confirm は使わない(宛先・件数・金額を示せない)。ConfirmSheet を使う。
  // 置き換え(UI刷新 1-2b)は完了。残るのは撤去予定の tasks 画面のみ(eslint-disable で明示)。
  {
    rules: {
      "no-restricted-properties": [
        "error",
        { object: "window", property: "confirm", message: "window.confirm の代わりに ConfirmSheet を使う" },
      ],
      "no-restricted-globals": [
        "error",
        { name: "confirm", message: "confirm() の代わりに ConfirmSheet を使う" },
      ],
    },
  },
]);

export default eslintConfig;
