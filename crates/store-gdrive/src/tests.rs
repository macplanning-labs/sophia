//! 偽の Drive(wiremock)を相手にしたテスト

use super::*;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

fn store(server: &MockServer) -> GDriveStore {
    let cfg = GDriveConfig { root_folder_id: "ROOT".into(), api_base: server.uri() };
    GDriveStore::new(cfg, Arc::new(StaticToken("TKN".into()))).unwrap()
}

fn q_of(req: &Request) -> String {
    req.url.query_pairs().find(|(k, _)| k == "q").map(|(_, v)| v.into_owned()).unwrap_or_default()
}

/// 検索の式に、指定の文字列が含まれるときだけ当たる
fn q_contains(needle: &'static str) -> impl Fn(&Request) -> bool {
    move |req: &Request| q_of(req).contains(needle)
}

fn empty_list() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({ "files": [] }))
}

fn one(id: &str, name: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({ "files": [{ "id": id, "name": name }] }))
}

#[test]
fn escapes_quotes_and_backslashes() {
    assert_eq!(escape_query("O'Reilly"), "O\\'Reilly");
    assert_eq!(escape_query("a\\b"), "a\\\\b");
}

#[test]
fn rejects_empty_root() {
    let r = GDriveStore::new(GDriveConfig::new(" "), Arc::new(StaticToken("t".into())));
    assert!(matches!(r, Err(StoreError::Invalid(_))));
}

#[tokio::test]
async fn put_new_creates_missing_folders_with_shared_drive_params() {
    let server = MockServer::start().await;
    // フォルダ「パートナー管理」は既にある、「O'Reilly 社」は無い
    Mock::given(method("GET")).and(path("/drive/v3/files"))
        .and(query_param("supportsAllDrives", "true"))
        .and(query_param("includeItemsFromAllDrives", "true"))
        .and(q_contains("name='パートナー管理' and 'ROOT' in parents"))
        .respond_with(one("F1", "パートナー管理")).expect(1).mount(&server).await;
    Mock::given(method("GET")).and(path("/drive/v3/files"))
        .and(q_contains("name='O\\'Reilly 社' and 'F1' in parents"))
        .respond_with(empty_list()).expect(1).mount(&server).await;
    Mock::given(method("POST")).and(path("/drive/v3/files"))
        .and(query_param("supportsAllDrives", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "id": "F2" })))
        .expect(1).mount(&server).await;
    // 同名ファイルの確認(無い)→ アップロード
    Mock::given(method("GET")).and(path("/drive/v3/files"))
        .and(q_contains("name='a.pdf' and 'F2' in parents"))
        .respond_with(empty_list()).expect(1).mount(&server).await;
    Mock::given(method("POST")).and(path("/upload/drive/v3/files"))
        .and(query_param("uploadType", "multipart")).and(query_param("supportsAllDrives", "true"))
        .and(header("authorization", "Bearer TKN"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "id": "X1", "name": "a.pdf", "webViewLink": "https://drive/x1" })))
        .expect(1).mount(&server).await;

    let f = store(&server).put_new(&["パートナー管理", "O'Reilly 社"], "a.pdf", b"%PDF", "application/pdf").await.unwrap();
    assert_eq!(f.id, "X1");
    assert_eq!(f.name, "a.pdf");
    assert_eq!(f.url.as_deref(), Some("https://drive/x1"));
}

#[tokio::test]
async fn put_new_conflicts_when_same_name_exists() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/drive/v3/files"))
        .respond_with(one("E1", "a.pdf")).mount(&server).await;
    Mock::given(method("POST")).and(path("/upload/drive/v3/files"))
        .respond_with(ResponseTemplate::new(200)).expect(0).mount(&server).await;
    let r = store(&server).put_new(&[], "a.pdf", b"x", "application/pdf").await;
    assert!(matches!(r, Err(StoreError::Conflict(_))));
}

#[tokio::test]
async fn put_versioned_picks_next_free_name() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/drive/v3/files")).and(q_contains("name='PO-1_2026-10.pdf'"))
        .respond_with(one("V1", "PO-1_2026-10.pdf")).mount(&server).await;
    Mock::given(method("GET")).and(path("/drive/v3/files")).and(q_contains("name='PO-1_2026-10_v2.pdf'"))
        .respond_with(one("V2", "PO-1_2026-10_v2.pdf")).mount(&server).await;
    Mock::given(method("GET")).and(path("/drive/v3/files")).and(q_contains("name='PO-1_2026-10_v3.pdf'"))
        .respond_with(empty_list()).mount(&server).await;
    Mock::given(method("POST")).and(path("/upload/drive/v3/files"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "id": "V3", "name": "PO-1_2026-10_v3.pdf" })))
        .expect(1).mount(&server).await;
    let f = store(&server).put_versioned(&[], "PO-1_2026-10.pdf", b"x", "application/pdf").await.unwrap();
    assert_eq!(f.name, "PO-1_2026-10_v3.pdf");
    assert_eq!(f.url.as_deref(), Some("https://drive.google.com/file/d/V3/view"), "リンクが無いときは ID から作る");
}

#[tokio::test]
async fn put_overwrite_updates_existing() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/drive/v3/files"))
        .respond_with(one("E1", "a.xlsx")).mount(&server).await;
    Mock::given(method("PATCH")).and(path("/upload/drive/v3/files/E1"))
        .and(query_param("uploadType", "media")).and(query_param("supportsAllDrives", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "id": "E1" })))
        .expect(1).mount(&server).await;
    let f = store(&server).put_overwrite(&[], "a.xlsx", b"x", "application/octet-stream").await.unwrap();
    assert_eq!(f.id, "E1");
    assert_eq!(f.name, "a.xlsx");
}

#[tokio::test]
async fn http_error_is_not_success() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/drive/v3/files"))
        .respond_with(ResponseTemplate::new(404).set_body_string("{\"error\":\"notFound\"}")).mount(&server).await;
    let r = store(&server).put_new(&["x"], "a.pdf", b"x", "application/pdf").await;
    match r {
        Err(StoreError::Remote(m)) => assert!(m.contains("404")),
        other => panic!("Remote を期待: {other:?}"),
    }
}

#[tokio::test]
async fn rejects_empty_folder_name() {
    let server = MockServer::start().await;
    let r = store(&server).put_new(&["パートナー管理", ""], "a.pdf", b"x", "application/pdf").await;
    assert!(matches!(r, Err(StoreError::Invalid(_))));
}

#[tokio::test]
async fn check_ok_when_folder_writable() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/drive/v3/files/ROOT")).and(query_param("supportsAllDrives", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "ROOT", "mimeType": FOLDER_MIME, "capabilities": { "canAddChildren": true } })))
        .mount(&server).await;
    let r = store(&server).check().await;
    assert!(r.ok(), "{:?}", r.failures());
    assert_eq!(r.items.len(), 3);
}

#[tokio::test]
async fn check_ng_when_not_writable_or_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/drive/v3/files/ROOT"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "ROOT", "mimeType": FOLDER_MIME, "capabilities": { "canAddChildren": false } })))
        .mount(&server).await;
    let r = store(&server).check().await;
    assert!(!r.ok());
    assert!(r.failures()[0].contains("書き込める"));

    let server2 = MockServer::start().await;
    Mock::given(method("GET")).and(path("/drive/v3/files/ROOT"))
        .respond_with(ResponseTemplate::new(404)).mount(&server2).await;
    let r2 = store(&server2).check().await;
    assert!(!r2.ok());
    assert!(r2.failures()[0].contains("見える"));
}

#[tokio::test]
async fn check_ng_when_token_fails() {
    struct Broken;
    #[async_trait]
    impl TokenSource for Broken {
        async fn token(&self, _s: &str) -> Result<String, StoreError> {
            Err(StoreError::Auth("鍵がありません".into()))
        }
    }
    let cfg = GDriveConfig::new("ROOT");
    let s = GDriveStore::new(cfg, Arc::new(Broken)).unwrap();
    let r = s.check().await;
    assert!(!r.ok());
    assert_eq!(r.items.len(), 1);
}

#[test]
fn bad_key_json_is_auth_error() {
    let r = ServiceAccountTokenSource::from_json("{\"private_key\":\"SECRET\"}", None);
    match r {
        Err(StoreError::Auth(m)) => assert!(!m.contains("SECRET")),
        _ => panic!("Auth を期待"),
    }
}

#[tokio::test]
async fn folder_name_reads_name_only() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/drive/v3/files/ROOT")).and(query_param("fields", "name"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "name": "sophia-stg" })))
        .mount(&server).await;
    assert_eq!(store(&server).folder_name().await.unwrap(), "sophia-stg");
}
