// 应用引导（由 main.ts 在安装完错误监听后动态 import）
import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";

createApp(App).use(createPinia()).mount("#app");
