import { useEffect, useState } from "react";

/** 値の変化が `delayMs` 止まってから反映する(入力のたびにAPIを呼ばないため) */
export function useDebounced<T>(value: T, delayMs: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const id = setTimeout(() => setDebounced(value), delayMs);
    return () => clearTimeout(id);
  }, [value, delayMs]);
  return debounced;
}
