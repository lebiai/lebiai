# 远程访问与安全

`hermes-server` **默认是安全的**:绑 `127.0.0.1`(只本机可达),且每个 `/api/v1/*`
请求(REST + WebSocket)都必须带一个 **bearer token**,否则 401。所以"知道服务器
地址"也不够——还得有 token。

本文讲:怎么拿 token、怎么在本机用、怎么暴露到局域网/云服务器、以及 TLS 怎么做。

## 1. Token(鉴权)

`hermes serve` 启动时会确保有一个 token,按以下优先级取:

1. `--token <值>`
2. `--token-file <路径>`(文件里一行,自动 trim)
3. 环境变量 `HERMES_SERVER_TOKEN`
4. 持久化文件 `~/.lebi-ai/server.token`(不存在则**自动生成** 32 字节随机
   hex,权限 0600)

**任何情况都会拿到一个非空 token——服务器永不"裸奔"。** 启动日志里会打印一次完整
token(方便你复制进客户端),例如:

```
INFO hermes-server listening addr=127.0.0.1:8765
INFO auth token (set this in the client's server settings; REST: Authorization header, WS: ?token=) token=3f9a...
```

客户端(Flutter)在 **抽屉 → 服务器连接** 里填地址 + token。token 持久化在设备本地
(`shared_preferences`),REST 自动带 `Authorization: Bearer <token>`,WS 自动带
`?token=<token>`。

**轮换 token**:删掉 `~/.lebi-ai/server.token` 重启 server,会生成新的;
记得在所有客户端更新。

## 2. 本机(默认,最安全)

```sh
hermes serve
# 等价于:hermes serve --host 127.0.0.1 --port 8765
```

只绑 `127.0.0.1`,网络上的其它机器**根本连不上**。Mac 端 Flutter app 连
`http://localhost:8765` 即可。token 仍必填。

快速自检:

```sh
curl -i http://localhost:8765/api/v1/health            # → 401
curl -i -H "Authorization: Bearer <TOKEN>" \
     http://localhost:8765/api/v1/health                # → 200 {"status":"ok"}
```

## 3. 局域网(手机连电脑等)

显式绑到所有网卡,**token 仍然强制**:

```sh
hermes serve --host 0.0.0.0
# 或绑具体 LAN IP:hermes serve --host 192.168.1.10
```

手机端填 `http://<电脑IP>:8765` + token。注意此时流量是**明文 HTTP**——token 和对话
内容在共享 WiFi 下理论上可被嗅探。家用/可信网络一般可接受;不可信网络请走下面的 TLS。

## 4. 云服务器(远程访问,务必上 TLS)

把 server 放公网时,bearer token 在明文 HTTP 上会被中间人抓走。**必须套 TLS**。
按你的选择,hermes-server **不在二进制里做 TLS**——用反代。两种推荐:

### 4a. Caddy 反向代理(自动 Let's Encrypt,最省心)

Caddy 是单二进制,自动申请/续期证书。server 仍只绑本机:

```sh
# 服务器上
hermes serve --host 127.0.0.1 --port 8765
```

把域名(如 `hermes.example.com`)A 记录指向服务器 IP,然后:

```caddyfile
# /etc/caddy/Caddyfile
hermes.example.com {
    reverse_proxy 127.0.0.1:8765
}
```

```sh
sudo caddy run --config /etc/caddy/Caddyfile
```

Caddy 自动:`https://hermes.example.com` ↔ `http://127.0.0.1:8765`,`Authorization`
头透传,**WebSocket Upgrade 自动处理**(Caddy 原生支持)。客户端填
`https://hermes.example.com` + token,WS 自动走 `wss://`。

> 防火墙只开 443(和 80,给 ACME 验证)。8765 **不要**对公网开放——它只绑 127.0.0.1
> 也访问不到,正好。

### 4b. Cloudflare Tunnel(零开放端口,无需证书)

不想暴露任何端口、不想管证书,用 `cloudflared`:

```sh
# 服务器上,hermes serve 同样只绑 127.0.0.1:8765
cloudflared tunnel --url http://localhost:8765
# 按提示在 Cloudflare 后台绑定一个域名(如 hermes.example.com)
```

得到一个 `https://hermes.example.com`,TLS 由 Cloudflare 终结, Authorization / WS
都透传。公网完全摸不到你的服务器。

## 5. 安全模型小结

| 场景 | bind | TLS | token | 谁能连 |
|---|---|---|---|---|
| 本机(默认) | `127.0.0.1` | 不需要 | 必填 | 仅本机 |
| 局域网 | `0.0.0.0` 或 LAN IP | 建议但非必须 | 必填 | 同网段 + token |
| 公网 | 反代到 `127.0.0.1` | **必须**(Caddy/CF) | 必填 | 任何人(经 TLS)+ token |

- token 是**共享密钥**:泄露=等同账号泄露。别提交进 git、别贴到公开处。
- `/api/v1/*` 全部鉴权(含 `/health`),无任何匿名端点。
- 危险工具(文件/shell 等)仍受 agent 的 confirm(Platypus)流程逐次把关——鉴权管
  "谁能用",confirm 管 "用的时候要不要你点头"。

## 不做(设计取舍)

- **不在二进制内置 TLS**(`--tls-cert` 等):证书生命周期交给反代/Cloudflare 更可靠。
  需要时再加 `axum-server` + rustls 即可。
- **暂不多用户 / 每设备令牌 / 限流**:单用户共享一个 token 即可。多设备要可吊销时,
  再加 `/api/v1/devices` 签发独立令牌。
