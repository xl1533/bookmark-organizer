<template>
  <div class="toolbar">
    <n-space :size="SPACE" align="center" :wrap-item="false" class="tool-group">
      <n-dropdown :options="importOptions" trigger="click" @select="onImportSelect">
        <n-button type="primary" ghost>{{ t("importMenu") }}</n-button>
      </n-dropdown>
      <n-button @click="emit('export')">{{ t("exportBtn") }}</n-button>
      <n-button @click="emit('createFolder')">{{ t("newFolder") }}</n-button>
    </n-space>

    <span class="toolbar-divider" aria-hidden="true"></span>

    <n-space :size="SPACE" align="center" :wrap-item="false" class="tool-group tool-group--checks">
      <n-button type="primary" :disabled="running" @click="emit('checkAll')">
        {{ t("checkAll") }}
      </n-button>
      <n-button :disabled="running" @click="emit('checkSelected')">
        {{ t("checkSelected") }}
      </n-button>
      <n-button type="warning" ghost :disabled="!running" @click="emit('stopCheck')">
        {{ t("stopCheck") }}
      </n-button>
      <n-button :disabled="running" @click="emit('incrementalCheck')">
        {{ t("incrementalCheck") }}
      </n-button>
    </n-space>

    <span class="toolbar-divider" aria-hidden="true"></span>

    <n-space :size="SPACE" align="center" :wrap-item="false" class="tool-group">
      <n-dropdown :options="organizeOptions" trigger="click" @select="onOrganizeSelect">
        <n-button type="primary" ghost class="organize-btn">{{ t("organizeTools") }}</n-button>
      </n-dropdown>
      <n-button @click="emit('dedup')">{{ t("dedup") }}</n-button>
      <n-button @click="emit('empty')">{{ t("emptyFolders") }}</n-button>
      <n-button type="error" ghost @click="emit('dead')">{{ t("deleteDead") }}</n-button>
      <n-badge :value="trashCount" :max="99">
        <n-button @click="emit('trash')">{{ t("trash") }}</n-button>
      </n-badge>
      <n-button @click="emit('settings')">{{ t("settings") }}</n-button>
    </n-space>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { NBadge, NButton, NDropdown, NSpace } from "naive-ui";
import { useI18n } from "../i18n";

// 统一按钮间距，避免散落的魔法数字
const SPACE = 8;

defineProps<{ running: boolean; trashCount: number }>();
const emit = defineEmits<{
  (e: "importSelect", mode: string): void;
  (e: "export"): void;
  (e: "createFolder"): void;
  (e: "checkAll"): void;
  (e: "checkSelected"): void;
  (e: "stopCheck"): void;
  (e: "incrementalCheck"): void;
  (e: "organizeSelect", key: string): void;
  (e: "dedup"): void;
  (e: "empty"): void;
  (e: "dead"): void;
  (e: "trash"): void;
  (e: "settings"): void;
}>();
const { t } = useI18n();

const importOptions = computed(() => [
  { label: t("importReplace"), key: "replace" },
  { label: t("importAppend"), key: "append" },
]);
const organizeOptions = computed(() => [
  { label: t("dedup"), key: "exactDedup" },
  { label: t("fuzzyDedup"), key: "fuzzyDedup" },
  { label: t("mergeFolders"), key: "mergeFolders" },
  { label: t("updateRedirects"), key: "updateRedirects" },
  { label: t("rulesClassify"), key: "rulesClassify" },
]);

function onImportSelect(key: string | number): void {
  emit("importSelect", String(key));
}
function onOrganizeSelect(key: string | number): void {
  emit("organizeSelect", String(key));
}
</script>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--bo-space, 12px);
  padding: 10px var(--bo-space-lg, 14px);
  border-bottom: 1px solid var(--bo-border, rgba(128, 128, 128, 0.18));
}
.tool-group {
  display: inline-flex;
  align-items: center;
}
.tool-group--checks {
  flex: 1;
  justify-content: center;
}
.toolbar-divider {
  flex: none;
  align-self: stretch;
  width: 1px;
  min-height: 22px;
  background: var(--bo-border, rgba(128, 128, 128, 0.18));
}
.organize-btn {
  font-weight: 600;
}
@media (max-width: 1200px) {
  .tool-group--checks {
    flex: none;
    justify-content: flex-start;
  }
  .toolbar-divider {
    display: none;
  }
}
</style>
