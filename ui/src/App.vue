<template>
  <n-config-provider
    :theme="naiveTheme"
    :locale="localeVal"
    :date-locale="dateLocaleVal"
    :theme-overrides="overrides"
    :style="rootStyle"
    :class="{ 'app-dark': isDark }"
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
  NDialogProvider,
  NMessageProvider,
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
// Naive UI 的 darkTheme 只作用于组件，根页面/滚动区需要显式上色，避免"白底白字"
const rootStyle = computed(() => ({
  height: "100vh",
  backgroundColor: isDark.value ? "#18181c" : "#ffffff",
  color: isDark.value ? "#e0e0e0" : "#303133",
}));
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
  background: #ffffff;
  color: #303133;
}
body:has(.app-dark) {
  background: #18181c;
  color: #e0e0e0;
}
.link-label {
  cursor: pointer;
}
.link-label:hover {
  text-decoration: underline;
}
</style>
