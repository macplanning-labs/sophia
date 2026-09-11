# fonts/

## IPAexゴシックフォントの配置

PDF生成（注文書・請求書等）に IPAexゴシック が必要です。

### ダウンロード手順

1. [IPAフォントダウンロードページ](https://moji.or.jp/ipafont/ipafontdownload/) からIPAexゴシック (ipaexg.ttf) をダウンロード
2. このディレクトリに `ipaexg.ttf` として配置

```
fonts/
  ipaexg.ttf    ← ここに配置
  README.md     ← このファイル
```

### 注意事項
- Dockerfileでこのディレクトリを `/app/fonts/` にコピーしています
- フォントファイルがない場合、PDF生成はエラーを返します（システム自体は動作します）
