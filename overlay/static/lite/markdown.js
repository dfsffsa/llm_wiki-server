import { marked } from "./vendor/marked.esm.js";
import DOMPurify from "./vendor/purify.es.js";

marked.setOptions({
  gfm: true,
  breaks: true,
});

export function renderMarkdown(text) {
  if (!text) return "";
  const html = marked.parse(text);
  return DOMPurify.sanitize(html, {
    USE_PROFILES: { html: true },
    ADD_ATTR: ["target", "rel"],
  });
}

export function setAssistantBubbleContent(el, text) {
  if (!el) return;
  if (!text) {
    el.textContent = "";
    el.classList.remove("msg-md");
    return;
  }
  el.classList.add("msg-md");
  let html = renderMarkdown(text);
  // Replace [N] markers with styled citation badges (sanitize-safe).
  html = html.replace(/\[(\d+)\]/g, '<sup class="ref-badge" data-id="$1">[$1]</sup>');
  el.innerHTML = html;
  for (const link of el.querySelectorAll("a[href]")) {
    link.target = "_blank";
    link.rel = "noopener noreferrer";
  }
}
