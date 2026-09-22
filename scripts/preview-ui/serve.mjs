/**
 * 开发用：把 `crates/hermes-gui/ui/dist` 用浏览器能看的方式端出来。
 *
 * 与桌面 GUI 的唯一差别是注入 `mock.js`（假 IPC）。**不是产品入口**，
 * 别拿它当验收：真数据、真流式、真文件都只在桌面 App 里。
 */
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join, normalize } from "node:path";

const DIST = new URL("../../crates/hermes-gui/ui/dist/", import.meta.url).pathname;
const MOCK = new URL("./mock.js", import.meta.url).pathname;
const PORT = Number(process.env.PORT ?? 4173);

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".woff2": "font/woff2",
};

createServer(async (req, res) => {
  const url = new URL(req.url, "http://localhost");
  if (url.pathname === "/__preview.js") {
    res.writeHead(200, { "content-type": TYPES[".js"] });
    res.end(await readFile(MOCK, "utf8"));
    return;
  }
  const rel = url.pathname === "/" ? "/index.html" : url.pathname;
  const path = join(DIST, normalize(rel).replace(/^(\.\.[/\\])+/, ""));
  try {
    const body = await readFile(path);
    if (rel.endsWith(".html")) {
      // 桩必须在应用包**之前**跑：invoke 是全局函数，晚一步就白屏。
      const html = body
        .toString("utf8")
        .replace("<head>", '<head>\n    <script src="/__preview.js"></script>');
      res.writeHead(200, { "content-type": TYPES[".html"] });
      res.end(html);
      return;
    }
    res.writeHead(200, { "content-type": TYPES[extname(path)] ?? "application/octet-stream" });
    res.end(body);
  } catch {
    res.writeHead(404).end("not found");
  }
}).listen(PORT, () => console.log(`preview: http://localhost:${PORT}`));
