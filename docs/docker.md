# Docker 部署

Hermes 提供一个 distroless 静态镜像，主要用于把 `hermes wechat run` 作为
后台长跑服务。同一个镜像也能跑 `ask` / `chat` / `run` 等任意子命令。

## 一键部署微信 Bot

```bash
# 1. 准备配置目录（与裸机安装完全一致的布局）
mkdir -p ~/.small-rust-hermes
cat > ~/.small-rust-hermes/config.toml <<'EOF'
default_provider = "anthropic"

[providers.anthropic]
base_url = "https://api.anthropic.com"
api_key = "sk-ant-..."
model = "claude-sonnet-4-20250514"
EOF
chmod 600 ~/.small-rust-hermes/config.toml

# 2. 构建镜像
docker compose build

# 3. 扫码登录微信（一次性，token 写到挂载卷里的 wechat.toml）
docker compose run --rm hermes-wechat wechat login

# 4. 启动长跑服务
docker compose up -d

# 5. 看日志 / 收到的消息
docker compose logs -f
```

停止 / 重启：

```bash
docker compose stop
docker compose restart
docker compose down            # 停掉并移除容器；数据卷不动
```

## 数据 / 配置

容器里 `HOME=/data`，宿主机的 `~/.small-rust-hermes/` 会挂到
`/data/.small-rust-hermes/`，所以 `config.toml`、`wechat.toml`、
`wechat-cursor.txt`、`sessions/`、`skills/`、`memories/` 全在宿主机本地，
和裸机安装可以无缝互换。

想把数据放别处？设环境变量再起：

```bash
HERMES_HOME=/srv/hermes-data docker compose up -d
```

## 当成通用 CLI 用

同一个镜像可以临时跑任意子命令：

```bash
# 一次性问答
docker run --rm \
  -v ~/.small-rust-hermes:/data/.small-rust-hermes \
  -e HOME=/data \
  hermes:latest ask "解释一下这段错误"

# 交互式 chat（需要 -it）
docker run --rm -it \
  -v ~/.small-rust-hermes:/data/.small-rust-hermes \
  -e HOME=/data \
  hermes:latest chat
```

## 镜像说明

- 构建：`rust:1-bookworm`（最新稳定 Rust），原生 glibc 工具链。
- 运行：`debian:bookworm-slim` + `ca-certificates`，~100 MB。
- TLS：`reqwest` 走 `rustls`，但传递依赖里 `aws-lc-sys` 需要 glibc，所以
  没有走纯静态 musl 路线。
- 默认非 root 运行（uid 1000，用户名 `hermes`）。如果宿主机挂载目录是
  root 拥有的，要么改属主 `sudo chown -R 1000:1000 ~/.small-rust-hermes`，
  要么在 `docker-compose.yml` 加 `user: "${UID}:${GID}"` 跑成当前用户。

## 常见问题

**Q: `wechat login` 看不到二维码？**
A: 终端要支持 Unicode 半角块字符。Windows 上建议用 Windows Terminal 或
WSL；老 cmd.exe 会乱码。

**Q: token 过期了？**
A: 容器里直接重跑登录：
```bash
docker compose run --rm hermes-wechat wechat login
docker compose restart
```

**Q: 想自己 build 一个 arm64 / Apple Silicon 镜像？**
A: 改 Dockerfile 里的 target 为 `aarch64-unknown-linux-musl`，
或者用 `docker buildx build --platform linux/arm64 -t hermes:arm64 .`。
