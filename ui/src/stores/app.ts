import { defineStore } from "pinia";
import { computed, reactive, ref } from "vue";
import { api } from "../api";
import type {
  CheckFinishedEvent,
  CheckProgressEvent,
  CheckStatus,
  Stats,
  TreeNode,
} from "../types";

export const useAppStore = defineStore("app", () => {
  // ---------------- 数据 ----------------
  const nodes = ref<TreeNode[]>([]);
  const stats = ref<Stats | null>(null);
  const dbPath = ref("");
  const loading = ref(false);

  // 检测运行态
  const running = ref(false);
  const progress = reactive({ done: 0, total: 0, ok: 0, suspicious: 0, dead: 0 });
  const lastUrl = ref("");
  // 检测过程中的即时状态覆盖（nodeId -> status）
  const liveStatus = ref(new Map<number, CheckStatus>());

  // 多选
  const checkedIds = ref<number[]>([]);

  // ---------------- 主题（跟随系统，可覆盖） ----------------
  const themeMode = ref<"auto" | "light" | "dark">(
    (localStorage.getItem("bo.theme") as "auto" | "light" | "dark") ?? "auto"
  );
  const systemDark = ref(
    window.matchMedia("(prefers-color-scheme: dark)").matches
  );
  window
    .matchMedia("(prefers-color-scheme: dark)")
    .addEventListener("change", (e) => {
      systemDark.value = e.matches;
    });
  const isDark = computed(() =>
    themeMode.value === "auto" ? systemDark.value : themeMode.value === "dark"
  );
  function setThemeMode(m: "auto" | "light" | "dark") {
    themeMode.value = m;
    localStorage.setItem("bo.theme", m);
  }

  // ---------------- 动作 ----------------
  async function refresh() {
    loading.value = true;
    try {
      const [tree, st, db] = await Promise.all([
        api.getTree(),
        api.getStats(),
        api.getDbPath(),
      ]);
      nodes.value = tree;
      stats.value = st;
      dbPath.value = db;
    } finally {
      loading.value = false;
    }
  }

  function nodeById(id: number): TreeNode | undefined {
    return nodes.value.find((n) => n.id === id);
  }

  function effectiveStatus(n: TreeNode): CheckStatus | null {
    const live = liveStatus.value.get(n.id);
    if (live) return live;
    return n.check?.status ?? null;
  }

  async function startCheck(scope: "all" | "selected", ids: number[]) {
    const res = await api.startCheck(scope, ids);
    running.value = true;
    progress.done = 0;
    progress.total = res.total;
    progress.ok = 0;
    progress.suspicious = 0;
    progress.dead = 0;
    liveStatus.value = new Map();
    return res.total;
  }

  function onCheckProgress(ev: CheckProgressEvent) {
    progress.done = ev.done;
    progress.total = ev.total;
    lastUrl.value = ev.url;
    if (ev.status === "ok") progress.ok++;
    else if (ev.status === "suspicious") progress.suspicious++;
    else progress.dead++;
    liveStatus.value.set(ev.nodeId, ev.status);
  }

  async function onCheckFinished(_ev: CheckFinishedEvent) {
    running.value = false;
    liveStatus.value = new Map();
    await refresh();
  }

  async function cancelCheck() {
    await api.cancelCheck();
  }

  function clearChecked() {
    checkedIds.value = [];
  }

  return {
    nodes,
    stats,
    dbPath,
    loading,
    running,
    progress,
    lastUrl,
    liveStatus,
    checkedIds,
    themeMode,
    isDark,
    setThemeMode,
    refresh,
    nodeById,
    effectiveStatus,
    startCheck,
    onCheckProgress,
    onCheckFinished,
    cancelCheck,
    clearChecked,
  };
});
