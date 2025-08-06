# Ark-Vote

明日方舟投票系统的后端服务，使用 Rust 构建。

## 安装与运行

### 1. 克隆仓库

```bash
git clone https://github.com/ArknightsVote/Backend.git
cd Backend
```

### 2. 构建项目

```bash
cargo build
```

### 3. 运行服务

项目支持多种服务模式，可以通过命令行参数指定启动特定服务：

```bash
# 运行 Web 服务
cargo run -- web-server

# 运行 NATS 消费者服务
cargo run -- nats-consumer

# 运行服务测试
cargo run -- service-test
```

或者docker构建并拉起

```sh
docker build -t arkvote:latest .

docker compose up -d
```

## 项目配置

配置文件位于 `config/app.toml`，首次运行时会自动创建默认配置。您可以根据需要修改配置文件中的参数。

示例配置可参考：`services/share/app.default.toml`

## 项目结构

```
ark-vote/
├── config/             # 配置文件目录
├── logs/               # 日志文件目录
├── services/           # 微服务模块
│   ├── nats-service/   # NATS 消息服务
│   ├── service-test/   # 测试服务
│   ├── share/          # 共享库
│   └── web-service/    # Web API 服务
├── src/                # Cli 包装
├── static/             # 静态资源文件
└── templates/          # 模板文件
```

## 贡献指南

欢迎提交问题报告和合并请求。
