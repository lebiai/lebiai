# web_search 稳定性说明与推荐方案

> **种类：** E 开发者手册（非权威）。冲突以 P0 v0.9 为准。网页搜索须用户主动触发。

## 当前策略（自动降级）

一次 `web_search` 会按顺序尝试，**任一成功即返回**（结果前缀可能带 `via xxx fallback`）：

1. 配置的首选后端（`scraper` / `tavily` / `brave_api` / `searxng`）
2. 其它已配置的 API / SearXNG
3. DuckDuckGo HTML（无 Key）
4. Bing HTML（无 Key）
5. **系统 `curl` 拉 DuckDuckGo / Bing**（reqwest 被墙/限流时的最后手段）

因此 **不必手写「失败再 curl」的业务逻辑**——已内置。

## 推荐方案（按优先级）

### 1. 自建 SearXNG（最推荐 · 免费开源 · 本地优先）

[SearXNG](https://docs.searxng.org/) 是开源元搜索引擎，聚合 Google/Bing/DDG 等，无厂商锁死。

```bash
# 示例：Docker 一键（本机 8080）
docker run -d -p 8080:8080 searxng/searxng
```

`~/.lebi-ai/config.toml`：

```toml
[web]
search_backend = "searxng"
searxng_url = "http://127.0.0.1:8080"
```

重启 GUI / CLI 后生效。

### 2. Tavily 免费额度（省心，有 Key）

注册 [tavily.com](https://tavily.com) 拿 API Key：

```toml
[web]
search_backend = "tavily"
tavily_api_key = "tvly-..."
```

适合不想运维、可接受少量云 API 的用户。

### 3. 保持默认 scraper + 自动降级

零配置：Brave HTML → DDG → Bing → curl。  
在国内网络/代理下可能仍不稳定，但比单源 Brave 强。

## 不推荐

- 依赖单一公共 SearXNG 实例（别人限流、隐私差）
- 把 API Key 写进对话 / 提交 git

## 与 web_fetch

搜索成功后，再用 `web_fetch` 打开 1–2 个结果 URL。  
若域名被解析到 `198.18.x`（Clash fake-ip），当前版本**已放行域名解析到 fake-ip**；字面量私网 IP 仍拦截。
