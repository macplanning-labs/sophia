/**
 * sanitize-html.ts
 *
 * HTML sanitization for rendering untrusted HTML content (e.g., Excel sheet_to_html output).
 * Uses DOMPurify with strict allowlist for table elements to prevent XSS.
 *
 * Handles SSR case where `window` is undefined by returning HTML-escaped string.
 */
import DOMPurify from "dompurify";

/**
 * Sanitize HTML for safe rendering in the browser.
 * Permits table elements and their attributes, blocks script/style/form tags and event handlers.
 *
 * @param html - Raw HTML string to sanitize (e.g., from xlsx sheet_to_html)
 * @returns Sanitized HTML string safe for dangerouslySetInnerHTML
 *
 * SSR: if `window` is undefined (static export), returns HTML-escaped string
 * to avoid hydration mismatches.
 */
export function sanitizeTableHtml(html: string): string {
  // SSR case: no DOM available (static export)
  if (typeof window === "undefined") {
    // Return HTML-escaped plain text to avoid hydration errors
    return html
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#x27;");
  }

  // Configure DOMPurify with strict allowlist
  const config = {
    // Forbid dangerous tags (XSS vectors)
    FORBID_TAGS: ["script", "style", "iframe", "object", "embed", "form"],

    // Forbid style attribute (decoration via Tailwind, not inline styles)
    FORBID_ATTR: ["style"],

    // Allow table-related tags
    ALLOWED_TAGS: [
      "table",
      "thead",
      "tbody",
      "tfoot",
      "tr",
      "td",
      "th",
      "caption",
      "colgroup",
      "col",
      // Preserve basic text formatting
      "b",
      "i",
      "em",
      "strong",
      "u",
      "br",
      "p",
      "span",
      "div",
    ],

    // Allow safe attributes on allowed tags
    ALLOWED_ATTR: ["colspan", "rowspan", "id", "class"],
  };

  return DOMPurify.sanitize(html, config);
}
