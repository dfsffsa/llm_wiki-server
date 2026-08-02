/**
 * Lite chat UI — static, no bundler.
 */

import { setAssistantBubbleContent } from "./markdown.js";

const CFG = window.LLM_WIKI_LITE_CONFIG || { apiBase: "", apiToken: "" };

const $ = (sel) => document.querySelector(sel);

const state = {
  projects: [],
  meta: {},
  activeProject: null,
  activeMeta: null,
  conversationId: null,
  chatEnabled: true,
  abortController: null,
  sendGeneration: 0,
  user: null,
  usage: null,
  plan: null,
  currentMessages: [],
  lastSources: [], // structured search results for source cards
};

function isAbortError(err) {
  if (!err) return false;
  if (err.name === "AbortError") return true;
  const msg = String(err.message || err).toLowerCase();
  return msg.includes("abort") || msg.includes("bodystreambuffer");
}

// --- API ---

function apiUrl(path, query = {}) {
  const base = (CFG.apiBase || "").replace(/\/$/, "");
  const q = new URLSearchParams();
  if (CFG.apiToken) q.set("token", CFG.apiToken);
  for (const [k, v] of Object.entries(query)) {
    if (v != null) q.set(k, String(v));
  }
  const qs = q.toString();
  return `${base}${path}${qs ? `?${qs}` : ""}`;
}

function apiHeaders(extra = {}) {
  const h = { Accept: "application/json", ...extra };
  if (CFG.apiToken) h.Authorization = `Bearer ${CFG.apiToken}`;
  return h;
}

async function apiGet(path) {
  const res = await fetch(apiUrl(path), { headers: apiHeaders(), credentials: "same-origin" });
  const body = await res.json().catch(() => ({}));
  if (!res.ok || body.ok === false) throw new Error(body.error || res.statusText);
  return body;
}

async function apiPost(path, data) {
  const res = await fetch(apiUrl(path), {
    method: "POST",
    headers: apiHeaders({ "Content-Type": "application/json" }),
    body: JSON.stringify(data),
    credentials: "same-origin",
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({}));
    throw new Error(body.error || res.statusText);
  }
  return res;
}

// --- Auth gate (cookie session) ---

// Returns:
//   {status: "ok", user, usage}  — logged in
//   {status: "no-auth"}          — 401, not logged in (redirect to /login)
//   {status: "disabled"}         — 500/auth-disabled or network error: proceed
//                                  in shared-token Bearer mode (no login required)
async function fetchMe() {
  try {
    const res = await fetch(`${(CFG.apiBase || "").replace(/\/$/, "")}/auth/me`, {
      credentials: "same-origin",
    });
    if (res.ok) {
      const data = await res.json();
      return { status: "ok", user: data.user, usage: data.usage, plan: data.plan };
    }
    if (res.status === 401) return { status: "no-auth" };
    // 500 (auth disabled on server) or other — treat as "auth not configured",
    // fall back to shared-token Bearer mode. Don't redirect.
    return { status: "disabled" };
  } catch {
    return { status: "disabled" };
  }
}

async function ensureLogin() {
  const me = await fetchMe();
  if (me.status === "no-auth") {
    location.href = "/login";
    return false;
  }
  if (me.status === "ok") {
    state.user = me.user;
    state.usage = me.usage;
    state.plan = me.plan;
    renderSidebarUser();
  }
  // status === "disabled": proceed without user/quota (shared-token mode)
  return true;
}

function renderSidebarUser() {
  const uel = $("#sidebar-user");
  const usageEl = $("#sidebar-usage");
  if (!state.user) {
    // Shared-token mode: hide footer
    const footer = document.querySelector(".sidebar-footer");
    if (footer) footer.style.display = "none";
    return;
  }
  const footer = document.querySelector(".sidebar-footer");
  if (footer) footer.style.display = "flex";
  if (uel) uel.textContent = state.user?.display_name || state.user?.email || "";
  if (usageEl && state.plan && state.plan.name === "pro") {
    // Pro member: show badge + period end instead of the free daily quota.
    const end = state.plan.periodEnd
      ? new Date(state.plan.periodEnd * 1000).toLocaleDateString("zh-CN")
      : "";
    usageEl.textContent = `Pro 会员${end ? ` · ${end} 到期` : ""}`;
    usageEl.classList.remove("low");
  } else if (usageEl && state.usage) {
    const remaining = Math.max(0, state.usage.limit - state.usage.used);
    usageEl.textContent = I18N.tpl("lite.usage.remaining", { used: remaining, limit: state.usage.limit });
    usageEl.classList.toggle("low", remaining <= Math.max(1, Math.floor(state.usage.limit * 0.2)));
  } else if (usageEl) {
    usageEl.textContent = "";
  }
}

async function refreshUsage() {
  const me = await fetchMe();
  if (me.status === "ok") {
    state.usage = me.usage;
    renderSidebarUser();
  }
}

// --- Conversations (server-side, cookie auth) ---

async function fetchConversations() {
  try {
    const res = await fetch(
      `${(CFG.apiBase || "").replace(/\/$/, "")}/api/v1/conversations`,
      { credentials: "same-origin", headers: apiHeaders() }
    );
    if (!res.ok) return [];
    const d = await res.json();
    return d.conversations || [];
  } catch {
    return [];
  }
}

async function fetchMessages(convId) {
  try {
    const res = await fetch(
      `${(CFG.apiBase || "").replace(/\/$/, "")}/api/v1/conversations/${encodeURIComponent(convId)}/messages`,
      { credentials: "same-origin", headers: apiHeaders() }
    );
    if (!res.ok) return [];
    const d = await res.json();
    return d.messages || [];
  } catch {
    return [];
  }
}

async function createConversation(projectId, title) {
  const res = await fetch(
    `${(CFG.apiBase || "").replace(/\/$/, "")}/api/v1/conversations`,
    {
      method: "POST",
      headers: apiHeaders({ "Content-Type": "application/json" }),
      credentials: "same-origin",
      body: JSON.stringify({ project_id: projectId, title }),
    }
  );
  if (!res.ok) throw new Error("create conversation failed");
  return res.json();
}

async function appendMessageToServer(convId, role, content) {
  if (!convId) return;
  try {
    await fetch(
      `${(CFG.apiBase || "").replace(/\/$/, "")}/api/v1/conversations/${encodeURIComponent(convId)}/messages`,
      {
        method: "POST",
        headers: apiHeaders({ "Content-Type": "application/json" }),
        credentials: "same-origin",
        body: JSON.stringify({ role, content }),
      }
    );
  } catch {
    /* best-effort */
  }
}

async function deleteConversation(convId) {
  try {
    await fetch(
      `${(CFG.apiBase || "").replace(/\/$/, "")}/api/v1/conversations/${encodeURIComponent(convId)}`,
      { method: "DELETE", credentials: "same-origin", headers: apiHeaders() }
    );
  } catch {}
}

// --- Meta merge ---

function projectKeyFromPath(path) {
  const parts = path.replace(/\\/g, "/").split("/").filter(Boolean);
  return parts[parts.length - 1] || path;
}

function mergeProject(apiProject) {
  const key = projectKeyFromPath(apiProject.path);
  const meta = state.meta[key] || {};
  return {
    ...apiProject,
    key,
    title: meta.title || apiProject.name,
    subtitle: meta.subtitle || "",
    emoji: meta.emoji || "📚",
    theme: meta.theme || "career",
    starters: meta.starters || [],
  };
}

// --- RAG context ---

const MAX_SOURCES = 6;

async function buildContext(projectId, query) {
  try {
    const res = await apiPost(`/api/v1/projects/${encodeURIComponent(projectId)}/search`, {
      query,
      topK: 8,
      includeContent: true,
    });
    const data = await res.json();
    if (!data.results?.length) return { text: "", sources: [] };
    const results = data.results.slice(0, MAX_SOURCES);
    const text = results
      .map((r, i) => `[${i + 1}] ${r.title || r.path}\n${r.content || r.snippet || ""}`.trim())
      .join("\n\n");
    return { text, sources: results };
  } catch {
    return { text: "", sources: [] };
  }
}

// --- SSE chat ---

const STREAM_TIMEOUT_MS = 120_000;

function parseSseChunk(chunk, onToken, onReasoning) {
  for (const line of chunk.split("\n")) {
    if (!line.startsWith("data: ")) continue;
    const raw = line.slice(6).trim();
    if (!raw) continue;
    let parsed;
    try { parsed = JSON.parse(raw); } catch { continue; }
    if (parsed.event === "token" && parsed.data?.token) onToken(parsed.data.token);
    if (parsed.event === "reasoning" && parsed.data?.token) onReasoning(parsed.data.token);
    if (parsed.event === "done") return "done";
    if (parsed.event === "error") throw new Error(parsed.data?.message || "Stream error");
  }
  return null;
}

async function streamChat(projectId, messages, onToken, onReasoning, signal) {
  const res = await fetch(apiUrl(`/api/v1/projects/${encodeURIComponent(projectId)}/chat`), {
    method: "POST",
    headers: apiHeaders({ "Content-Type": "application/json", Accept: "text/event-stream" }),
    body: JSON.stringify({ messages }),
    signal,
    credentials: "same-origin",
  });
  if (!res.ok || !res.body) {
    const raw = await res.text().catch(() => "");
    let msg = raw;
    try {
      const parsed = JSON.parse(raw);
      msg = parsed?.error?.message || parsed?.error || raw;
    } catch {
      /* keep raw */
    }
    throw new Error(msg || `Chat failed (${res.status})`);
  }
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      let boundary = buffer.indexOf("\n\n");
      while (boundary !== -1) {
        const chunk = buffer.slice(0, boundary);
        buffer = buffer.slice(boundary + 2);
        if (parseSseChunk(chunk, onToken, onReasoning) === "done") return;
        boundary = buffer.indexOf("\n\n");
      }
    }
    if (buffer.trim() && parseSseChunk(buffer, onToken, onReasoning) === "done") return;
  } catch (err) {
    if (isAbortError(err)) return;
    throw err;
  }
}

// ---- Source cards & citation hover ---

let _citationCard = null;

function initCitationCard() {
  if (_citationCard) return _citationCard;
  const card = document.createElement("div");
  card.className = "citation-card";
  card.hidden = true;
  document.body.appendChild(card);
  _citationCard = card;

  // Delegate hover on #messages for .ref-badge and .source-card
  const msgs = $("#messages");
  if (msgs) {
    let hideTimer = null;
    let lastId = -1;

    function showCard(el, idx) {
      const sources = window.__lastSources || [];
      const s = sources[idx];
      if (!s) { hideCard(); return; }
      card.innerHTML = `<div class="citation-header"><span class="citation-num">[${idx + 1}]</span><span class="citation-title">${escapeHtml(s.title || s.path || "")}</span></div><div class="citation-snippet">${escapeHtml((s.snippet || s.content || "").slice(0, 200))}</div><a class="citation-link" href="/api/v1/projects/${encodeURIComponent(state.activeProject?.id || "")}/files/content?path=${encodeURIComponent(s.path || "")}" target="_blank">${escapeHtml(I18N.t("lite.sourceCard.view"))}</a>`;
      card.hidden = false;
      const rect = el.getBoundingClientRect();
      const top = rect.bottom + 6;
      let left = rect.left + rect.width / 2 - 160;
      if (left < 8) left = 8;
      if (left + 320 > window.innerWidth) left = window.innerWidth - 328;
      card.style.top = `${top + window.scrollY}px`;
      card.style.left = `${left}px`;
    }

    function hideCard() { card.hidden = true; }

    msgs.addEventListener("mouseover", (e) => {
      const badge = e.target.closest(".ref-badge");
      if (!badge) return;
      const id = parseInt(badge.dataset.id || "0", 10);
      if (id === lastId) return;
      clearTimeout(hideTimer);
      lastId = id;
      showCard(badge, id - 1);
    });

    msgs.addEventListener("mouseout", (e) => {
      const badge = e.target.closest(".ref-badge");
      if (!badge) return;
      // Delay hide to allow moving mouse to the card
      hideTimer = setTimeout(() => { hideCard(); lastId = -1; }, 250);
    });

    // Keep showing when mouse is on the card itself
    card.addEventListener("mouseenter", () => clearTimeout(hideTimer));
    card.addEventListener("mouseleave", () => { hideCard(); lastId = -1; });
  }
}

function renderSourceCards(bubble, sources) {
  if (!sources || !sources.length) return;
  window.__lastSources = sources;
  const wrap = document.createElement("div");
  wrap.className = "sources-grid";
  let html = '<div class="sources-label">' + escapeHtml(I18N.t("lite.sources.label")) + '</div>';
  for (let i = 0; i < sources.length; i++) {
    const r = sources[i];
    const title = r.title || r.path || "";
    const dir = r.path ? r.path.replace(/[^/]+\/?$/, "") : "";
    const pid = state.activeProject?.id || "";
    const path = r.path ? `/api/v1/projects/${encodeURIComponent(pid)}/files/content?path=${encodeURIComponent(r.path)}` : "#";
    html += `<a class="source-card" href="${path}" target="_blank">
      <span class="source-index">[${i + 1}]</span>
      <span class="source-info">
        <span class="source-title">${escapeHtml(title)}</span>
        ${dir ? `<span class="source-dir">${escapeHtml(dir)}</span>` : ""}
      </span>
    </a>`;
  }
  wrap.innerHTML = html;
  bubble.appendChild(wrap);
}

// --- Shared helpers ---

function escapeHtml(s) {
  return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

function renderProjectList() {
  const el = $("#project-list");
  if (!el) return;
  el.innerHTML = "";
  for (const p of state.projects) {
    const btn = document.createElement("button");
    btn.className = "sidebar-project" + (p.id === state.activeProject?.id ? " active" : "");
    btn.textContent = (p.emoji || "📁") + " " + (p.title || p.name);
    btn.addEventListener("click", () => openProject(p));
    el.appendChild(btn);
  }
}

function renderEmptyState() {
  const msgs = $("#messages");
  if (!msgs) return;
  // Only show empty state when there are no messages
  if (state.currentMessages.length > 0) return;
  msgs.innerHTML = "";
  const welcome = document.createElement("div");
  welcome.className = "empty-state";
  welcome.innerHTML = `<h2 class="empty-title">${escapeHtml(I18N.t("lite.empty.title"))}</h2>` +
    (state.activeProject?.starters?.length > 0
      ? `<div class="suggestion-list">` +
        state.activeProject.starters.map(s =>
          `<button class="suggestion-chip" data-text="${escapeHtml(s)}">${escapeHtml(s)}</button>`
        ).join("") +
        `</div>`
      : "");
  msgs.appendChild(welcome);
  welcome.querySelectorAll(".suggestion-chip").forEach(btn => {
    btn.addEventListener("click", () => sendMessage(btn.dataset.text));
  });
}

function showBanner(text) {
  const el = $("#banner-offline");
  if (!text) { el.classList.add("hidden"); return; }
  el.textContent = text;
  el.classList.remove("hidden");
}

// --- Search ---

let _lastQuery = "";

function showSearch() {
  abortActiveStream();
  showView("search");
  setTimeout(() => $("#search-input")?.focus(), 50);
}

async function doSearch(query) {
  const q = (query || "").trim();
  if (!q || !state.activeProject) return;
  _lastQuery = q;
  const el = $("#search-results");
  el.innerHTML = '<div class="search-status">' + escapeHtml(I18N.t("lite.searchStatus.searching")) + '</div>';
  try {
    const res = await apiPost(`/api/v1/projects/${encodeURIComponent(state.activeProject.id)}/search`, { query: q, topK: 30, includeContent: true });
    const data = await res.json();
    const results = data.results || [];
    if (!results.length) { el.innerHTML = '<div class="search-status search-empty">' + escapeHtml(I18N.t("lite.searchStatus.empty")) + '</div>'; return; }
    el.innerHTML = results.map((r, i) => {
      const snippet = r.snippet || r.content || "";
      const title = r.title || r.path || "";
      const pid = state.activeProject.id;
      const link = `/api/v1/projects/${encodeURIComponent(pid)}/files/content?path=${encodeURIComponent(r.path || "")}`;
      return `<div class="result-item"><h3 class="result-title"><a href="${link}" target="_blank">${escapeHtml(title)}</a></h3><p class="result-snippet">${highlightQuery(escapeHtml(snippet), q)}</p><div class="result-meta"><span class="meta-file">${escapeHtml(r.path || "")}</span></div></div>`;
    }).join("");
  } catch { el.innerHTML = '<div class="search-status search-empty">' + escapeHtml(I18N.t("lite.searchStatus.failed")) + '</div>'; }
}

function highlightQuery(text, q) {
  if (!q || !text) return text;
  const words = q.split(/\s+/).filter(Boolean).map(w => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  let result = text;
  for (const w of words) { result = result.replace(new RegExp(`(${w})`, "gi"), "<mark>$1</mark>"); }
  return result;
}

function showView(name) {
  // home view removed -- only chat and search exist
  $("#view-search").classList.toggle("active", name === "search");
  // chat view always visible as default
  $("#view-chat").classList.toggle("active", name !== "search");
}

async function openProject(project) {
  state.activeProject = project;
  state.activeMeta = project;
  document.documentElement.dataset.theme = project.theme;
  // Reset conversation for the new project
  state.conversationId = null;
  state.currentMessages = [];
  renderEmptyState();
  renderProjectList();
  renderHistoryList();
}

function normalizeMessages(messages) {
  if (!Array.isArray(messages)) return [];
  return messages.filter((m) => m && (m.role === "user" || m.role === "assistant")).map((m) => ({ role: m.role, content: String(m.content ?? ""), error: !!m.error }));
}

function renderMessages(messages, opts = {}) {
  const box = $("#messages");
  box.innerHTML = "";
  const list = normalizeMessages(messages);
  if (!list.length) {
    renderEmptyState();
    return;
  }
  for (let i = 0; i < list.length; i++) {
    const m = list[i];
    const row = document.createElement("div");
    row.className = `msg-row ${m.role}`;
    const isStreaming = opts.streaming && i === list.length - 1 && m.role === "assistant";
    if (isStreaming) row.classList.add("streaming");
    const bubble = document.createElement("div");
    bubble.className = "msg";
    if (m.error) bubble.classList.add("error");
    if (m.role === "assistant" && !m.error) {
      if (isStreaming) bubble.id = "streaming-bubble";
      if (!m.content) { bubble.classList.add("msg-placeholder"); }
      else { setAssistantBubbleContent(bubble, m.content); }
    } else { bubble.textContent = m.content; }
    if (isStreaming) {
      const stack = document.createElement("div");
      stack.className = "msg-stack";
      const reasoning = document.createElement("details");
      reasoning.className = "msg-reasoning";
      reasoning.id = "streaming-reasoning";
      reasoning.open = true;
      const summary = document.createElement("summary");
      summary.textContent = I18N.t("lite.reasoning.title");
      reasoning.appendChild(summary);
      const reasoningText = document.createElement("div");
      reasoningText.className = "msg-reasoning-text";
      reasoningText.id = "streaming-reasoning-text";
      reasoning.appendChild(reasoningText);
      stack.appendChild(reasoning);
      stack.appendChild(bubble);
      const status = document.createElement("div");
      status.className = "stream-status";
      status.id = "stream-status";
      status.setAttribute("aria-live", "polite");
      status.innerHTML = '<span class="stream-dots" aria-hidden="true"><i></i><i></i><i></i></span>' + escapeHtml(I18N.t("lite.streamStatus.replying"));
      stack.appendChild(status);
      row.appendChild(stack);
    } else { row.appendChild(bubble); }
    box.appendChild(row);
  }
  box.scrollTop = box.scrollHeight;
}

let streamRafId = 0;
let streamPendingContent = "";
let streamPendingReasoning = "";

function paintStreamingBubble() {
  streamRafId = 0;
  const bubble = document.getElementById("streaming-bubble");
  if (bubble) { bubble.classList.remove("msg-placeholder"); bubble.textContent = streamPendingContent; }
  paintStreamingReasoning();
  const box = $("#messages");
  if (box) box.scrollTop = box.scrollHeight;
}

function paintStreamingReasoning() {
  const el = document.getElementById("streaming-reasoning-text");
  if (el) el.textContent = streamPendingReasoning;
}

function scheduleStreamRender() {
  if (streamRafId) return;
  streamRafId = window.requestAnimationFrame(paintStreamingBubble);
}

function flushStreamMarkdownRender(content) {
  if (streamRafId) { window.cancelAnimationFrame(streamRafId); streamRafId = 0; }
  streamPendingContent = content;
  const el = document.getElementById("streaming-bubble");
  if (el) { el.classList.remove("msg-placeholder"); setAssistantBubbleContent(el, content); }
}

function updateStreamStatus(label) {
  const status = document.getElementById("stream-status");
  if (!status) return;
  status.innerHTML = `<span class="stream-dots" aria-hidden="true"><i></i><i></i><i></i></span>${escapeHtml(label)}`;
}

function finishReplyUI(messages, generation) {
  if (generation !== state.sendGeneration) return;
  const last = messages[messages.length - 1];
  if (last?.role === "assistant" && last.content) { flushStreamMarkdownRender(last.content); }
  state.currentMessages = messages;
  renderMessages(messages);
  setComposerStreaming(false);
  if (state.abortController) state.abortController = null;
}

function setComposerStreaming(active) {
  const input = $("#input");
  const btnSend = $("#btn-send");
  const composer = document.querySelector(".composer");
  if (active) {
    composer?.classList.add("is-streaming");
    input.placeholder = I18N.t("lite.composer.replying");
    input.disabled = true;
    btnSend.disabled = true;
  } else {
    composer?.classList.remove("is-streaming");
    input.placeholder = I18N.t("lite.composer.placeholder");
    input.disabled = false;
    btnSend.disabled = !input.value.trim() || !state.chatEnabled;
  }
}

async function renderHistoryList() {
  const list = $("#history-list");
  if (!list) return;
  list.innerHTML = "";
  if (!state.activeProject) return;
  const convs = await fetchConversations();
  const here = convs.filter((c) => c.project_id === state.activeProject.id);
  for (const c of here) {
    const li = document.createElement("li");
    li.className = "history-row";
    li.innerHTML = `<button type="button" class="history-item${c.id === state.conversationId ? " active" : ""}" data-id="${c.id}"><span class="history-item-title">${escapeHtml(c.title)}</span></button><button type="button" class="history-del" data-id="${c.id}" aria-label="${escapeHtml(I18N.t("lite.history.delete"))}">&times;</button>`;
    list.appendChild(li);
  }
  list.querySelectorAll(".history-item").forEach((b) => { b.addEventListener("click", () => selectConversation(b.dataset.id)); });
  list.querySelectorAll(".history-del").forEach((b) => { b.addEventListener("click", async () => { await deleteConversation(b.dataset.id); if (state.conversationId === b.dataset.id) { state.conversationId = null; state.currentMessages = []; renderMessages([]); } renderHistoryList(); }); });
}

async function newConversation() {
  abortActiveStream();
  state.conversationId = null;
  state.currentMessages = [];
  renderMessages([]);
  renderHistoryList();
}

async function selectConversation(id) {
  if (id === state.conversationId) return;
  abortActiveStream();
  state.conversationId = id;
  const msgs = await fetchMessages(id);
  state.currentMessages = msgs.map((m) => ({ role: m.role, content: m.content, error: false }));
  renderMessages(state.currentMessages);
  renderHistoryList();
}

function abortActiveStream() {
  if (state.abortController) { state.abortController.abort(); state.sendGeneration += 1; state.abortController = null; }
  setComposerStreaming(false);
}

async function sendMessage(text) {
  const trimmed = text.trim();
  if (!trimmed || !state.activeProject || !state.chatEnabled) return;
  if (state.usage && state.usage.used >= state.usage.limit) { alert(I18N.t("lite.chat.quotaAlert")); return; }
  if (state.abortController) state.abortController.abort();
  state.abortController = new AbortController();
  const generation = ++state.sendGeneration;
  const signal = state.abortController.signal;
  let timedOut = false;
  const timeoutId = setTimeout(() => { timedOut = true; state.abortController?.abort(); }, STREAM_TIMEOUT_MS);
  if (!state.conversationId) {
    try { const conv = await createConversation(state.activeProject.id, trimmed.slice(0, 24)); state.conversationId = conv.id; } catch {}
  }
  const messages = state.currentMessages || [];
  messages.push({ role: "user", content: trimmed });
  const assistant = { role: "assistant", content: "" };
  messages.push(assistant);
  renderMessages(messages, { streaming: true });
  setComposerStreaming(true);
  updateStreamStatus(I18N.t("lite.streamStatus.searching"));
  appendMessageToServer(state.conversationId, "user", trimmed);
  const input = $("#input");
  input.value = "";
  streamPendingContent = "";
  streamPendingReasoning = "";
  let aborted = false;
  let firstAnswerToken = true;
  try {
    const ctx = await buildContext(state.activeProject.id, trimmed);
    if (generation !== state.sendGeneration) return;
    state.lastSources = ctx.sources;
    updateStreamStatus(I18N.t("lite.streamStatus.thinking"));
    const systemParts = [I18N.tpl("lite.systemPrompt.role", { title: state.activeProject.title }), I18N.t("lite.systemPrompt.noContext")];
    if (ctx.text) {
      systemParts.push(I18N.t("lite.systemPrompt.contextLabel") + ctx.text);
      if (ctx.sources.length > 0) {
        systemParts.push(I18N.t("lite.systemPrompt.cite"));
      }
    }
    const historyForApi = messages.slice(0, -1).filter((m) => m.role === "user" || (m.role === "assistant" && m.content.trim().length > 0)).map((m) => ({ role: m.role, content: m.content }));
    const apiMessages = [{ role: "system", content: systemParts.join("\n") }, ...historyForApi];
    await streamChat(state.activeProject.id, apiMessages,
      (token) => { if (firstAnswerToken) { firstAnswerToken = false; updateStreamStatus(I18N.t("lite.streamStatus.generating")); } assistant.content += token; streamPendingContent = assistant.content; scheduleStreamRender(); },
      (token) => { streamPendingReasoning += token; scheduleStreamRender(); },
      signal);
    if (generation !== state.sendGeneration) return;
    if (!assistant.content) assistant.content = I18N.t("lite.chat.noReply");
    appendMessageToServer(state.conversationId, "assistant", assistant.content);
  } catch (err) {
    if (generation !== state.sendGeneration) return;
    if (isAbortError(err)) { aborted = true; if (timedOut && !assistant.content.trim()) { assistant.content = I18N.t("lite.chat.timeout"); assistant.error = true; } return; }
    const msg = err instanceof Error ? err.message : String(err);
    assistant.content = msg;
    assistant.error = true;
    if (/daily_limit|额度|429/i.test(msg)) { refreshUsage(); }
  } finally {
    clearTimeout(timeoutId);
    if (generation !== state.sendGeneration) return;
    if (aborted && !assistant.content.trim() && messages[messages.length - 1]?.role === "assistant") { messages.pop(); }
    finishReplyUI(messages, generation);
    // Append source cards to the last assistant message (re-render wiped them).
    if (state.lastSources?.length > 0) {
      const lastMsg = document.querySelector(".msg-row.assistant:last-child .msg");
      if (lastMsg) renderSourceCards(lastMsg, state.lastSources);
      state.lastSources = [];
    }
    refreshUsage();
    renderHistoryList();
  }
}

async function init() {
  if (!(await ensureLogin())) return;
  try {
    const [projectsRes, metaRes, runtimeRes] = await Promise.all([
      apiGet("/api/v1/projects"),
      fetch("/lite/projects.meta.json").then((r) => r.json()),
      apiGet("/api/v1/runtime-config").catch(() => ({ chatEnabled: false })),
    ]);
    state.meta = metaRes;
    state.projects = (projectsRes.projects || []).map(mergeProject);
    state.chatEnabled = runtimeRes.chatEnabled !== false;
    if (!state.chatEnabled) showBanner(I18N.t("lite.banner.chatDisabled"));
    if (!state.projects.length) showBanner(I18N.t("lite.banner.noProjects"));
    renderProjectList();
    if (state.projects.length > 0) {
      await openProject(state.projects[0]);
    }
    renderSidebarUser();
  } catch (err) { showBanner(I18N.tpl("lite.banner.connection", { msg: err instanceof Error ? err.message : err })); }
  // 语言切换时刷新动态文本（用量、占位符、空态问候等）
  document.addEventListener("i18n:changed", () => {
    renderSidebarUser();
    renderEmptyState();
    const composer = document.querySelector(".composer");
    if (composer && !composer.classList.contains("is-streaming")) {
      const inputEl = $("#input");
      if (inputEl) inputEl.placeholder = I18N.t("lite.composer.placeholder");
    }
  });
  // Sidebar actions
  $("#btn-new-chat").addEventListener("click", () => newConversation());
  $("#btn-search-sidebar")?.addEventListener("click", () => showSearch());
  $("#btn-logout-sidebar")?.addEventListener("click", async () => { try { await fetch(`${(CFG.apiBase || "").replace(/\/$/, "")}/auth/logout`, { method: "POST", credentials: "same-origin" }); } catch {} location.href = "/login"; });
  // Theme toggle: "🌓" cycles light -> dark -> system -> light
  function applyTheme(mode) {
    const html = document.documentElement;
    if (mode === "dark") { html.dataset.colorScheme = "dark"; }
    else if (mode === "light") { html.dataset.colorScheme = "light"; }
    else { html.removeAttribute("data-color-scheme"); }
  }
  const saved = localStorage.getItem("theme_scheme") || "auto";
  applyTheme(saved);
  $("#btn-theme-sidebar")?.addEventListener("click", () => {
    const cur = document.documentElement.getAttribute("data-color-scheme");
    const next = cur === "dark" ? "light" : cur === "light" ? "auto" : "dark";
    localStorage.setItem("theme_scheme", next);
    applyTheme(next);
  });
  initCitationCard();
  // Mobile menu toggle
  $("#btn-menu-mobile")?.addEventListener("click", () => {
    document.querySelector(".sidebar")?.classList.toggle("open");
    const backdrop = document.querySelector(".sidebar-backdrop");
    if (backdrop) backdrop.classList.toggle("show");
  });

  // Close sidebar when clicking a project (navigates)
  document.querySelector(".project-list")?.addEventListener("click", () => {
    document.querySelector(".sidebar")?.classList.remove("open");
    document.querySelector(".sidebar-backdrop")?.classList.remove("show");
  });

  // Also close when clicking a conversation
  document.querySelector(".history-list")?.addEventListener("click", () => {
    document.querySelector(".sidebar")?.classList.remove("open");
    document.querySelector(".sidebar-backdrop")?.classList.remove("show");
  });
  // Search view
  $("#btn-back-from-search")?.addEventListener("click", () => showView("chat"));
  $("#search-input")?.addEventListener("keydown", (e) => { if (e.key === "Enter") doSearch(e.target.value); });
  $("#search-btn")?.addEventListener("click", () => doSearch($("#search-input")?.value));
  const input = $("#input");
  const btnSend = $("#btn-send");
  input.addEventListener("input", () => { btnSend.disabled = !input.value.trim() || !state.chatEnabled; input.style.height = "auto"; input.style.height = `${Math.min(input.scrollHeight, 120)}px`; });
  input.addEventListener("keydown", (e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); if (!btnSend.disabled) sendMessage(input.value); } });
  btnSend.addEventListener("click", () => sendMessage(input.value));
}

init();
