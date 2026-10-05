// Server Component wrapper
// dynamicParamsはNext.jsの制約上、リテラルのtrue/falseのみ許可され条件式にできない。
// standalone(Docker本番/ステージング)・npm run dev を優先し、実IDを動的レンダリングする。
// NEXT_OUTPUT=export (cargo run 一本化) でのビルド時のみ、scripts/toggle_export_routes.sh が
// ビルド前にこのファイルをdynamicParams=false版へ一時的に書き換える（ビルド後にgit checkoutで復元）。
import PartnerContractDetailPage from "./client";

export const dynamicParams = true;

export default function Page() {
  return <PartnerContractDetailPage />;
}
