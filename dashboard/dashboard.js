const STORAGE_KEYS = {
  apiUrl: "ghostteam.dashboard.apiUrl",
  apiKey: "ghostteam.dashboard.apiKey",
  focusAgent: "ghostteam.dashboard.focusAgent",
  logsAgent: "ghostteam.dashboard.logsAgent",
  autoRefresh: "ghostteam.dashboard.autoRefresh",
};

const state = {
  apiUrl: localStorage.getItem(STORAGE_KEYS.apiUrl) || window.location.origin,
  apiKey: localStorage.getItem(STORAGE_KEYS.apiKey) || "",
  focusAgent: localStorage.getItem(STORAGE_KEYS.focusAgent) || "",
  logsAgent: localStorage.getItem(STORAGE_KEYS.logsAgent) || "",
  autoRefresh: localStorage.getItem(STORAGE_KEYS.autoRefresh) !== "false",
  agents: [],
  tasks: [],
  messages: [],
  selectedTaskId: null,
  selectedTaskDetail: null,
  selectedAgentId: null,
  chartFilters: {
    taskStatus: "",
    taskAssignee: "",
    messageSender: "",
  },
  connected: false,
  lastError: "",
};

const refs = {};
let refreshTimer = null;
let logsSocket = null;
let logsReconnectTimer = null;

function $(id) {
  return document.getElementById(id);
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function normalizeBaseUrl(url) {
  const trimmed = String(url || "").trim();
  if (!trimmed) {
    return window.location.origin;
  }
  return trimmed.endsWith("/") ? trimmed.slice(0, -1) : trimmed;
}

function buildUrl(path) {
  const base = `${normalizeBaseUrl(state.apiUrl)}/`;
  const suffix = path.startsWith("/") ? path.slice(1) : path;
  return new URL(suffix, base);
}

async function api(path, options = {}) {
  const headers = new Headers(options.headers || {});
  if (state.apiKey) {
    headers.set("X-GhostTeam-Key", state.apiKey);
  }

  const init = { ...options, headers };
  if (init.body && typeof init.body === "object" && !(init.body instanceof FormData) && !(init.body instanceof Blob)) {
    headers.set("Content-Type", "application/json");
    init.body = JSON.stringify(init.body);
  }

  const response = await fetch(buildUrl(path), init);
  const text = await response.text();
  let payload = null;
  if (text) {
    try {
      payload = JSON.parse(text);
    } catch {
      payload = text;
    }
  }

  if (!response.ok) {
    const message = payload && typeof payload === "object" && payload.error ? payload.error : response.statusText;
    throw new Error(message);
  }

  if (payload && typeof payload === "object" && Object.prototype.hasOwnProperty.call(payload, "data")) {
    return payload.data;
  }

  return payload;
}

function formatDate(value) {
  if (!value) {
    return "n/a";
  }
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function formatCountLabel(count, total) {
  if (!total) {
    return "0";
  }
  const pct = Math.round((count / total) * 100);
  return `${count} (${pct}%)`;
}

function normalizeFilterValue(value) {
  return value ? String(value) : "";
}

function filteredTasks() {
  return state.tasks.filter((task) => {
    if (state.chartFilters.taskStatus && String(task.status || "") !== state.chartFilters.taskStatus) {
      return false;
    }
    if (state.chartFilters.taskAssignee && String(task.assignee || "unassigned") !== state.chartFilters.taskAssignee) {
      return false;
    }
    return true;
  });
}

function filteredMessages() {
  return state.messages.filter((message) => {
    if (state.chartFilters.messageSender && String(message.sender || "unknown") !== state.chartFilters.messageSender) {
      return false;
    }
    return true;
  });
}

function setChartFilter(kind, value) {
  const current = normalizeFilterValue(value);
  if (kind === "taskStatus") {
    state.chartFilters.taskStatus = state.chartFilters.taskStatus === current ? "" : current;
  } else if (kind === "taskAssignee") {
    state.chartFilters.taskAssignee = state.chartFilters.taskAssignee === current ? "" : current;
  } else if (kind === "messageSender") {
    state.chartFilters.messageSender = state.chartFilters.messageSender === current ? "" : current;
  }
}

function clearChartFilters() {
  state.chartFilters.taskStatus = "";
  state.chartFilters.taskAssignee = "";
  state.chartFilters.messageSender = "";
}

function activeFilterEntries() {
  const entries = [];
  if (state.chartFilters.taskStatus) {
    entries.push({ kind: "taskStatus", label: `Task status: ${state.chartFilters.taskStatus}` });
  }
  if (state.chartFilters.taskAssignee) {
    entries.push({ kind: "taskAssignee", label: `Assignee: ${state.chartFilters.taskAssignee}` });
  }
  if (state.chartFilters.messageSender) {
    entries.push({ kind: "messageSender", label: `Message sender: ${state.chartFilters.messageSender}` });
  }
  return entries;
}

function setStatus(connected, detail = "") {
  state.connected = connected;
  refs.connectionStatus.textContent = connected ? "Connected" : "Disconnected";
  refs.connectionStatus.classList.toggle("success", connected);
  refs.connectionStatus.classList.toggle("danger", !connected);
  if (detail) {
    refs.lastRefresh.textContent = detail;
  }
}

function setError(message) {
  state.lastError = message || "";
  if (message) {
    refs.metricLogs.textContent = "Error";
    refs.metricLogs.classList.add("danger");
  } else {
    refs.metricLogs.classList.remove("danger");
  }
}

function saveSettings() {
  localStorage.setItem(STORAGE_KEYS.apiUrl, normalizeBaseUrl(state.apiUrl));
  localStorage.setItem(STORAGE_KEYS.apiKey, state.apiKey);
  localStorage.setItem(STORAGE_KEYS.focusAgent, state.focusAgent);
  localStorage.setItem(STORAGE_KEYS.logsAgent, state.logsAgent);
  localStorage.setItem(STORAGE_KEYS.autoRefresh, String(state.autoRefresh));
}

function syncInputs() {
  refs.apiUrl.value = normalizeBaseUrl(state.apiUrl);
  refs.apiKey.value = state.apiKey;
  refs.autoRefresh.checked = state.autoRefresh;
}

function syncSelect(select, value, options, placeholder) {
  const entries = Array.isArray(options) ? options : [];
  const normalized = value && entries.some((entry) => entry.id === value) ? value : "";

  select.innerHTML = "";
  if (placeholder) {
    const placeholderOption = document.createElement("option");
    placeholderOption.value = "";
    placeholderOption.textContent = placeholder;
    select.appendChild(placeholderOption);
  }

  for (const entry of entries) {
    const option = document.createElement("option");
    option.value = entry.id;
    option.textContent = `${entry.id} - ${entry.role || entry.status || "agent"}`;
    select.appendChild(option);
  }

  select.value = normalized;
}

function countBy(items, accessor) {
  const counts = new Map();
  for (const item of items) {
    const key = accessor(item) || "unknown";
    counts.set(key, (counts.get(key) || 0) + 1);
  }
  return counts;
}

function toChartItems(counts, palette) {
  const total = Array.from(counts.values()).reduce((sum, value) => sum + value, 0);
  return Array.from(counts.entries())
    .sort((left, right) => right[1] - left[1])
    .map(([label, count], index) => ({
      label,
      count,
      total,
      color: palette[index % palette.length],
    }));
}

function renderBarChart(container, items, emptyText, footnote, kind) {
  if (!container) {
    return;
  }

  if (!items.length) {
    container.innerHTML = `<div class="chart-empty">${escapeHtml(emptyText)}</div>`;
    return;
  }

  container.innerHTML = `
    <div class="chart-bars">
      ${items
        .map(
          (item) => `
            <button
              type="button"
              class="chart-button ${item.active ? "selected" : ""}"
              data-chart-kind="${kind}"
              data-chart-value="${escapeHtml(item.label)}"
              aria-pressed="${item.active ? "true" : "false"}"
            >
              <div class="chart-row">
              <div class="chart-label">
                <strong>${escapeHtml(item.label)}</strong>
                <span>${escapeHtml(formatCountLabel(item.count, item.total))}</span>
              </div>
              <div class="chart-track">
                <div class="chart-fill" style="width:${item.total ? (item.count / item.total) * 100 : 0}%; background:${item.color};"></div>
              </div>
              </div>
            </button>
          `
        )
        .join("")}
    </div>
    ${footnote ? `<div class="chart-footnote">${escapeHtml(footnote)}</div>` : ""}
    <div class="chart-click-hint">Click a bar to toggle a filter.</div>
  `;
}

function renderCharts() {
  const statusBuckets = new Map([
    ["created", 0],
    ["acked", 0],
    ["completed", 0],
    ["requeued", 0],
    ["other", 0],
  ]);

  for (const task of state.tasks) {
    const key = String(task.status || "other").toLowerCase();
    if (statusBuckets.has(key)) {
      statusBuckets.set(key, statusBuckets.get(key) + 1);
    } else {
      statusBuckets.set("other", statusBuckets.get("other") + 1);
    }
  }

  const statusItems = toChartItems(statusBuckets, [
    "linear-gradient(90deg, #8fd3ff, #8ca9ff)",
    "linear-gradient(90deg, #f8b26a, #ffd58a)",
    "linear-gradient(90deg, #74e8a3, #39d98a)",
    "linear-gradient(90deg, #ff7e7e, #ffb08f)",
    "linear-gradient(90deg, #bca7ff, #8fd3ff)",
  ]);

  const assigneeCounts = countBy(state.tasks, (task) => task.assignee || "unassigned");
  const assigneeItems = toChartItems(assigneeCounts, [
    "linear-gradient(90deg, #8fd3ff, #64e2ff)",
    "linear-gradient(90deg, #f8b26a, #fdd97d)",
    "linear-gradient(90deg, #74e8a3, #9df0c4)",
    "linear-gradient(90deg, #bca7ff, #8fd3ff)",
  ]);

  const senderCounts = countBy(state.messages, (message) => message.sender || "unknown");
  const senderItems = toChartItems(senderCounts, [
    "linear-gradient(90deg, #8fd3ff, #8ca9ff)",
    "linear-gradient(90deg, #f8b26a, #ffd58a)",
    "linear-gradient(90deg, #74e8a3, #39d98a)",
    "linear-gradient(90deg, #ff7e7e, #ffb08f)",
  ]);

  for (const item of statusItems) {
    item.active = state.chartFilters.taskStatus === item.label;
  }
  for (const item of assigneeItems) {
    item.active = state.chartFilters.taskAssignee === item.label;
  }
  for (const item of senderItems) {
    item.active = state.chartFilters.messageSender === item.label;
  }

  renderBarChart(
    refs.taskStatusChart,
    statusItems,
    "No task status data yet.",
    `${statusItems.reduce((sum, item) => sum + item.count, 0)} tasks in the workspace`,
    "taskStatus"
  );
  renderBarChart(
    refs.taskLoadChart,
    assigneeItems,
    "No assignment data yet.",
    "Task load is based on the current assignee field",
    "taskAssignee"
  );
  renderBarChart(
    refs.messageSourceChart,
    senderItems,
    "No unread messages for the selected agent.",
    "Inbox source counts reflect unread messages only",
    "messageSender"
  );
}

function renderActiveFilters() {
  if (!refs.activeFilters) {
    return;
  }

  const entries = activeFilterEntries();
  if (!entries.length) {
    refs.activeFilters.innerHTML = '<div class="detail-empty">No active filters. Use the charts to focus the workspace.</div>';
    return;
  }

  refs.activeFilters.innerHTML = entries
    .map(
      (entry) => `
        <span class="filter-chip">
          <span>${escapeHtml(entry.label)}</span>
          <button type="button" data-clear-filter="${escapeHtml(entry.kind)}" aria-label="Clear ${escapeHtml(entry.label)}">×</button>
        </span>
      `
    )
    .join("");
}

function renderAgents() {
  const list = refs.agentsList;
  list.innerHTML = "";

  if (!state.agents.length) {
    list.innerHTML = '<div class="detail-empty">No agents registered yet.</div>';
    refs.agentDetail.innerHTML = '<div class="detail-empty">Select an agent to inspect its profile.</div>';
    return;
  }

  for (const agent of state.agents) {
    const selected = agent.id === state.focusAgent;
    const row = document.createElement("article");
    row.className = `item ${selected ? "selected" : ""}`;
    row.innerHTML = `
      <div class="item-row">
        <div>
          <div class="item-title">${escapeHtml(agent.id)}</div>
          <div class="item-meta">
            ${escapeHtml(agent.role)} - ${escapeHtml(agent.backend)}<br />
            Joined ${escapeHtml(formatDate(agent.joined_at))}
          </div>
        </div>
        <span class="pill ${selected ? "success" : ""}">${selected ? "Focused" : "Ready"}</span>
      </div>
      <div class="agent-actions">
        <button type="button" class="secondary" data-action="focus-agent" data-id="${escapeHtml(agent.id)}">Focus</button>
        <button type="button" class="secondary" data-action="leave-agent" data-id="${escapeHtml(agent.id)}">Leave</button>
        <button type="button" class="secondary" data-action="inspect-agent" data-id="${escapeHtml(agent.id)}">Inspect</button>
      </div>
    `;
    list.appendChild(row);
  }
}

function renderMessages() {
  const list = refs.messagesList;
  const messages = filteredMessages();
  list.innerHTML = "";

  if (!messages.length) {
    list.innerHTML = state.chartFilters.messageSender
      ? '<div class="detail-empty">No unread messages match the active chart filter.</div>'
      : '<div class="detail-empty">No unread messages for the selected agent.</div>';
    return;
  }

  for (const message of messages) {
    const item = document.createElement("article");
    item.className = "item";
    item.innerHTML = `
      <div class="item-row">
        <div>
          <div class="item-title">${escapeHtml(message.sender)} -> ${escapeHtml(message.recipient)}</div>
          <div class="item-meta">${escapeHtml(message.body)}</div>
        </div>
        <span class="pill warning">Unread</span>
      </div>
      <div class="item-meta">Created ${escapeHtml(formatDate(message.created_at))}</div>
      <div class="agent-actions">
        <button type="button" class="secondary" data-action="mark-read" data-id="${message.id}">Mark read</button>
      </div>
    `;
    list.appendChild(item);
  }
}

function renderTaskDetail(detail) {
  if (!detail) {
    state.selectedTaskDetail = null;
    refs.taskInspectorSummary.textContent = "Select a task to inspect history and transitions.";
    refs.taskSnapshot.innerHTML = '<div class="detail-empty">Select a task to inspect details.</div>';
    refs.taskTimeline.innerHTML = "";
    refs.taskRaw.textContent = "Select a task to inspect raw JSON.";
    return;
  }

  state.selectedTaskDetail = detail;
  const history = Array.isArray(detail.history) ? detail.history : [];

  refs.taskInspectorSummary.textContent = `Task #${detail.task.id} is ${detail.task.status} with ${history.length} history event${history.length === 1 ? "" : "s"}.`;
  refs.taskSnapshot.innerHTML = `
    <div class="item-title">#${detail.task.id} - ${escapeHtml(detail.task.description)}</div>
    <div class="task-summary-grid">
      <div class="task-summary-card">
        <span>Status</span>
        <strong>${escapeHtml(detail.task.status)}</strong>
      </div>
      <div class="task-summary-card">
        <span>Assignee</span>
        <strong>${escapeHtml(detail.task.assignee || "unassigned")}</strong>
      </div>
      <div class="task-summary-card">
        <span>Creator</span>
        <strong>${escapeHtml(detail.task.creator)}</strong>
      </div>
      <div class="task-summary-card">
        <span>Result</span>
        <strong>${escapeHtml(detail.task.result || "pending")}</strong>
      </div>
      <div class="task-summary-card">
        <span>Created</span>
        <strong>${escapeHtml(formatDate(detail.task.created_at))}</strong>
      </div>
      <div class="task-summary-card">
        <span>Updated</span>
        <strong>${escapeHtml(formatDate(detail.task.updated_at))}</strong>
      </div>
    </div>
  `;

  refs.taskTimeline.innerHTML = history.length
    ? history
        .map(
          (entry, index) => `
            <article class="task-timeline-item">
              <div class="timeline-top">
                <strong>${escapeHtml(entry.event)}</strong>
                <span class="pill">#${index + 1}</span>
              </div>
              <div class="timeline-meta">${escapeHtml(entry.actor)} - ${escapeHtml(formatDate(entry.at))}</div>
            </article>
          `
        )
        .join("")
    : '<div class="detail-empty">No task history yet.</div>';

  refs.taskRaw.textContent = JSON.stringify(detail, null, 2);
}

function renderTasks() {
  const list = refs.tasksList;
  const tasks = filteredTasks();
  list.innerHTML = "";

  if (!tasks.length) {
    const hasFilter = Boolean(state.chartFilters.taskStatus || state.chartFilters.taskAssignee);
    list.innerHTML = hasFilter
      ? '<div class="detail-empty">No tasks match the active chart filters.</div>'
      : '<div class="detail-empty">No tasks recorded yet.</div>';
    if (!state.selectedTaskDetail) {
      renderTaskDetail(null);
    }
    return;
  }

  for (const task of tasks) {
    const selected = Number(task.id) === Number(state.selectedTaskId);
    const item = document.createElement("article");
    item.className = `item ${selected ? "selected" : ""}`;
    item.innerHTML = `
      <div class="item-row">
        <div>
          <div class="item-title">#${task.id} - ${escapeHtml(task.description)}</div>
          <div class="item-meta">
            ${escapeHtml(task.creator)} -> ${escapeHtml(task.assignee || "unassigned")}<br />
            Created ${escapeHtml(formatDate(task.created_at))}
          </div>
        </div>
        <span class="pill ${task.status === "completed" ? "success" : task.status === "requeued" ? "danger" : task.status === "acked" ? "warning" : ""}">${escapeHtml(task.status)}</span>
      </div>
      <div class="item-meta">Result: ${escapeHtml(task.result || "pending")}</div>
      <div class="task-item-actions">
        <button type="button" class="secondary" data-action="inspect-task" data-id="${task.id}">Inspect</button>
      </div>
    `;
    list.appendChild(item);
  }
}

function renderAgentDetail(agent) {
  if (!agent) {
    refs.agentDetail.innerHTML = '<div class="detail-empty">Select an agent to inspect its profile.</div>';
    return;
  }

  refs.agentDetail.innerHTML = `
    <div class="item-title">${escapeHtml(agent.id)}</div>
    <div class="item-meta">
      Role: ${escapeHtml(agent.role)}<br />
      Backend: ${escapeHtml(agent.backend)}<br />
      Joined: ${escapeHtml(formatDate(agent.joined_at))}
    </div>
  `;
}

function updateSelectors() {
  syncSelect(refs.focusAgent, state.focusAgent, state.agents, "Choose focus agent");
  syncSelect(refs.messageFrom, state.focusAgent || refs.messageFrom.value, state.agents, "from");
  syncSelect(refs.messageTo, refs.messageTo.value, state.agents, "to");
  syncSelect(refs.taskFrom, state.focusAgent || refs.taskFrom.value, state.agents, "from");
  syncSelect(refs.taskTo, refs.taskTo.value, state.agents, "to");
  syncSelect(refs.logAgent, state.logsAgent || state.focusAgent, state.agents, "Choose log agent");
}

function updateMetrics() {
  refs.metricAgents.textContent = String(state.agents.length);
  refs.metricTasks.textContent = String(state.tasks.length);
  refs.metricUnread.textContent = String(state.messages.length);
  refs.metricLogs.textContent = logsSocket && logsSocket.readyState === WebSocket.OPEN ? "Streaming" : "Idle";
}

function chooseDefaultAgents() {
  if (!state.focusAgent && state.agents.length) {
    state.focusAgent = state.agents[0].id;
  }
  if (!state.logsAgent && state.focusAgent) {
    state.logsAgent = state.focusAgent;
  }
}

async function refreshDetailPanels() {
  const focusAgent = state.focusAgent;
  if (focusAgent) {
    try {
      const agent = await api(`/agents/${encodeURIComponent(focusAgent)}`);
      state.selectedAgentId = agent.id;
      renderAgentDetail(agent);
    } catch (error) {
      refs.agentDetail.innerHTML = `<div class="detail-empty">${escapeHtml(error.message)}</div>`;
    }

    try {
      state.messages = await api(`/messages/${encodeURIComponent(focusAgent)}`);
    } catch (error) {
      state.messages = [];
      setError(error.message);
    }
  } else {
    state.messages = [];
    refs.agentDetail.innerHTML = '<div class="detail-empty">Select an agent to inspect its profile.</div>';
  }

  if (state.selectedTaskId != null) {
    try {
      const task = await api(`/tasks/${state.selectedTaskId}`);
      renderTaskDetail(task);
    } catch (error) {
      refs.taskSnapshot.innerHTML = `<div class="detail-empty">${escapeHtml(error.message)}</div>`;
      refs.taskTimeline.innerHTML = "";
      refs.taskRaw.textContent = error.message;
    }
  } else {
    renderTaskDetail(null);
  }
}

function wsBaseUrl() {
  const url = new URL(normalizeBaseUrl(state.apiUrl));
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  return url;
}

function connectLogs(agent) {
  if (logsReconnectTimer) {
    clearTimeout(logsReconnectTimer);
    logsReconnectTimer = null;
  }

  if (logsSocket) {
    logsSocket.close();
    logsSocket = null;
  }

  if (!agent) {
    refs.logOutput.textContent = "Select an agent to begin tailing logs.";
    refs.metricLogs.textContent = "Idle";
    return;
  }

  const url = wsBaseUrl();
  url.pathname = `/logs/${encodeURIComponent(agent)}/stream`;
  if (state.apiKey) {
    url.searchParams.set("api_key", state.apiKey);
  }

  refs.logOutput.textContent = `Connecting to ${agent} logs...`;
  const socket = new WebSocket(url.toString());
  logsSocket = socket;

  socket.addEventListener("open", () => {
    refs.metricLogs.textContent = "Streaming";
    refs.logOutput.textContent = `Streaming logs for ${agent}\n`;
  });

  socket.addEventListener("message", (event) => {
    refs.logOutput.textContent += `${event.data}\n`;
    refs.logOutput.scrollTop = refs.logOutput.scrollHeight;
  });

  socket.addEventListener("close", () => {
    refs.metricLogs.textContent = "Idle";
    if (state.logsAgent === agent) {
      logsReconnectTimer = window.setTimeout(() => connectLogs(agent), 2500);
    }
  });

  socket.addEventListener("error", () => {
    refs.metricLogs.textContent = "Error";
  });
}

async function refreshDashboard() {
  try {
    setError("");
    setStatus(false, "Refreshing...");

    const [agents, tasks] = await Promise.all([api("/agents"), api("/tasks")]);
    state.agents = Array.isArray(agents) ? agents : [];
    state.tasks = Array.isArray(tasks) ? tasks : [];

    chooseDefaultAgents();
    if (state.focusAgent && !state.agents.some((agent) => agent.id === state.focusAgent)) {
      state.focusAgent = state.agents[0]?.id || "";
    }
    if (state.logsAgent && !state.agents.some((agent) => agent.id === state.logsAgent)) {
      state.logsAgent = state.focusAgent || "";
    }

    updateSelectors();
    renderAgents();
  renderTasks();
  renderCharts();
  await refreshDetailPanels();
  renderMessages();
  renderCharts();
  renderActiveFilters();

  updateMetrics();
  setStatus(true, `Updated ${new Date().toLocaleTimeString()}`);
  saveSettings();

    if (state.logsAgent) {
      connectLogs(state.logsAgent);
    }
  } catch (error) {
    setStatus(false, error.message || "Disconnected");
    setError(error.message || "Dashboard refresh failed");
  } finally {
    updateMetrics();
  }
}

async function handleJoin(event) {
  event.preventDefault();
  const payload = {
    id: refs.joinId.value.trim(),
    role: refs.joinRole.value.trim() || "worker",
    backend: refs.joinBackend.value.trim() || "ollama",
  };

  if (!payload.id) {
    return;
  }

  await api("/agents/join", { method: "POST", body: payload });
  refs.joinId.value = "";
  await refreshDashboard();
}

async function handleLeave(agentId) {
  if (!agentId) {
    return;
  }

  await api("/agents/leave", { method: "POST", body: { id: agentId } });
  if (state.focusAgent === agentId) {
    state.focusAgent = "";
  }
  if (state.logsAgent === agentId) {
    state.logsAgent = "";
  }
  await refreshDashboard();
}

async function handleSendMessage(event) {
  event.preventDefault();
  const payload = {
    from: refs.messageFrom.value.trim(),
    to: refs.messageTo.value.trim(),
    body: refs.messageBody.value.trim(),
  };

  if (!payload.from || !payload.to || !payload.body) {
    return;
  }

  await api("/messages/send", { method: "POST", body: payload });
  refs.messageBody.value = "";
  await refreshDashboard();
}

async function handleMarkRead(messageId) {
  await api("/messages/mark-read", { method: "POST", body: { id: messageId } });
  await refreshDashboard();
}

async function handleTaskCreate(event) {
  event.preventDefault();
  const payload = {
    from: refs.taskFrom.value.trim(),
    to: refs.taskTo.value.trim(),
    description: refs.taskDescription.value.trim(),
  };

  if (!payload.from || !payload.to || !payload.description) {
    return;
  }

  const created = await api("/tasks/create", { method: "POST", body: payload });
  refs.taskDescription.value = "";
  if (created && created.task) {
    state.selectedTaskId = created.task.id;
    renderTaskDetail(created);
  } else if (created && created.id) {
    state.selectedTaskId = created.id;
  }
  await refreshDashboard();
}

async function handleTaskAck(taskId, worker) {
  await api("/tasks/ack", { method: "POST", body: { id: taskId, worker } });
  await refreshDashboard();
}

async function handleTaskComplete(taskId, worker, result) {
  await api("/tasks/complete", { method: "POST", body: { id: taskId, worker, result } });
  await refreshDashboard();
}

async function handleTaskRequeue(taskId) {
  await api("/tasks/requeue", { method: "POST", body: { id: taskId } });
  await refreshDashboard();
}

async function handleTaskSelect(taskId) {
  state.selectedTaskId = Number(taskId);
  const detail = await api(`/tasks/${taskId}`);
  renderTaskDetail(detail);
  renderTasks();
}

async function handleTaskRefresh() {
  if (state.selectedTaskId != null) {
    const detail = await api(`/tasks/${state.selectedTaskId}`);
    renderTaskDetail(detail);
  }
}

async function handleTaskCopy() {
  if (!state.selectedTaskDetail) {
    return;
  }
  await navigator.clipboard.writeText(JSON.stringify(state.selectedTaskDetail, null, 2));
}

async function handleAgentSelect(agentId) {
  state.focusAgent = agentId;
  state.logsAgent = agentId;
  updateSelectors();
  renderAgents();
  renderMessages();
  connectLogs(agentId);
  await refreshDetailPanels();
  saveSettings();
}

async function handleGhostOsInfer(event) {
  event.preventDefault();
  const prompt = refs.ghostosPrompt.value.trim();
  if (!prompt) {
    return;
  }

  refs.ghostosOutput.textContent = "Running GhostOS inference...";
  try {
    const result = await api("/ghostos/infer", { method: "POST", body: { prompt } });
    refs.ghostosOutput.textContent = result.output || JSON.stringify(result, null, 2);
  } catch (error) {
    refs.ghostosOutput.textContent = `GhostOS error: ${error.message}`;
  }
}

function bindEvents() {
  refs.settingsForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    state.apiUrl = normalizeBaseUrl(refs.apiUrl.value);
    state.apiKey = refs.apiKey.value.trim();
    state.autoRefresh = refs.autoRefresh.checked;
    state.focusAgent = refs.focusAgent.value;
    state.logsAgent = refs.logAgent.value || state.focusAgent;
    saveSettings();
    await refreshDashboard();
  });

  refs.refreshNow.addEventListener("click", refreshDashboard);
  refs.autoRefresh.addEventListener("change", () => {
    state.autoRefresh = refs.autoRefresh.checked;
    saveSettings();
    scheduleAutoRefresh();
  });

  refs.joinForm.addEventListener("submit", handleJoin);
  refs.messageForm.addEventListener("submit", handleSendMessage);
  refs.taskForm.addEventListener("submit", handleTaskCreate);
  refs.ghostosForm.addEventListener("submit", handleGhostOsInfer);

  refs.leaveSelected.addEventListener("click", async () => {
    if (state.focusAgent) {
      await handleLeave(state.focusAgent);
    }
  });

  refs.clearLogs.addEventListener("click", () => {
    refs.logOutput.textContent = "";
  });

  refs.clearChartFilters.addEventListener("click", () => {
    clearChartFilters();
    renderCharts();
    renderTasks();
    renderMessages();
    renderActiveFilters();
  });

  for (const chart of [refs.taskStatusChart, refs.taskLoadChart, refs.messageSourceChart]) {
    chart.addEventListener("click", (event) => {
      const target = event.target instanceof Element ? event.target.closest(".chart-button") : null;
      if (!target) {
        return;
      }

      setChartFilter(target.dataset.chartKind, target.dataset.chartValue);
      renderCharts();
      renderTasks();
      renderMessages();
      renderActiveFilters();
    });
  }

  refs.activeFilters.addEventListener("click", (event) => {
    const target = event.target instanceof Element ? event.target.closest("[data-clear-filter]") : null;
    if (!target) {
      return;
    }

    const kind = target.dataset.clearFilter;
    if (kind === "taskStatus") {
      state.chartFilters.taskStatus = "";
    } else if (kind === "taskAssignee") {
      state.chartFilters.taskAssignee = "";
    } else if (kind === "messageSender") {
      state.chartFilters.messageSender = "";
    }

    renderCharts();
    renderTasks();
    renderMessages();
    renderActiveFilters();
  });

  refs.refreshTaskDetail.addEventListener("click", async () => {
    await handleTaskRefresh();
  });

  refs.copyTaskJson.addEventListener("click", async () => {
    await handleTaskCopy();
  });

  refs.agentsList.addEventListener("click", async (event) => {
    const target = event.target;
    if (!(target instanceof HTMLElement)) {
      return;
    }
    const action = target.dataset.action;
    const id = target.dataset.id;
    if (action === "focus-agent" || action === "inspect-agent") {
      await handleAgentSelect(id);
    } else if (action === "leave-agent") {
      await handleLeave(id);
    }
  });

  refs.messagesList.addEventListener("click", async (event) => {
    const target = event.target;
    if (!(target instanceof HTMLElement)) {
      return;
    }
    if (target.dataset.action === "mark-read") {
      await handleMarkRead(Number(target.dataset.id));
    }
  });

  refs.tasksList.addEventListener("click", async (event) => {
    const target = event.target;
    if (!(target instanceof HTMLElement)) {
      return;
    }
    if (target.dataset.action === "inspect-task") {
      await handleTaskSelect(Number(target.dataset.id));
    }
  });

  refs.focusAgent.addEventListener("change", async () => {
    state.focusAgent = refs.focusAgent.value;
    state.logsAgent = state.focusAgent;
    saveSettings();
    updateSelectors();
    renderAgents();
    await refreshDetailPanels();
    renderMessages();
    renderCharts();
    connectLogs(state.logsAgent);
  });

  refs.logAgent.addEventListener("change", () => {
    state.logsAgent = refs.logAgent.value;
    saveSettings();
    connectLogs(state.logsAgent);
  });

  refs.ackTask.addEventListener("click", async () => {
    const id = Number(refs.taskIdAction.value);
    const worker = refs.taskWorker.value.trim();
    if (id && worker) {
      await handleTaskAck(id, worker);
    }
  });

  refs.completeTask.addEventListener("click", async () => {
    const id = Number(refs.taskIdAction.value);
    const worker = refs.taskWorker.value.trim();
    const result = refs.taskResult.value.trim();
    if (id && worker && result) {
      await handleTaskComplete(id, worker, result);
    }
  });

  refs.requeueTask.addEventListener("click", async () => {
    const id = Number(refs.taskIdAction.value);
    if (id) {
      await handleTaskRequeue(id);
    }
  });
}

function scheduleAutoRefresh() {
  if (refreshTimer) {
    clearInterval(refreshTimer);
    refreshTimer = null;
  }

  if (state.autoRefresh) {
    refreshTimer = window.setInterval(refreshDashboard, 10000);
  }
}

function initRefs() {
  refs.connectionStatus = $("connectionStatus");
  refs.lastRefresh = $("lastRefresh");
  refs.metricAgents = $("metricAgents");
  refs.metricTasks = $("metricTasks");
  refs.metricUnread = $("metricUnread");
  refs.metricLogs = $("metricLogs");
  refs.settingsForm = $("settingsForm");
  refs.apiUrl = $("apiUrl");
  refs.apiKey = $("apiKey");
  refs.focusAgent = $("focusAgent");
  refs.autoRefresh = $("autoRefresh");
  refs.refreshNow = $("refreshNow");
  refs.joinForm = $("joinForm");
  refs.joinId = $("joinId");
  refs.joinRole = $("joinRole");
  refs.joinBackend = $("joinBackend");
  refs.agentDetail = $("agentDetail");
  refs.leaveSelected = $("leaveSelected");
  refs.agentsList = $("agentsList");
  refs.messageForm = $("messageForm");
  refs.messageFrom = $("messageFrom");
  refs.messageTo = $("messageTo");
  refs.messageBody = $("messageBody");
  refs.messagesList = $("messagesList");
  refs.taskForm = $("taskForm");
  refs.taskFrom = $("taskFrom");
  refs.taskTo = $("taskTo");
  refs.taskDescription = $("taskDescription");
  refs.taskIdAction = $("taskIdAction");
  refs.taskWorker = $("taskWorker");
  refs.taskResult = $("taskResult");
  refs.ackTask = $("ackTask");
  refs.completeTask = $("completeTask");
  refs.requeueTask = $("requeueTask");
  refs.tasksList = $("tasksList");
  refs.taskInspectorSummary = $("taskInspectorSummary");
  refs.refreshTaskDetail = $("refreshTaskDetail");
  refs.copyTaskJson = $("copyTaskJson");
  refs.taskSnapshot = $("taskSnapshot");
  refs.taskTimeline = $("taskTimeline");
  refs.taskRaw = $("taskRaw");
  refs.ghostosForm = $("ghostosForm");
  refs.ghostosPrompt = $("ghostosPrompt");
  refs.ghostosOutput = $("ghostosOutput");
  refs.logAgent = $("logAgent");
  refs.logOutput = $("logOutput");
  refs.clearLogs = $("clearLogs");
  refs.taskStatusChart = $("taskStatusChart");
  refs.taskLoadChart = $("taskLoadChart");
  refs.messageSourceChart = $("messageSourceChart");
  refs.activeFilters = $("activeFilters");
  refs.clearChartFilters = $("clearChartFilters");
}

async function boot() {
  initRefs();
  syncInputs();
  bindEvents();
  updateSelectors();
  scheduleAutoRefresh();
  await refreshDashboard();
}

window.addEventListener("beforeunload", () => {
  if (refreshTimer) {
    clearInterval(refreshTimer);
  }
  if (logsReconnectTimer) {
    clearTimeout(logsReconnectTimer);
  }
  if (logsSocket) {
    logsSocket.close();
  }
});

boot().catch((error) => {
  console.error(error);
  refs.connectionStatus.textContent = "Error";
  refs.lastRefresh.textContent = error.message;
  refs.logOutput.textContent = error.message;
});
