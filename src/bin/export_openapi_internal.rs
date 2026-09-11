/// 社内 OpenAPI JSON エクスポート用 CLI ツール

fn main() {
    let openapi = sophia::openapi::build_internal_v1_api();

    match serde_json::to_string_pretty(&openapi) {
        Ok(json) => println!("{}", json),
        Err(e) => {
            eprintln!("Failed to serialize internal OpenAPI schema: {}", e);
            std::process::exit(1);
        }
    }
}
