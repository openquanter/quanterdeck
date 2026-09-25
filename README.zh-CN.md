# Quanterdeck

**OpenQuanter 的自托管控制台。密钥留在你自己的机器上。**

[English](README.md) · 中文

> ⚠️ 早期开发中（v0.0.1）。接口不稳定。本项目不构成投资建议，风险自负。

---

## 这是什么

[OpenQuanter](https://github.com/openquanter/openquanter) 把每一分钱的去向都
记了下来——身份三元组、逐笔 parity、差额归因、不可分解的残差——但这些数字目前
只存在于文件和终端里。Quanterdeck 是把它们变成能看的东西的那一层。

> **让没有量化经验的人，在自己的机器上，30 分钟内跑出第一条净值曲线；
> 让有经验的人，永远不会把"无法判断"看成"一切正常"。**

后半句和前半句一样重要。这个控制台的一部分职责是**拒绝**。

## 不做什么

- **不做云托管、不做 SaaS、不代管你的 API key。** 上游把"代管用户密钥"列为
  非目标，这里也一样。
- **不做行情终端、不做社交跟单、不做策略市场。**
- **不替你判断策略好坏。** 它保证你在上线前确实跑过回测，不保证那次回测有意义。
- **不改变框架。** 控制台是消费者，单向依赖公开 crate。框架不必知道它存在。

## 需要登录，无条件需要

回环不是安全边界。同机的其他进程和账号都能打开这个端口；操作者浏览器里的一个
页面可以用 DNS rebinding 访问 `127.0.0.1`；`ssh -L` 一转发"只听本机"就名存实亡；
而开启写入之后，它能下单。

所以：Argon2id 密码（无条件）、`Host` 白名单、写入时校验 `Origin`、
`HttpOnly`+`SameSite=Strict` 会话、失败锁定、非回环监听强制第二因素。
首次启动没有密码时，deck 会在**启动它的那个终端**打印一次性令牌——读到它需要
本机访问权限，而令牌只活在进程内存里。

完整的威胁模型、做法与**尚未做的事**，见 [docs/SECURITY.zh-CN.md](docs/SECURITY.zh-CN.md)。

## 三条硬规则

写进代码，不是写进文档。

**1. "无法判断"永远不能渲染成"一致"。** 基准失效时——输入数据或有效配置动过——
关于引擎的任何结论都不成立。这时 `passes` 为假、差异列表为空、横幅是琥珀色不是
红色，并写明该做什么（rebase），因为它不是回归，把它画成回归会让人去找一个
不存在的 bug。

**2. 残差不完整时是"未知"，不是零。** 任一成因不可得时，归因的残差是 `None`。
一个由不完整分解算出的零残差，等于宣称"全都解释清楚了"——那正是它绝不能声称的。

**3. 读的东西不改运行时。** 控制台是观察者。这是上游 FR-CORE-7 的同一条规则。

## 状态

对一个 run 文件目录做到：

- 列出全部运行记录，**包括读不了的那个**——附上原因，而不是从列表里消失
- 逐条展示身份三元组、成交、已实现盈亏；无 tag 与空 tag 的成交显示不同
- 比对两次运行，并区分三种结论：可比对 / 仅代码变更 / **基准已失效**；并按 tick 文件对两次运行的成交做 markout 比较
- 展示参数扫描（`.sweep` 文件）：拒绝理由、折减夏普和 PBO **先于**参数表出现
- 能力自省：做不到的功能不渲染控件，并说明为什么做不到
- 默认只读、默认只听回环；监听其他地址而没有密码与第二因素时**拒绝启动**

对一个 journal 目录做到：

- 重建每个进程**相信自己持有什么**（持仓、均价、挂单；双向持仓按腿分开），并把解不开的帧数一并报出
- 与交易所读数对账：操作者粘贴的 `oq-recon --record` 输出，或对账进程写下的最新读数，逐条列出差别
- 按事件逐条回放 journal，可按类型筛选

以及归因：

- 把一次实盘与一次模型的差额分解到五个成因，数据来自 run 文件，或实时来自交易进程自己的 shadow 模型
- **已测得为零**与**不可得**分开呈现，残差不完整时是 `null` 而不是 `0`
- 说明还缺什么输入才能让不可得的成因变得可得

配合在交易主机上以独立用户运行的主机代理（`oq-agent`，在本仓库内），它还能运维那台主机：

- 从交易进程的控制口读状态：持仓、挂单、本次运行盈亏、生效中的风控限额、停机情况；查看和启停服务；停机、退出、解除停机
- 日志：日志文件，以及每个服务的 systemd journal（带时间戳）
- 部署签名构件，带健康检查和自动回滚
- 策略配置：表单与原始 JSON、保存前 diff、每个版本自动备份、回滚
- 每个策略实例从草稿到实盘的上线门控，写明某一步为什么不能走；配置一改就退回草稿
- 告警（停机、持仓不一致、服务停止、磁盘将满、时钟未同步、服务内存持续增长）及其历史、测试发送、静默；各进程用的是哪个交易所账户；实时行情的质量
- **黑匣子**：每 30 秒记录一次，保留 90 天——主机（负载、内存、资源压力、时钟、磁盘）、各服务（内存、CPU、任务数）和交易进程状态，状态变化与告警记为事件；复盘页可打开任一时刻：当时的快照、前后的决策与成交、前后的程序输出

高风险操作需要原因和一个**由代理验证**的一次性验证码，所以 deck 被攻破也无法单独行动。每个操作都写进哈希链审计并发到告警频道。

界面有新手模式（每个术语一句人话）和专家模式。目前只有中文。

## 上手

```bash
git clone https://github.com/openquanter/quanterdeck
cd quanterdeck

cd web && npm install && npm run build && cd ..   # 构建期才需要 Node
export OQ_DECK_RUNS_DIR=/path/to/your/runs
export OQ_DECK_JOURNALS_DIR=/path/to/your/journals   # 实盘对账需要
export OQ_DECK_TICKS_DIR=/path/to/your/ticks         # .oqtk 文件，markout 用来给成交定价
cargo run -p oq-deck                              # http://127.0.0.1:8899
```

第一次启动会在终端打印一个一次性令牌。用它走 `/setup` 设置密码，把返回的
`OQ_DECK_PASSWORD_HASH` 放进环境变量后重启。在此之前，除 `/api/v1/health`、`/api/v1/session`、
`/api/v1/session/login` 与 `/api/v1/setup` 外的所有接口都返回 401。

没有自己的 run 文件时，用仓库里的示例：

```bash
export OQ_DECK_RUNS_DIR=$PWD/examples/fixtures/runs
```

这些示例由框架自己的写入器生成（`cargo run -p oq-deck-core --example make_fixtures`），
所以格式一定合法——其中一个是**故意截断**的，用来看列表如何呈现读不了的文件。

## 配置

deck：

| 变量 | 作用 |
|---|---|
| `OQ_DECK_HOST`、`OQ_DECK_PORT` | 监听地址，默认 `127.0.0.1:8899`；非回环必须有第二因素 |
| `OQ_DECK_PASSWORD_HASH` | `/setup` 返回的 Argon2id hash |
| `OQ_DECK_TOTP_SECRET` | 第二因素；对外可达时必需 |
| `OQ_DECK_BEHIND_TLS` | 前面有 TLS 反向代理时设为 `1`，会话 cookie 标记 `Secure` |
| `OQ_DECK_EXTRA_HOSTS` | 监听地址之外允许访问的主机名（反向代理的） |
| `OQ_DECK_RUNS_DIR` | run 文件与 `.sweep` 文件 |
| `OQ_DECK_JOURNALS_DIR` | journal，用于对账与回放 |
| `OQ_DECK_TICKS_DIR` | `.oqtk` tick 文件，用于 markout |
| `OQ_DECK_VENUE_RECORD` | 对账进程写下的交易所最新读数，免粘贴对账 |
| `OQ_DECK_AGENT_SOCKET` | 主机代理的 socket；不设就没有运维功能 |
| `OQ_DECK_ALLOW_WRITES` | 设为 `1` 才允许任何操作，否则只读 |
| `OQ_DECK_WEB_DIST` | 构建好的界面，默认源码旁的 `web/dist` |

主机代理（`oq-agent`），默认值按参考部署的主机布局：

| 变量 | 默认 | 作用 |
|---|---|---|
| `OQ_AGENT_SOCKET` | `$RUNTIME_DIRECTORY/agent.sock` | deck 从这里连它 |
| `OQ_AGENT_PEERS` | `oq-deck` | 允许连接的用户 |
| `OQ_AGENT_UNITS` | 交易进程、对账进程、deck、代理、反向代理 | 展示并记录的服务 |
| `OQ_AGENT_MANAGEABLE` | `trader.service,oq-recon.service` | 允许启停的服务 |
| `OQ_AGENT_TRADER_UNIT` | `trader.service` | 交易进程 |
| `OQ_AGENT_CONTROL_DIR` | `/run/oq-live` | 交易进程控制口所在目录 |
| `OQ_AGENT_LOG_DIR` | `/var/log/oq` | 日志文件 |
| `OQ_AGENT_STATE` | `/var/lib/oq-agent` | 审计、告警、黑匣子、门控状态 |
| `OQ_AGENT_RELEASES`、`OQ_AGENT_INCOMING` | `/opt/oq/releases`、`/var/lib/oq/incoming` | 已安装与待部署的构件 |
| `OQ_AGENT_SIGNERS` | `/etc/oq/allowed_signers` | 只安装这些密钥签名的构件 |
| `OQ_AGENT_CONFIG_DIR` | `/var/lib/oq/config` | 允许它修改的策略配置 |
| `OQ_AGENT_JOURNALS` | `/var/lib/oq/journals` | journal，作为上线门控的证据 |
| `OQ_AGENT_HOST` | `host` | 告警里的主机名 |
| `OQ_AGENT_DISCORD_GUILD`、`OQ_AGENT_DISCORD_CHANNEL` | —、`alerts` | 告警发往哪里；机器人令牌以 systemd 凭据传入 |
| `OQ_AGENT_PROXY` | — | 告警渠道使用的 HTTP 代理 |

## 文档

| 文档 | 内容 |
|---|---|
| [STACK.zh-CN.md](docs/STACK.zh-CN.md) | 技术选型的理由、被否决的方案，以及 v1 错在哪里 |
| [UI-BRIEF.zh-CN.md](docs/UI-BRIEF.zh-CN.md) | 交给设计的页面清单：每屏的数据、状态与判定 |
| [SECURITY.zh-CN.md](docs/SECURITY.zh-CN.md) | 威胁模型、防护措施、以及尚未做的事 |
| [AGENTS.md](AGENTS.md) | 本地命令与十条不变量 |

## 许可

Apache-2.0。贡献需 DCO 签名（`git commit -s`），见 [CONTRIBUTING.md](CONTRIBUTING.md)。
