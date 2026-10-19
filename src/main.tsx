import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

/**
 * 关掉 WebView 的默认右键菜单。
 *
 * 不关的话，在界面任何地方右键都会弹出「刷新 / 另存为 / 检查」这类浏览器菜单 ——
 * 对一个桌面应用来说是纯粹的噪音，用户会以为点错了东西。
 *
 * 输入框和文本域例外：粘贴、全选、拼写检查是真要用的，拦掉反而变难用。
 */
document.addEventListener("contextmenu", (e) => {
  const target = e.target as HTMLElement | null;
  const editable = target?.closest?.(
    "input, textarea, [contenteditable='true']",
  );
  if (!editable) e.preventDefault();
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
