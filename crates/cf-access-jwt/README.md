# cf-access-jwt

Cloudflare Access が付与する JWT(`Cf-Access-Jwt-Assertion`)の検証と、axum の Extractor。認証元が切り替わっても、取り出すメールアドレスは同じになる。Sophia のログインが使う。

鍵は公開鍵(JWKS)を取得して検証する。テストの署名鍵は実行時に生成する(リポジトリに鍵を置かない)。
