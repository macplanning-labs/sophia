import { describe, expect, it } from "vitest";
import {
  CLOSING_SECTION,
  HOME_SECTION,
  PARTIES_SECTION,
  SECTIONS,
  SETTINGS_SECTION,
  STAFF_SECTION,
  TIMESHEETS_SECTION,
  activeTab,
  canSeeTab,
  isInSection,
  landingHref,
  matchesHref,
  sectionOfPath,
  visibleTabs,
} from "../section-tabs";

const labels = (tabs: { label: string }[]) => tabs.map((t) => t.label);

describe("SECTIONS(定義)", () => {
  it("どのタブの URL も、セクションをまたいで重複しない(1つの画面は1つのセクションに属する)", () => {
    const hrefs = SECTIONS.flatMap((s) => s.tabs.map((t) => t.href));
    expect(new Set(hrefs).size).toBe(hrefs.length);
  });

  it("セクションの中で key が重複しない", () => {
    for (const s of SECTIONS) {
      expect(new Set(s.tabs.map((t) => t.key)).size, s.key).toBe(s.tabs.length);
    }
  });

  it("従来の URL がすべてタブに残っている(URLは変えない)", () => {
    const hrefs = SECTIONS.flatMap((s) => s.tabs.map((t) => t.href)).sort();
    expect(hrefs).toEqual(
      [
        "/", "/home",
        "/received-emails", "/sent-emails",
        "/assignments", "/client-contracts", "/partner-contracts",
        "/parties", "/members",
        "/timesheet-matching", "/timesheets", "/my-timesheet",
        "/status", "/settlement", "/billing", "/invoices", "/peppol",
        "/payroll", "/employees", "/expenses",
        "/users", "/settings/org", "/settings/security", "/settings/api-keys", "/settings/integrations", "/masters",
      ].sort()
    );
  });
});

describe("visibleTabs / canSeeTab(権限での出し分け)", () => {
  it("管理者には、すべてのタブが見える", () => {
    for (const s of SECTIONS) {
      expect(visibleTabs(s, true), s.key).toHaveLength(s.tabs.length);
    }
  });

  it("一般社員: 管理者専用のタブは見えない", () => {
    expect(labels(visibleTabs(HOME_SECTION, false))).toEqual(["ダッシュボード"]);
    expect(labels(visibleTabs(TIMESHEETS_SECTION, false))).toEqual(["稼働報告", "自己申告"]);
    expect(labels(visibleTabs(CLOSING_SECTION, false))).toEqual(["月次確定", "請求書一覧"]);
    expect(labels(visibleTabs(STAFF_SECTION, false))).toEqual(["給与", "社員", "経費"]);
    expect(labels(visibleTabs(SETTINGS_SECTION, false))).toEqual(["セキュリティ"]);
  });

  it("一般社員: 要員は、見えるタブが無い", () => {
    expect(visibleTabs(PARTIES_SECTION, false)).toEqual([]);
  });

  it("canSeeTab は、管理者専用のタブを管理者にだけ見せる", () => {
    const adminTab = { key: "a", label: "A", href: "/a", adminOnly: true };
    const anyTab = { key: "b", label: "B", href: "/b" };
    expect(canSeeTab(adminTab, true)).toBe(true);
    expect(canSeeTab(adminTab, false)).toBe(false);
    expect(canSeeTab(anyTab, false)).toBe(true);
  });
});

describe("matchesHref / activeTab / isInSection / sectionOfPath", () => {
  it("そのものと、その配下に一致する", () => {
    expect(matchesHref("/users", "/users")).toBe(true);
    expect(matchesHref("/users/12", "/users")).toBe(true);
    expect(matchesHref("/settings/org/roles", "/settings/org")).toBe(true);
  });

  it("名前が似ているだけのパスには一致しない", () => {
    expect(matchesHref("/usersX", "/users")).toBe(false);
    expect(matchesHref("/masters-old", "/masters")).toBe(false);
    expect(matchesHref("/settings/organization", "/settings/org")).toBe(false);
    // /timesheets と /timesheet-matching は別の画面
    expect(matchesHref("/timesheet-matching", "/timesheets")).toBe(false);
  });

  it("「/」(ダッシュボード)は、トップだけに一致する", () => {
    expect(matchesHref("/", "/")).toBe(true);
    expect(matchesHref("/projects", "/")).toBe(false);
    expect(sectionOfPath("/projects")).toBeUndefined();
  });

  it("各画面と、その配下で、正しいセクションのタブが選択中になる", () => {
    const cases: [string, string, string][] = [
      ["/", "home", "dashboard"],
      ["/home", "home", "todo"],
      ["/assignments", "contracts", "assignments"],
      ["/client-contracts/5", "contracts", "client"],
      ["/partner-contracts", "contracts", "partner"],
      ["/parties/3", "settings", "parties"],
      ["/members", "parties", "members"],
      ["/timesheet-matching", "timesheets", "matching"],
      ["/timesheets/12", "timesheets", "reports"],
      ["/my-timesheet", "timesheets", "self"],
      ["/status", "closing", "status"],
      ["/settlement", "closing", "settlement"],
      ["/billing", "closing", "billing"],
      ["/invoices/9", "closing", "invoices"],
      ["/peppol", "closing", "peppol"],
      ["/payroll", "staff", "payroll"],
      ["/employees/2", "staff", "employees"],
      ["/expenses", "staff", "expenses"],
      ["/users/12", "settings", "users"],
      ["/settings/org/assignments", "settings", "org"],
      ["/settings/security/totp", "settings", "security"],
      ["/masters", "settings", "masters"],
    ];
    for (const [path, sectionKey, tabKey] of cases) {
      const section = sectionOfPath(path);
      expect(section?.key, path).toBe(sectionKey);
      expect(section && activeTab(section, path)?.key, path).toBe(tabKey);
    }
  });

  it("セクションに属さない画面では、タブの帯を出さない", () => {
    for (const p of ["/projects", "/orders/1", "/received-orders/1", "/notices", "/login", "/settings", "/settings/webhook"]) {
      expect(sectionOfPath(p), p).toBeUndefined();
    }
  });

  it("isInSection は、そのセクションの画面でだけ true", () => {
    expect(isInSection(CLOSING_SECTION, "/billing")).toBe(true);
    expect(isInSection(CLOSING_SECTION, "/timesheets")).toBe(false);
  });
});

describe("landingHref(サイドバーのボタンの行き先)", () => {
  it("管理者は、各セクションの最初のタブ", () => {
    expect(landingHref(HOME_SECTION, true)).toBe("/home");
    expect(landingHref(TIMESHEETS_SECTION, true)).toBe("/timesheet-matching");
    expect(landingHref(CLOSING_SECTION, true)).toBe("/status");
    expect(landingHref(SETTINGS_SECTION, true)).toBe("/users");
  });

  it("一般社員は、見える最初のタブ", () => {
    expect(landingHref(HOME_SECTION, false)).toBe("/");
    expect(landingHref(TIMESHEETS_SECTION, false)).toBe("/timesheets");
    expect(landingHref(CLOSING_SECTION, false)).toBe("/settlement");
    expect(landingHref(SETTINGS_SECTION, false)).toBe("/settings/security");
  });
});
