"use client";

/**
 * ポータルマニュアルページ
 *
 * docs/partner_portal_manual.md の内容を画面上に表示する。
 * 画像はpublicディレクトリから取得。
 */

export default function PortalManualPage() {
  return (
    <div className="p-6 max-w-4xl">
      <div className="mb-6">
        <h1 className="text-xl font-bold text-foreground flex items-center gap-2">
          <span className="text-blue-400">📖</span> 操作マニュアル
        </h1>
        <p className="text-xs text-muted-foreground mt-1">Sophia パートナーポータルの操作方法</p>
      </div>

      <div className="bg-card border border-border rounded-lg p-6 space-y-8 text-sm leading-relaxed">
        {/* はじめに */}
        <section>
          <h2 className="text-lg font-bold text-foreground mb-3 border-b border-border pb-2">はじめに</h2>
          <p className="text-muted-foreground">本マニュアルは、Sophia パートナーポータルの操作方法について説明します。</p>
          <table className="mt-3 w-full border-collapse text-xs">
            <thead>
              <tr className="border-b border-border">
                <th className="text-left py-2 px-3 text-muted-foreground">メニュー</th>
                <th className="text-left py-2 px-3 text-muted-foreground">概要</th>
              </tr>
            </thead>
            <tbody>
              <tr className="border-b border-border/50"><td className="py-2 px-3">📋 注文一覧</td><td className="py-2 px-3 text-muted-foreground">弊社からの注文書の確認・承諾</td></tr>
              <tr className="border-b border-border/50"><td className="py-2 px-3">📄 請求一覧</td><td className="py-2 px-3 text-muted-foreground">支払通知書・請求書の確認・承諾</td></tr>
              <tr className="border-b border-border/50"><td className="py-2 px-3">📝 稼働報告</td><td className="py-2 px-3 text-muted-foreground">稼働報告書のアップロード・日次入力</td></tr>
              <tr><td className="py-2 px-3">📖 マニュアル</td><td className="py-2 px-3 text-muted-foreground">本マニュアルの表示</td></tr>
            </tbody>
          </table>
        </section>

        {/* 1. ログイン */}
        <section>
          <h2 className="text-lg font-bold text-foreground mb-3 border-b border-border pb-2">1. ログイン方法</h2>
          <ol className="list-decimal list-inside space-y-1 text-muted-foreground">
            <li>弊社からお送りした<strong className="text-foreground">ログインURL</strong>（マジックリンク）をクリックします</li>
            <li>メールに記載のリンクをクリックすると、自動的にポータルにログインされます</li>
            <li>リンクは一定時間で無効になります。期限切れの場合は弊社担当者にご連絡ください</li>
          </ol>
          <div className="mt-3 bg-amber-500/10 border border-amber-500/20 rounded-lg px-4 py-2.5">
            <p className="text-xs text-amber-300">⚠️ ログインリンクは1回限り有効です。ブックマークしてもご利用いただけません。</p>
          </div>
        </section>

        {/* 2. 注文書一覧 */}
        <section>
          <h2 className="text-lg font-bold text-foreground mb-3 border-b border-border pb-2">2. 注文書一覧</h2>
          <p className="text-muted-foreground mb-3">弊社からの注文書（発注書）を確認・承諾できます。</p>

          <h3 className="text-sm font-semibold text-foreground mt-4 mb-2">注文書の確認方法</h3>
          <ol className="list-decimal list-inside space-y-1 text-muted-foreground">
            <li>一覧の中から確認したい注文書の行をクリックします</li>
            <li>画面上部にアクションボタン（注文書PDF / 注文請書PDF / ダウンロード）が表示されます</li>
            <li>画面下部にPDFのプレビューが表示されます</li>
          </ol>

          <h3 className="text-sm font-semibold text-foreground mt-4 mb-2">注文書の承諾方法</h3>
          <ol className="list-decimal list-inside space-y-1 text-muted-foreground">
            <li>ステータスが<strong className="text-foreground">「送付済」</strong>の注文書の行をクリックします</li>
            <li><strong className="text-foreground">「承諾する」ボタン</strong>をクリックします</li>
            <li>確認ダイアログが表示されます。内容をご確認の上「OK」をクリックしてください</li>
            <li>承諾が完了すると、ステータスが<strong className="text-foreground">「承諾済」</strong>に変わります</li>
          </ol>
          <div className="mt-3 bg-red-500/10 border border-red-500/20 rounded-lg px-4 py-2.5">
            <p className="text-xs text-red-300">⚠️ 一度承諾した注文書は取り消しできません。内容に疑問がある場合は、承諾前に弊社担当者にお問い合わせください。</p>
          </div>
        </section>

        {/* 3. 請求書一覧 */}
        <section>
          <h2 className="text-lg font-bold text-foreground mb-3 border-b border-border pb-2">3. 請求書一覧</h2>
          <p className="text-muted-foreground mb-3">弊社からの支払通知書・請求書を確認・承諾できます。</p>

          <h3 className="text-sm font-semibold text-foreground mt-4 mb-2">請求書の承諾について</h3>
          <p className="text-muted-foreground">
            弊社が代理作成した請求書を、承諾操作により<strong className="text-foreground">「貴社が発行した請求書」</strong>として正式に受理します。
          </p>
          <div className="mt-3 bg-blue-500/10 border border-blue-500/20 rounded-lg px-4 py-2.5">
            <p className="text-xs text-blue-300">💡 金額や明細に不明な点がある場合は、承諾前に弊社担当者にご確認ください。</p>
          </div>
        </section>

        {/* 4. 稼働報告 */}
        <section>
          <h2 className="text-lg font-bold text-foreground mb-3 border-b border-border pb-2">4. 稼働報告</h2>
          <p className="text-muted-foreground mb-3">月次の稼働報告書をアップロード、または日次で勤務時間を入力できます。</p>

          <h3 className="text-sm font-semibold text-foreground mt-4 mb-2">Excelファイルのアップロード手順</h3>
          <ol className="list-decimal list-inside space-y-1 text-muted-foreground">
            <li>サイドバーの<strong className="text-foreground">「稼働報告」</strong>をクリックします</li>
            <li>Excelファイルをドラッグ＆ドロップします（または「ファイルを選択」ボタン）</li>
            <li>解析結果の<strong className="text-foreground">確認画面</strong>が表示されます</li>
            <li>左のExcel元データと右の解析結果を<strong className="text-foreground">見比べて確認</strong>します</li>
            <li>問題なければ<strong className="text-foreground">「この内容で登録する」</strong>をクリックします</li>
          </ol>

          <h3 className="text-sm font-semibold text-foreground mt-4 mb-2">確認画面（左右分割）の見方</h3>
          <table className="mt-2 w-full border-collapse text-xs">
            <thead>
              <tr className="border-b border-border">
                <th className="text-left py-2 px-3 text-muted-foreground">エリア</th>
                <th className="text-left py-2 px-3 text-muted-foreground">表示内容</th>
              </tr>
            </thead>
            <tbody>
              <tr className="border-b border-border/50"><td className="py-2 px-3 font-medium">左パネル</td><td className="py-2 px-3 text-muted-foreground">アップロードしたExcelの元データ</td></tr>
              <tr className="border-b border-border/50"><td className="py-2 px-3 font-medium">右パネル上部</td><td className="py-2 px-3 text-muted-foreground">サマリー（作業者名・対象月・合計時間・稼働日数・残業時間）</td></tr>
              <tr className="border-b border-border/50"><td className="py-2 px-3 font-medium">右パネル中央</td><td className="py-2 px-3 text-muted-foreground">日別明細テーブル（日付・曜日・開始・終了・休憩・実働時間）</td></tr>
              <tr><td className="py-2 px-3 font-medium">右パネル下部</td><td className="py-2 px-3 text-muted-foreground">アラート（土日祝稼働・休日稼働など）</td></tr>
            </tbody>
          </table>

          <div className="mt-3 bg-emerald-500/10 border border-emerald-500/20 rounded-lg px-4 py-2.5">
            <p className="text-xs text-emerald-300">✅ 「この内容で登録する」を押すまでデータは保存されません。安心して確認作業を行ってください。</p>
          </div>

          <h3 className="text-sm font-semibold text-foreground mt-4 mb-2">対応ファイル形式</h3>
          <ul className="list-disc list-inside space-y-1 text-muted-foreground">
            <li><code className="bg-muted px-1 rounded text-xs">.xlsx</code> — Excel 2007以降</li>
            <li><code className="bg-muted px-1 rounded text-xs">.xlsm</code> — マクロ付きExcel</li>
          </ul>
        </section>

        {/* 5. FAQ */}
        <section>
          <h2 className="text-lg font-bold text-foreground mb-3 border-b border-border pb-2">5. よくある質問（FAQ）</h2>
          <div className="space-y-4">
            <FaqItem q="ログインリンクの期限が切れました" a="弊社担当者にご連絡ください。新しいログインリンクをお送りいたします。" />
            <FaqItem q="注文書の承諾を取り消したい" a="システム上での取り消しはできません。弊社担当者にご連絡ください。" />
            <FaqItem q="稼働報告書のアップロードでエラーが出る" a="ファイル形式が .xlsx / .xlsm であること、ファイルが破損していないこと、ファイルサイズが50MB以下であることをご確認ください。" />
            <FaqItem q="パスワードを忘れました" a="本システムはパスワード不要（マジックリンク方式）です。ログインリンクは弊社担当者から送付されます。" />
          </div>
        </section>

        <p className="text-xs text-muted-foreground text-center pt-4 border-t border-border">
          ご不明な点がございましたら、弊社担当者までご連絡ください。 — 最終更新: 2026年7月
        </p>
      </div>
    </div>
  );
}

function FaqItem({ q, a }: { q: string; a: string }) {
  return (
    <div>
      <p className="text-sm font-medium text-foreground">Q: {q}</p>
      <p className="text-sm text-muted-foreground mt-1">A: {a}</p>
    </div>
  );
}
