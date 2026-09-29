/**
 * sanitize-html.test.ts
 *
 * Test suite for sanitizeTableHtml function.
 * Verifies XSS attack vectors are blocked while table structure is preserved.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { sanitizeTableHtml } from "@/lib/sanitize-html";

describe("sanitizeTableHtml", () => {
  /**
   * Test: <script> tags are completely removed (XSS vector #1)
   */
  it("removes <script> tags entirely", () => {
    const malicious = '<table><tr><td><script>alert("XSS")</script>Data</td></tr></table>';
    const result = sanitizeTableHtml(malicious);

    expect(result).not.toContain("<script>");
    expect(result).not.toContain("alert");
    expect(result).toContain("Data");
  });

  /**
   * Test: Event handler attributes (onerror, onclick, etc.) are removed (XSS vector #2)
   */
  it("removes onerror and other event handler attributes", () => {
    const malicious = '<table><tr><td onerror="alert(\'XSS\')">Content</td></tr></table>';
    const result = sanitizeTableHtml(malicious);

    expect(result).not.toContain("onerror");
    expect(result).not.toContain("alert");
    expect(result).toContain("Content");
  });

  /**
   * Test: javascript: protocol in links is blocked (XSS vector #3)
   */
  it("removes javascript: protocol in href attributes", () => {
    const malicious = '<table><tr><td><a href="javascript:alert(\'XSS\')">Click me</a></td></tr></table>';
    const result = sanitizeTableHtml(malicious);

    expect(result).not.toContain("javascript:");
    expect(result).toContain("Click me");
  });

  /**
   * Test: Legitimate <style> tags are removed (no inline styling, use Tailwind)
   */
  it("removes <style> tags", () => {
    const withStyle = '<table><tr><td><style>body { color: red; }</style>Data</td></tr></table>';
    const result = sanitizeTableHtml(withStyle);

    expect(result).not.toContain("<style>");
    expect(result).not.toContain("{ color: red; }");
    expect(result).toContain("Data");
  });

  /**
   * Test: Core table structure is preserved
   */
  it("preserves valid table structure with colspan and rowspan", () => {
    const validTable = '<table><thead><tr><th>Header</th></tr></thead><tbody><tr><td colspan="2" rowspan="1">Cell</td></tr></tbody></table>';
    const result = sanitizeTableHtml(validTable);

    expect(result).toContain("<table>");
    expect(result).toContain("<thead>");
    expect(result).toContain("<tbody>");
    expect(result).toContain("<tr>");
    expect(result).toContain("<td");
    expect(result).toContain("colspan");
    expect(result).toContain("rowspan");
    expect(result).toContain("Header");
    expect(result).toContain("Cell");
  });

  /**
   * Test: id and class attributes are allowed (needed for styling/accessibility)
   */
  it("preserves id and class attributes for styling", () => {
    const withAttrs = '<table id="excel-preview" class="preview-table"><tr><td class="data-cell">Value</td></tr></table>';
    const result = sanitizeTableHtml(withAttrs);

    expect(result).toContain('id="excel-preview"');
    expect(result).toContain("class");
    expect(result).toContain("preview-table");
    expect(result).toContain("data-cell");
  });

  /**
   * Test: style attribute (inline CSS) is removed to enforce Tailwind-only styling
   */
  it("removes inline style attributes", () => {
    const withInlineStyle = '<table><tr><td style="color: red; background: blue;">Styled</td></tr></table>';
    const result = sanitizeTableHtml(withInlineStyle);

    expect(result).not.toContain('style="');
    expect(result).not.toContain("color: red");
    expect(result).toContain("Styled");
  });

  /**
   * Test: iframe elements are blocked (XSS vector #4 - content injection)
   */
  it("removes iframe elements", () => {
    const withIframe = '<table><tr><td><iframe src="evil.com"></iframe></td></tr></table>';
    const result = sanitizeTableHtml(withIframe);

    expect(result).not.toContain("<iframe>");
    expect(result).not.toContain("evil.com");
  });

  /**
   * Test: Complex real-world Excel output (simulated)
   *
   * Mimics what xlsx sheet_to_html produces: nested tables, formatted cells
   */
  it("handles complex Excel-like table structure", () => {
    const excelLike = `
      <table id="excel-table">
        <tr>
          <td colspan="3">Quarter 1</td>
        </tr>
        <tr>
          <td>Jan</td>
          <td>Feb</td>
          <td>Mar</td>
        </tr>
        <tr>
          <td class="number">1000</td>
          <td class="number">2000</td>
          <td class="number">3000</td>
        </tr>
      </table>
    `;
    const result = sanitizeTableHtml(excelLike);

    expect(result).toContain("<table");
    expect(result).toContain("colspan");
    expect(result).toContain("Quarter 1");
    expect(result).toContain("Jan");
    expect(result).toContain("Feb");
    expect(result).toContain("Mar");
    expect(result).toContain("1000");
    expect(result).toContain("2000");
    expect(result).toContain("3000");
  });

  /**
   * Test: SSR case where window is undefined
   *
   * In static export mode (SSR), DOMPurify is unavailable.
   * The function should return HTML-escaped text instead of throwing.
   */
  it("handles SSR case (window undefined) gracefully", () => {
    // Save original window reference
    const originalWindow = global.window;

    try {
      // Simulate SSR by deleting window
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      delete (global as any).window;

      // This should not throw, but return escaped HTML
      const input = '<table><tr><td>Test</td></tr></table>';
      const result = sanitizeTableHtml(input);

      // The result should be a string (escaped, not DOM-processed)
      expect(typeof result).toBe("string");
      // Escaped version should contain the table text
      expect(result).toContain("Test");
    } finally {
      // Restore window
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (global as any).window = originalWindow;
    }
  });
});
