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
  const res = await fetch(apiUrl(path), { headers: apiHeaders() });
  const body = await res.json().catch(() => ({}));
  if (!res.ok || body.ok === false) throw new Error(body.error || res.statusText);
  return body;
}

async function apiPost(path, data) {
  const res = await fetch(apiUrl(path), {
    method: "POST",
    headers: apiHeaders({ "Content-Type": "application/json" }),
    body: JSON.stringify(data),
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({}));
    throw new Error(body.error || res.statusText);
  }
  return res;
}

// --- Storage ---

function storageKey(projectId) {
  return `llm-wiki-lite:${projectId}`;
}

function loadStore(projectId) {
  try {
    const raw = localStorage.getItem(storageKey(projectId));
    if (raw) return JSON.parse(raw);
  } catch {
    /* ignore */
  }
  return { conversations: [], messages: {} };
}

function saveStore(projectId, store) {
  localStorage.setItem(storageKey(projectId), JSON.stringify(store));
}

function newConversationId() {
  return crypto.randomUUID();
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

function parseSseChunk(chunk, onToken) {
  for (const line of chunk.split("\n")) {
    if (!line.startsWith("data: ")) continue;
    const raw = line.slice(6).trim();
    if (!raw) continue;
    let parsed;
    try {
      parsed = JSON.parse(raw);
    } catch {
      continue;
    }
    if (parsed.event === "token" && parsed.data?.token) onToken(parsed.data.token);
    if (parsed.event === "done") return "done";
    if (parsed.event === "error") {
      throw new Error(parsed.data?.message || "Stream error");
    }
  }
  return null;
}

async function streamChat(projectId, messages, onToken, signal) {
  const res = await fetch(apiUrl(`/api/v1/projects/${encodeURIComponent(projectId)}/chat`), {
    method: "POST",
    headers: apiHeaders({
      "Content-Type": "application/json",
      Accept: "text/event-stream",
    }),
    body: JSON.stringify({ messages }),
    signal,
  });
  if (!res.ok || !res.body) {
    const err = await res.text().catch(() => "");
    throw new Error(err || `Chat failed (${res.status})`);
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
        if (parseSseChunk(chunk, onToken) === "done") return;
        boundary = buffer.indexOf("\n\n");
      }
    }
    if (buffer.trim() && parseSseChunk(buffer, onToken) === "done") return;
  } catch (err) {
    if (isAbortError(err)) return;
    throw err;
  }
}

// --- UI: home ---

function renderProjectGrid() {
  const grid = $("#project-grid");
  grid.innerHTML = "";
  for (const p of state.projects) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "project-card";
    btn.dataset.theme = p.theme;
    btn.innerHTML = `
      <div class="card-emoji">${escapeHtml(p.emoji)}</div>
      <h2 class="card-title">${escapeHtml(p.title)}</h2>
      <p class="card-subtitle">${escapeHtml(p.subtitle)}</p>
    `;
    btn.addEventListener("click", () => openProject(p));
    grid.appendChild(btn);
  }
}

function showBanner(text) {
  const el = $("#banner-offline");
  if (!text) {
    el.classList.add("hidden");
    return;
  }
  el.textContent = text;
  el.classList.remove("hidden");
}

// --- UI: chat ---

function showView(name) {
  $("#view-home").classList.toggle("active", name === "home");
  $("#view-chat").classList.toggle("active", name === "chat");
}

function openProject(project) {
  state.activeProject = project;
  state.activeMeta = project;
  document.documentElement.dataset.theme = project.theme;
  $("#chat-emoji").textContent = project.emoji;
  $("#chat-title").textContent = project.title;
  renderStarters(project.starters);
  const store = loadStore(project.id);
  if (store.conversations.length > 0) {
    const latest = [...store.conversations].sort((a, b) => b.updatedAt - a.updatedAt)[0];
    switchConversation(latest.id);
  } else {
    startNewConversation();
  }
  renderHistoryList();
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
    const isStreaming =
      opts.streaming && i === list.length - 1 && m.role === "assistant";
    if (isStreaming) row.classList.add("streaming");

    const bubble = document.createElement("div");
    bubble.className = "msg";
    if (m.error) bubble.classList.add("error");
    if (m.role === "assistant" && !m.error) {
      if (isStreaming) bubble.id = "streaming-bubble";
      if (!m.content) {
        bubble.classList.add("msg-placeholder");
      } else {
        setAssistantBubbleContent(bubble, m.content);
      }
    } else {
      bubble.textContent = m.content;
    }

    if (isStreaming) {
      const stack = document.createElement("div");
      stack.className = "msg-stack";
      stack.appendChild(bubble);
      const status = document.createElement("div");
      status.className = "stream-status";
      status.id = "stream-status";
      status.setAttribute("aria-live", "polite");
      status.innerHTML =
        '<span class="stream-dots" aria-hidden="true"><i></i><i></i><i></i></span>正在回复…';
      stack.appendChild(status);
      row.appendChild(stack);
    } else {
      row.appendChild(bubble);
    }
    box.appendChild(row);
  }
  box.scrollTop = box.scrollHeight;
}

function normalizeMessages(messages) {
  if (!Array.isArray(messages)) return [];
  return messages
    .filter((m) => m && (m.role === "user" || m.role === "assistant"))
    .map((m) => ({ role: m.role, content: String(m.content ?? ""), error: !!m.error }));
}

let streamMdTimer = 0;
let streamMdPending = "";

function scheduleStreamMarkdownRender(content) {
  streamMdPending = content;
  if (streamMdTimer) return;
  streamMdTimer = window.setTimeout(() => {
    streamMdTimer = 0;
    const pending = streamMdPending;
    const el = document.getElementById("streaming-bubble");
    if (el) {
      el.classList.remove("msg-placeholder");
      setAssistantBubbleContent(el, pending);
      $("#messages").scrollTop = $("#messages").scrollHeight;
    }
    if (streamMdPending !== pending) scheduleStreamMarkdownRender(streamMdPending);
  }, 120);
}

function flushStreamMarkdownRender(content) {
  if (streamMdTimer) {
    clearTimeout(streamMdTimer);
    streamMdTimer = 0;
  }
  streamMdPending = content;
  const el = document.getElementById("streaming-bubble");
  if (el) {
    el.classList.remove("msg-placeholder");
    setAssistantBubbleContent(el, content);
  }
}

function updateStreamStatus(label) {
  const status = document.getElementById("stream-status");
  if (!status) return;
  status.innerHTML = `<span class="stream-dots" aria-hidden="true"><i></i><i></i><i></i></span>${escapeHtml(label)}`;
}

function finishReplyUI(messages, generation) {
  if (generation !== state.sendGeneration) return;
  const last = messages[messages.length - 1];
  if (last?.role === "assistant" && last.content) {
    flushStreamMarkdownRender(last.content);
  }
  persistMessages(messages);
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

function renderHistoryList() {
  const list = $("#history-list");
  const store = loadStore(state.activeProject.id);
  list.innerHTML = "";
  const sorted = [...store.conversations].sort((a, b) => b.updatedAt - a.updatedAt);
  for (const c of sorted) {
    const li = document.createElement("li");
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "history-item" + (c.id === state.conversationId ? " active" : "");
    btn.innerHTML = `<span class="history-item-title">${escapeHtml(c.title)}</span>`;
    btn.addEventListener("click", () => switchConversation(c.id));
    li.appendChild(btn);
    list.appendChild(li);
  }
}

function abortActiveStream() {
  if (state.abortController) {
    state.abortController.abort();
    state.sendGeneration += 1;
    state.abortController = null;
  }
  setComposerStreaming(false);
}

function startNewConversation() {
  abortActiveStream();
  state.conversationId = newConversationId();
  const store = loadStore(state.activeProject.id);
  store.conversations.unshift({
    id: state.conversationId,
    title: "新对话",
    updatedAt: Date.now(),
  });
  store.messages[state.conversationId] = [];
  saveStore(state.activeProject.id, store);
  renderMessages([]);
  renderHistoryList();
}

function switchConversation(id) {
  if (id === state.conversationId) return;
  abortActiveStream();
  state.conversationId = id;
  const store = loadStore(state.activeProject.id);
  renderMessages(store.messages[id] || []);
  renderHistoryList();
}

function getMessages() {
  const store = loadStore(state.activeProject.id);
  const raw = store.messages[state.conversationId] || [];
  return raw.map((m) => ({ role: m.role, content: m.content, error: m.error }));
}

function persistMessages(messages) {
  const store = loadStore(state.activeProject.id);
  store.messages[state.conversationId] = messages;
  const conv = store.conversations.find((c) => c.id === state.conversationId);
  if (conv) {
    conv.updatedAt = Date.now();
    const firstUser = messages.find((m) => m.role === "user");
    if (firstUser) conv.title = firstUser.content.slice(0, 24) || "新对话";
  }
  if (store.conversations.length > 50) {
    const removed = store.conversations.splice(50);
    for (const c of removed) delete store.messages[c.id];
  }
  saveStore(state.activeProject.id, store);
  renderHistoryList();
}

async function sendMessage(text) {
  const trimmed = text.trim();
  if (!trimmed || !state.activeProject || !state.chatEnabled) return;

  if (state.abortController) state.abortController.abort();
  state.abortController = new AbortController();
  const generation = ++state.sendGeneration;
  const signal = state.abortController.signal;
  let timedOut = false;
  const timeoutId = setTimeout(() => {
    timedOut = true;
    state.abortController?.abort();
  }, STREAM_TIMEOUT_MS);

  const messages = getMessages();
  messages.push({ role: "user", content: trimmed });
  const assistant = { role: "assistant", content: "" };
  messages.push(assistant);
  persistMessages(messages);
  renderMessages(messages, { streaming: true });
  setComposerStreaming(true);
  updateStreamStatus("正在检索资料…");

  const input = $("#input");
  input.value = "";

  let aborted = false;
  try {
    const context = await buildContext(state.activeProject.id, trimmed);
    if (generation !== state.sendGeneration) return;

    updateStreamStatus("正在生成回答…");
    const systemParts = [
      `你是「${state.activeProject.title}」知识库助手，用简洁中文回答家长/职场新人的实际问题。`,
      "优先依据下方检索到的资料；若无相关资料，请诚实说明并给出通用建议。",
    ];
    if (context) systemParts.push("\n--- 检索资料 ---\n" + context);

    const historyForApi = messages
      .slice(0, -1)
      .filter(
        (m) =>
          m.role === "user" ||
          (m.role === "assistant" && m.content.trim().length > 0),
      )
      .map((m) => ({ role: m.role, content: m.content }));
    const apiMessages = [
      { role: "system", content: systemParts.join("\n") },
      ...historyForApi,
    ];

    await streamChat(
      state.activeProject.id,
      apiMessages,
      (token) => {
        assistant.content += token;
        scheduleStreamMarkdownRender(assistant.content);
      },
      signal,
    );
    if (generation !== state.sendGeneration) return;
    if (!assistant.content) assistant.content = "（无回复内容）";
  } catch (err) {
    if (generation !== state.sendGeneration) return;
    if (isAbortError(err)) {
      aborted = true;
      if (timedOut && !assistant.content.trim()) {
        assistant.content = "回复超时，请稍后重试。";
        assistant.error = true;
      }
      return;
    }
    const msg = err instanceof Error ? err.message : String(err);
    assistant.content = msg;
    assistant.error = true;
  } finally {
    clearTimeout(timeoutId);
    if (generation !== state.sendGeneration) return;
    if (aborted && !assistant.content.trim() && messages[messages.length - 1]?.role === "assistant") {
      messages.pop();
    }
    finishReplyUI(messages, generation);
  }
}

function escapeHtml(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

// --- Init ---

async function init() {
  try {
    const [projectsRes, metaRes, runtimeRes] = await Promise.all([
      apiGet("/api/v1/projects"),
      fetch("/lite/projects.meta.json").then((r) => r.json()),
      apiGet("/api/v1/runtime-config").catch(() => ({ chatEnabled: false })),
    ]);
    state.meta = metaRes;
    state.projects = (projectsRes.projects || []).map(mergeProject);
    state.chatEnabled = runtimeRes.chatEnabled !== false;
    if (!state.chatEnabled) {
      showBanner("问答功能暂不可用，请检查服务端 LLM 配置。");
    }
    if (!state.projects.length) {
      showBanner("暂无可用知识库，请在服务端配置 projects。");
    }
    renderProjectGrid();
  } catch (err) {
    showBanner(`无法连接服务：${err instanceof Error ? err.message : err}`);
  }

  $("#btn-back").addEventListener("click", () => {
    abortActiveStream();
    showView("home");
  });

  $("#btn-new-chat").addEventListener("click", () => startNewConversation());

  const input = $("#input");
  const btnSend = $("#btn-send");

  input.addEventListener("input", () => {
    btnSend.disabled = !input.value.trim() || !state.chatEnabled;
    input.style.height = "auto";
    input.style.height = `${Math.min(input.scrollHeight, 120)}px`;
  });

  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      if (!btnSend.disabled) sendMessage(input.value);
    }
  });

  btnSend.addEventListener("click", () => sendMessage(input.value));
}

init();
