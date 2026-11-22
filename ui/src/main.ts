// 屏蔽 WebView 自带的浏览器右键菜单（树节点上使用应用自己的上下文菜单）。
// 输入框/可编辑区域保留原生菜单，以便复制粘贴。
window.addEventListener(
  "contextmenu",
  (event) => {
    const target = event.target as HTMLElement | null;
    if (target?.closest("input, textarea, select, [contenteditable='true']")) return;
    event.preventDefault();
  },
  true
);

// 启动错误可视化：任何导致白屏的 JS 错误都会以红字浮层显示，方便诊断
function showFatal(title: string, detail: unknown) {
  const err = detail as { stack?: string; message?: string } | undefined;
  const el = document.createElement("pre");
  el.style.cssText =
    "position:fixed;inset:0;z-index:99999;margin:0;background:#fff;color:#b91c1c;" +
    "font:12px/1.6 Consolas,monospace;padding:16px;overflow:auto;white-space:pre-wrap";
  el.textContent =
    `${title}\n\n` +
    (err?.stack ?? err?.message ?? String(detail ?? "(无详情)"));
  document.body.appendChild(el);
}

window.addEventListener("error", (e) =>
  showFatal("页面脚本错误", e.error ?? e.message)
);
window.addEventListener("unhandledrejection", (e) =>
  showFatal("未处理的 Promise 异常", e.reason)
);

// 动态加载应用：确保错误监听先于业务模块安装，
// 模块加载阶段的异常也能被捕获显示
import("./bootstrap").catch((e) => showFatal("应用加载失败", e));
