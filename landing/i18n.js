// ─── Sophia Landing Page — i18n (EN / JA toggle, no framework) ───
const TRANSLATIONS = {
  ja: {
    "nav.features": "機能",
    "nav.download": "ダウンロード",
    "nav.cta": "ダウンロード",

    "hero.badge": "自社実証プロダクト — JP PINT（Peppol準拠）完全対応",
    "hero.title": "SES受発注・稼働管理・給与を、<span class=\"hero-gradient\">ひとつに。</span>",
    "hero.subtitle": "自社バックオフィスで、いま毎日使い倒しているツールです。日本のデジタルインボイス標準規格「JP PINT（Peppol準拠）」に完全対応し、帳票発行から稼働報告のチェック、案件ごとの収支管理までを一気通貫で行います。",
    "meta.platform": "macOS 12+ / Windows 10+ 対応",
    "meta.internal": "社内配布・要GitHubアクセス権限",
    "meta.warning.summary": "初回起動時に警告が出た場合",
    "warning.mac": "<strong>macOS:</strong> Finderでアプリを右クリック(またはControl+クリック)→「開く」を選択してください",
    "warning.windows": "<strong>Windows:</strong> 「WindowsによってPCが保護されました」と表示された場合は「詳細情報」→「実行」を選択してください",
    "beta.note": "ベータ版として提供中です。不具合を見つけた場合は <a href=\"https://github.com/macplanning-labs/sophia/issues\" target=\"_blank\" rel=\"noopener noreferrer\">GitHub Issues</a> までご報告ください。",

    "demo.badge": "デモ",
    "demo.title": "動くSophiaを、実際の画面で。",
    "demo.subtitle": "日々の請求・稼働・給与業務のワークフローをご覧いただけます。",
    "demo.placeholder": "デモ動画は近日公開予定です",

    "features.badge": "機能",
    "features.title": "現場のリアルな業務から生まれた機能。",
    "features.subtitle": "自社の経理・労務チームが「今日も」実際に動かしているツールです。",
    "feature.1.title": "ワンクリック帳票発行",
    "feature.1.body": "注文書・支払通知書・請求書を1分で生成・送付。パートナー様はパスワード不要でスピード承諾。",
    "feature.2.title": "稼働報告の自動解析",
    "feature.2.body": "Excel／Cross定型PDFを自動読込。精算ラインの超過・不足を判定し自動リマインド。",
    "feature.3.title": "案件別収支ダッシュボード",
    "feature.3.body": "案件ごとの売上・支払・利益を可視化。会社×案件単位で一括請求・支払処理。",
    "feature.4.title": "Rustリアーキテクチャ",
    "feature.4.body": "超高速・省メモリ・堅牢な基盤で、日々の業務を止めない。",

    "cta.title": "請求・稼働管理を、もっとスムーズに。",
    "cta.subtitle": "机上の空論ではなく、自社バックオフィスで実際に動いているツールです。",
    "cta.button": "デスクトップアプリをダウンロード →",

    "footer.tagline": "SES受発注・稼働管理・給与統合システム。",
    "footer.col.product": "製品",
    "footer.col.resources": "リソース",
    "footer.col.company": "会社",
    "footer.link.features": "機能",
    "footer.link.download": "ダウンロード",
    "footer.link.github": "GitHub",
    "footer.link.issues": "Issues",
    "footer.link.about": "MAC Planning",
    "footer.bottom": "© 2026 MAC Planning. Sophiaは社内ツールです — Beta.",
  },
  en: {
    "nav.features": "Features",
    "nav.download": "Download",
    "nav.cta": "Download",

    "hero.badge": "Self-Use Case — JP PINT (Peppol) Ready",
    "hero.title": "SES ordering, timesheets, and payroll — <span class=\"hero-gradient\">unified.</span>",
    "hero.subtitle": "We use this tool every day, in our own back office. It fully supports Japan's digital invoicing standard, JP PINT (Peppol-compliant), covering everything from document issuance to timesheet review to per-project profit tracking.",
    "meta.platform": "macOS 12+ / Windows 10+ supported",
    "meta.internal": "Internal distribution — GitHub access required",
    "meta.warning.summary": "First-time launch warning?",
    "warning.mac": "<strong>macOS:</strong> Right-click (or Control+click) the app in Finder → select \"Open\"",
    "warning.windows": "<strong>Windows:</strong> If prompted with \"Windows protected your PC\", click \"More info\" → \"Run anyway\"",
    "beta.note": "This is a beta release. Please report any issues you find on <a href=\"https://github.com/macplanning-labs/sophia/issues\" target=\"_blank\" rel=\"noopener noreferrer\">GitHub Issues</a>.",

    "demo.badge": "Demo",
    "demo.title": "See Sophia in action.",
    "demo.subtitle": "Watch the day-to-day billing, timesheet, and payroll workflow.",
    "demo.placeholder": "Demo video coming soon",

    "features.badge": "Features",
    "features.title": "Built from real back-office work.",
    "features.subtitle": "Our own accounting and HR team runs on this — today, right now.",
    "feature.1.title": "One-Click Document Issuance",
    "feature.1.body": "Generate and send purchase orders, payment notices, and invoices in under a minute. Partners approve without a password.",
    "feature.2.title": "Automatic Timesheet Parsing",
    "feature.2.body": "Reads Excel and Cross-format PDFs automatically. Detects over/under billing lines and sends automatic reminders.",
    "feature.3.title": "Per-Project P&L Dashboard",
    "feature.3.body": "Visualize revenue, payments, and profit per project. Batch invoicing and payments by company or project.",
    "feature.4.title": "Rust Re-Architecture",
    "feature.4.body": "A fast, lightweight, robust backend that never gets in the way of daily operations.",

    "cta.title": "Smoother billing and timesheet management.",
    "cta.subtitle": "Not a theoretical pitch — this is what actually runs our back office.",
    "cta.button": "Download the Desktop App →",

    "footer.tagline": "SES ordering, timesheet, and payroll management.",
    "footer.col.product": "Product",
    "footer.col.resources": "Resources",
    "footer.col.company": "Company",
    "footer.link.features": "Features",
    "footer.link.download": "Download",
    "footer.link.github": "GitHub",
    "footer.link.issues": "Issues",
    "footer.link.about": "MAC Planning",
    "footer.bottom": "© 2026 MAC Planning. Sophia is an internal tool — Beta.",
  },
};

const PAGE_TITLES = {
  ja: "Sophia — SES受発注・稼働管理・給与統合システム",
  en: "Sophia — SES Ordering, Timesheet & Payroll Management",
};

function detectDefaultLang() {
  try {
    const saved = localStorage.getItem("sophia-lang");
    if (saved === "en" || saved === "ja") return saved;
  } catch (e) {
    /* localStorage unavailable — fall through to browser detection */
  }
  return navigator.language && navigator.language.toLowerCase().startsWith("ja") ? "ja" : "en";
}

function applyLanguage(lang) {
  const dict = TRANSLATIONS[lang] || TRANSLATIONS.en;
  document.documentElement.lang = lang;
  document.title = PAGE_TITLES[lang] || PAGE_TITLES.en;

  document.querySelectorAll("[data-i18n]").forEach((el) => {
    const key = el.getAttribute("data-i18n");
    if (Object.prototype.hasOwnProperty.call(dict, key)) {
      el.innerHTML = dict[key];
    }
  });

  document.querySelectorAll(".lang-toggle-btn").forEach((btn) => {
    btn.classList.toggle("active", btn.dataset.lang === lang);
    btn.setAttribute("aria-pressed", String(btn.dataset.lang === lang));
  });

  try {
    localStorage.setItem("sophia-lang", lang);
  } catch (e) {
    /* ignore — persistence is a nice-to-have, not required */
  }

  window.currentLang = lang;
  document.dispatchEvent(new CustomEvent("sophia-lang-changed", { detail: { lang } }));
}

document.addEventListener("DOMContentLoaded", () => {
  applyLanguage(detectDefaultLang());
  document.querySelectorAll(".lang-toggle-btn").forEach((btn) => {
    btn.addEventListener("click", () => applyLanguage(btn.dataset.lang));
  });
});
