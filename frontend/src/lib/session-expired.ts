// lib/session-expired.ts — セッション期限切れ(401)通知のpub-sub
//
// fetchJson はReactツリー外のプレーン関数なので、モーダル表示はイベント経由で疎結合にする。
// QueryProvider の mutations.onError はこのエラー型を見て重複トーストを抑止する。

export class SessionExpiredError extends Error {
  constructor() {
    super("SESSION_EXPIRED");
    this.name = "SessionExpiredError";
  }
}

type Listener = (loginUrl: string) => void;

const listeners = new Set<Listener>();

export function notifySessionExpired(loginUrl: string) {
  listeners.forEach((listener) => listener(loginUrl));
}

export function subscribeSessionExpired(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}
