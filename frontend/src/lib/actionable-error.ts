import type { ActionBlocker, ApiResult } from "./types";

export function showActionableError(
  toast: typeof import("sonner").toast,
  result: ApiResult,
  title = "操作を完了できませんでした"
): void {
  const blockers = result.blockers ?? [];

  if (blockers.length > 0) {
    const description = blockers
      .map((b) => `${b.subject}: ${b.reason}\n→ ${b.suggestion}`)
      .join("\n\n");

    toast.error(title, {
      description,
      duration: 15000,
    });
  } else if (result.message) {
    toast.error(title, {
      description: result.message,
      duration: 10000,
    });
  } else {
    toast.error(title, {
      duration: 10000,
    });
  }
}
