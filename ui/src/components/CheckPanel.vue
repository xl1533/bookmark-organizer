<template>
  <section class="check-panel">
    <header class="panel-head">
      <span class="panel-title">{{ t("checkProgressLabel") }}</span>
      <span class="panel-pct">{{ pct }}%</span>
    </header>

    <n-progress type="line" :percentage="pct" :processing="running" :show-indicator="true" />

    <div class="counter-grid">
      <div class="counter-card ok">
        <span class="counter-dot ok"></span>
        <span class="counter-label">{{ t("statOk") }}</span>
        <b>{{ progress.ok }}</b>
      </div>
      <div class="counter-card susp">
        <span class="counter-dot susp"></span>
        <span class="counter-label">{{ t("statSuspicious") }}</span>
        <b>{{ progress.suspicious }}</b>
      </div>
      <div class="counter-card dead">
        <span class="counter-dot dead"></span>
        <span class="counter-label">{{ t("statDead") }}</span>
        <b>{{ progress.dead }}</b>
      </div>
      <div class="counter-card total">
        <span class="counter-label">{{ t("checkProgressLabel") }}</span>
        <b>{{ progress.done }} / {{ progress.total }}</b>
      </div>
    </div>

    <div class="last-url" :title="lastUrl">{{ lastUrl }}</div>

    <n-alert type="default" :bordered="false" class="disclaimer">{{ t("checkDisclaimer") }}</n-alert>
  </section>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { NAlert, NProgress } from "naive-ui";
import { useI18n } from "../i18n";

interface ProgressLike {
  done: number;
  total: number;
  ok: number;
  suspicious: number;
  dead: number;
}

const props = defineProps<{ running: boolean; progress: ProgressLike; lastUrl: string }>();
const { t } = useI18n();

const pct = computed(() =>
  props.progress.total > 0
    ? Math.round((props.progress.done / props.progress.total) * 100)
    : 0
);
</script>

<style scoped>
.check-panel {
  flex: 1;
  min-width: 260px;
  max-width: 420px;
  padding: var(--bo-space-lg, 14px);
  display: flex;
  flex-direction: column;
  gap: var(--bo-space, 12px);
}
.panel-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--bo-space-sm, 8px);
}
.panel-title {
  font-weight: 600;
  font-size: 14px;
}
.panel-pct {
  font-size: 12px;
  opacity: 0.7;
  font-variant-numeric: tabular-nums;
}
.counter-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--bo-space-sm, 8px);
}
.counter-card {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  padding: 8px 10px;
  border: 1px solid var(--bo-border, rgba(128, 128, 128, 0.18));
  border-radius: var(--bo-radius, 8px);
  background: var(--bo-surface-2, rgba(128, 128, 128, 0.06));
  font-size: 12px;
}
.counter-card b {
  margin-left: auto;
  font-size: 16px;
  font-variant-numeric: tabular-nums;
}
.counter-label {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.counter-dot {
  flex: none;
  width: 8px;
  height: 8px;
  border-radius: 50%;
}
.counter-dot.ok {
  background: #22c55e;
}
.counter-dot.susp {
  background: #f59e0b;
}
.counter-dot.dead {
  background: #ef4444;
}
.counter-card.ok b {
  color: #22c55e;
}
.counter-card.susp b {
  color: #f59e0b;
}
.counter-card.dead b {
  color: #ef4444;
}
.last-url {
  font-size: 11px;
  opacity: 0.6;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-height: 14px;
}
.disclaimer {
  font-size: 11px;
  opacity: 0.7;
}
</style>
