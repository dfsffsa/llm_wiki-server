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
  currentMessages: [],
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
      return { status: "ok", user: data.user, usage: data.usage };
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
    renderTopbar();
  }
  // status === "disabled": proceed without user/quota (shared-token mode)
  return true;
}

function renderTopbar() {
  const bar = $("#topbar");
  if (!bar) return;
  // In shared-token (auth-disabled) mode there is no user; keep the topbar hidden.
  if (!state.user) {
    bar.hidden = true;
    return;
  }
  bar.hidden = false;
  $("#user-email").textContent = state.user?.display_name || state.user?.email || "";
  const info = $("#usage-info");
  if (state.usage) {
    const remaining = Math.max(0, state.usage.limit - state.usage.used);
    info.textContent = `今日剩余 ${remaining}/${state.usage.limit}`;
    info.classList.toggle("low", remaining <= Math.max(1, Math.floor(state.usage.limit * 0.2)));
  } else {
    info.textContent = "";
  }
}

async function refreshUsage() {
  const me = await fetchMe();
  if (me.status === "ok") {
    state.usage = me.usage;
    renderTopbar();
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

async function buildContext(projectId, query) {
  try {
    const res = await apiPost(`/api/v1/projects/${encodeURIComponent(projectId)}/search`, {
      query,
      topK: 8,
      includeContent: true,
    });
    const data = await res.json();
    if (!data.results?.length) return "";
    return data.results
      .slice(0, 6)
      .map((r, i) => {
        const text = r.content || r.snippet || "";
        return `[${i + 1}] ${r.title || r.path}\n${text}`.trim();
      })
      .join("\n\n");
  } catch {
    return "";
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

// --- UI ---

function escapeHtml(s) {
  return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

function renderProjectGrid() {
  const grid = $("#project-grid");
  grid.innerHTML = "";
  for (const p of state.projects) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "project-card";
    btn.dataset.theme = p.theme;
    btn.innerHTML = `<div class="card-emoji">${escapeHtml(p.emoji)}</div><h2 class="card-title">${escapeHtml(p.title)}</h2><p class="card-subtitle">${escapeHtml(p.subtitle)}</p>`;
    btn.addEventListener("click", () => openProject(p));
    grid.appendChild(btn);
  }
}

function showBanner(text) {
  const el = $("#banner-offline");
  if (!text) { el.classList.add("hidden"); return; }
  el.textContent = text;
  el.classList.remove("hidden");
}

function showView(name) {
  $("#view-home").classList.toggle("active", name === "home");
  $("#view-chat").classList.toggle("active", name === "chat");
}

async function openProject(project) {
  state.activeProject = project;
  state.activeMeta = project;
  document.documentElement.dataset.theme = project.theme;
  $("#chat-emoji").textContent = project.emoji;
  $("#chat-title").textContent = project.title;
  renderStarters(project.starters);
  const convs = await fetchConversations();
  const here = convs.filter((c) => c.project_id === project.id);
  if (here.length > 0) {
    const latest = [...here].sort((a, b) => new Date(b.updated_at) - new Date(a.updated_at))[0];
    await selectConversation(latest.id);
  } else {
    await newConversation();
  }
  showView("chat");
}

function renderStarters(starters) {
  const el = $("#starters");
  el.innerHTML = "";
  for (const text of starters) {
    const chip = document.createElement("button");
    chip.type = "button";
    chip.className = "starter-chip";
    chip.textContent = text;
    chip.addEventListener("click", () => sendMessage(text));
    el.appendChild(chip);
  }
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
    const hint = document.createElement("p");
    hint.className = "empty-hint";
    hint.textContent = "有什么想了解的？输入问题或点下方建议。";
    box.appendChild(hint);
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
      summary.textContent = "思考过程";
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
      status.innerHTML = '<span class="stream-dots" aria-hidden="true"><i></i><i></i><i></i></span>正在回复…';
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
    input.placeholder = "正在回复中，请稍候…";
    input.disabled = true;
    btnSend.disabled = true;
  } else {
    composer?.classList.remove("is-streaming");
    input.placeholder = "输入你的问题…";
    input.disabled = false;
    btnSend.disabled = !input.value.trim() || !state.chatEnabled;
  }
}

async function renderHistoryList() {
  const list = $("#history-list");
  if (!list) return;
  list.innerHTML = "";
  const convs = await fetchConversations();
  const here = convs.filter((c) => c.project_id === state.activeProject.id);
  for (const c of here) {
    const li = document.createElement("li");
    li.className = "history-row";
    li.innerHTML = `<button type="button" class="history-item${c.id === state.conversationId ? " active" : ""}" data-id="${c.id}"><span class="history-item-title">${escapeHtml(c.title)}</span></button><button type="button" class="history-del" data-id="${c.id}" aria-label="删除">&times;</button>`;
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
  if (state.usage && state.usage.used >= state.usage.limit) { alert("今日额度已用完,明日重置"); return; }
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
  updateStreamStatus("正在检索资料…");
  appendMessageToServer(state.conversationId, "user", trimmed);
  const input = $("#input");
  input.value = "";
  streamPendingContent = "";
  streamPendingReasoning = "";
  let aborted = false;
  let firstAnswerToken = true;
  try {
    const context = await buildContext(state.activeProject.id, trimmed);
    if (generation !== state.sendGeneration) return;
    updateStreamStatus("正在思考…");
    const systemParts = [`你是「${state.activeProject.title}」知识库助手，用简洁中文回答家长/职场新人的实际问题。`, "优先依据下方检索到的资料；若无相关资料，请诚实说明并给出通用建议。"];
    if (context) systemParts.push("\n--- 检索资料 ---\n" + context);
    const historyForApi = messages.slice(0, -1).filter((m) => m.role === "user" || (m.role === "assistant" && m.content.trim().length > 0)).map((m) => ({ role: m.role, content: m.content }));
    const apiMessages = [{ role: "system", content: systemParts.join("\n") }, ...historyForApi];
    await streamChat(state.activeProject.id, apiMessages,
      (token) => { if (firstAnswerToken) { firstAnswerToken = false; updateStreamStatus("正在生成回答…"); } assistant.content += token; streamPendingContent = assistant.content; scheduleStreamRender(); },
      (token) => { streamPendingReasoning += token; scheduleStreamRender(); },
      signal);
    if (generation !== state.sendGeneration) return;
    if (!assistant.content) assistant.content = "（无回复内容）";
    appendMessageToServer(state.conversationId, "assistant", assistant.content);
  } catch (err) {
    if (generation !== state.sendGeneration) return;
    if (isAbortError(err)) { aborted = true; if (timedOut && !assistant.content.trim()) { assistant.content = "回复超时，请稍后重试。"; assistant.error = true; } return; }
    const msg = err instanceof Error ? err.message : String(err);
    assistant.content = msg;
    assistant.error = true;
    if (/daily_limit|额度|429/i.test(msg)) { refreshUsage(); }
  } finally {
    clearTimeout(timeoutId);
    if (generation !== state.sendGeneration) return;
    if (aborted && !assistant.content.trim() && messages[messages.length - 1]?.role === "assistant") { messages.pop(); }
    finishReplyUI(messages, generation);
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
    if (!state.chatEnabled) showBanner("问答功能暂不可用，请检查服务端 LLM 配置。");
    if (!state.projects.length) showBanner("暂无可用知识库，请在服务端配置 projects。");
    renderProjectGrid();
  } catch (err) { showBanner(`无法连接服务：${err instanceof Error ? err.message : err}`); }
  $("#btn-back").addEventListener("click", () => { abortActiveStream(); showView("home"); });
  $("#btn-new-chat").addEventListener("click", () => newConversation());
  $("#btn-logout")?.addEventListener("click", async () => { try { await fetch(`${(CFG.apiBase || "").replace(/\/$/, "")}/auth/logout`, { method: "POST", credentials: "same-origin" }); } catch {} location.href = "/login"; });
  $("#btn-menu")?.addEventListener("click", () => { $("#history-sidebar")?.classList.toggle("open"); });
  const input = $("#input");
  const btnSend = $("#btn-send");
  input.addEventListener("input", () => { btnSend.disabled = !input.value.trim() || !state.chatEnabled; input.style.height = "auto"; input.style.height = `${Math.min(input.scrollHeight, 120)}px`; });
  input.addEventListener("keydown", (e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); if (!btnSend.disabled) sendMessage(input.value); } });
  btnSend.addEventListener("click", () => sendMessage(input.value));
}

init();
