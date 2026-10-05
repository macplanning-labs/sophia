/**
 * Sophia Landing Page — Micro-animations & Interactions
 */

// ─── Scroll-based nav background ───
const nav = document.getElementById("nav");
window.addEventListener("scroll", () => {
  if (window.scrollY > 50) {
    nav.style.borderBottomColor = "rgba(255,255,255,0.08)";
  } else {
    nav.style.borderBottomColor = "rgba(255,255,255,0.06)";
  }
});

// ─── Fade-in on scroll (Intersection Observer) ───
const observerOptions = { threshold: 0.1, rootMargin: "0px 0px -40px 0px" };

const fadeObserver = new IntersectionObserver((entries) => {
  entries.forEach((entry) => {
    if (entry.isIntersecting) {
      entry.target.classList.add("visible");
      fadeObserver.unobserve(entry.target);
    }
  });
}, observerOptions);

document.querySelectorAll(
  ".feature-card, .section-header"
).forEach((el) => {
  el.style.opacity = "0";
  el.style.transform = "translateY(20px)";
  el.style.transition = "opacity 0.6s ease, transform 0.6s ease";
  fadeObserver.observe(el);
});

// Add visible class styles
const style = document.createElement("style");
style.textContent = `.visible { opacity: 1 !important; transform: translateY(0) !important; }`;
document.head.appendChild(style);

// ─── Smooth anchor scrolling ───
document.querySelectorAll('a[href^="#"]').forEach((anchor) => {
  anchor.addEventListener("click", (e) => {
    const href = anchor.getAttribute("href");
    if (href === "#") return;
    e.preventDefault();
    const target = document.querySelector(href);
    if (target) {
      target.scrollIntoView({ behavior: "smooth", block: "start" });
    }
  });
});

// ─── Stagger animation for grid items ───
document.querySelectorAll(".features-grid").forEach((grid) => {
  const items = grid.children;
  Array.from(items).forEach((item, i) => {
    item.style.transitionDelay = `${i * 0.08}s`;
  });
});

// ─── Desktop App Download (Tauri) ───
//
// Links to the GitHub Releases page rather than auto-detecting a specific binary
// asset via the releases API, for simplicity — OS detection below is used only to
// customize the button's label text, purely cosmetic.
