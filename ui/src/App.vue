<template>
  <n-config-provider
    :theme="naiveTheme"
    :locale="localeVal"
    :date-locale="dateLocaleVal"
    :theme-overrides="overrides"
    style="height: 100vh"
  >
    <n-message-provider>
      <n-dialog-provider>
        <MainView />
      </n-dialog-provider>
    </n-message-provider>
  </n-config-provider>
</template>

<script setup lang="ts">
import { computed } from "vue";
import {
  NConfigProvider,
  darkTheme,
  dateEnUS,
  dateZhCN,
  enUS,
  zhCN,
  type GlobalTheme,
} from "naive-ui";
import { storeToRefs } from "pinia";
import { useAppStore } from "./stores/app";
import { useI18n } from "./i18n";
import MainView from "./components/MainView.vue";

const store = useAppStore();
const { isDark } = storeToRefs(store);
const { locale } = useI18n();

const naiveTheme = computed<GlobalTheme | null>(() => (isDark.value ? darkTheme : null));
const localeVal = computed(() => (locale.value === "zh" ? zhCN : enUS));
const dateLocaleVal = computed(() => (locale.value === "zh" ? dateZhCN : dateEnUS));

// 品牌色覆盖
const overrides = computed(() => ({
  common: {
    primary: "#2563eb",
    primaryHover: "#1d4ed8",
    primaryPressed: "#1e40af",
  },
}));
</script>

<style>
html,
body,
#app {
  height: 100%;
  margin: 0;
  padding: 0;
}
body {
  font-family: "PingFang SC", "Microsoft YaHei", "Segoe UI", system-ui, sans-serif;
  -webkit-font-smoothing: antialiased;
}
</style>
