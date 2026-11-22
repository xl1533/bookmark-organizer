<template>
  <div class="status-bar">
    <span>{{ t("statBookmarks") }} {{ stats?.bookmarks ?? "-" }}</span>
    <span>{{ t("statFolders") }} {{ stats?.folders ?? "-" }}</span>
    <span class="c-ok">{{ t("statOk") }} {{ stats?.ok ?? 0 }}</span>
    <span class="c-susp">{{ t("statSuspicious") }} {{ stats?.suspicious ?? 0 }}</span>
    <span class="c-dead">{{ t("statDead") }} {{ stats?.dead ?? 0 }}</span>
    <span class="c-gray">{{ t("statUnchecked") }} {{ stats?.unchecked ?? 0 }}</span>
    <span class="spacer"></span>
    <span class="db" :title="dbPath">{{ t("dbPath") }}: {{ dbPath }}</span>
  </div>
</template>

<script setup lang="ts">
import { useI18n } from "../i18n";
import type { Stats } from "../types";

defineProps<{ stats: Stats | null; dbPath: string }>();
const { t } = useI18n();
</script>

<style scoped>
.status-bar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--bo-space, 12px);
  padding: 6px var(--bo-space-lg, 14px);
  font-size: 12px;
  border-top: 1px solid var(--bo-border, rgba(128, 128, 128, 0.18));
  opacity: 0.85;
}
.status-bar .spacer {
  flex: 1;
}
.status-bar .db {
  max-width: 380px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.c-ok {
  color: #22c55e;
}
.c-susp {
  color: #f59e0b;
}
.c-dead {
  color: #ef4444;
}
.c-gray {
  opacity: 0.6;
}
</style>
