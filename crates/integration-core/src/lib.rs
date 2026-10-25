//! integration-core — 外部連携の共通の決まり
//!
//! 「何をするか」(保存する [`FileStore`]・知らせる [`Notifier`])と、
//! 「どこにつなぐか」(Google Drive・Google Chat などの部品)を分けるための土台。
//!
//! - 各部品は [`Integration`] を実装し、「動くのに必要な設定」([`SettingSpec`])と
//!   「点検」([`Integration::check`])を持つ。点検は相手に何も書き込まない。
//! - アプリは「使う部品の一覧」([`Manifest`]、`integrations.toml`)を持ち、
//!   起動時・毎朝・デプロイ前に、一覧と実際の設定を照らし合わせる([`evaluate`])。
//! - 部品は設定の値を引数で受け取る。環境変数や DB を直接読まない(どこから読むかはアプリが決める)。

mod check;
mod manifest;
mod notify;
mod store;

pub use check::{CheckItem, CheckReport, Integration, SettingSpec};
pub use manifest::{evaluate, Manifest, ManifestError, UseDecl, UseReport, UseStatus};
pub use notify::{CompositeNotifier, Delivery, Level, Notice, Notifier, NotifyError};
pub use store::{versioned_name, FileStore, StoreError, StoredFile};
