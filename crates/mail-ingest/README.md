# mail-ingest

メール取込の共通部品（IMAP の読み取りと MIME の解読）。Sophia のメール自動取込が使う。

- IMAP: 受信箱を読み取り専用（EXAMINE）で開き、既読フラグを変えずに、指定日以降のメールや、Message-ID で指定したメールの添付を取り出す。
- MIME: 件名・差出人のデコード、本文（text/plain 優先、無ければ HTML のタグを除く）、添付の取り出し。

DB にも特定のアプリケーションにも依存しない。IMAP の通信は同期（ブロッキング）I/O のため、非同期の処理からは `tokio::task::spawn_blocking` などの中で呼ぶ。
