# OpenQuanter 管理后台 · 立项方案 v0.1

> 状态：**草案，待评审** · 日期：2026-09-08
> 定位：独立开源项目，OpenQuanter 的自托管管理控制台

---

## 1. 命名

| 候选 | 含义 | 评价 |
|---|---|---|
| **Quanterdeck**（推荐） | quarterdeck 后甲板 / 舰桥指挥位 + Quanter；包名/二进制 `oq-deck`，命令 `oq deck` | 独特、可品牌化、与 OpenQuanter 明显同源；`deck` 兼有"交易台 trading desk"双关；GitHub/npm/PyPI 冲突概率极低 |
| **OpenQuanter Console**（`oq-console`） | 直白描述 | 零歧义、SEO 友好，但无辨识度，搜索结果里会被各种 console 淹没 |
| **Binnacle** | 罗经柜，舰桥上安放罗盘的座架 | 克制、单词、与主仓的冷峻文风一致；但含义太隐晦，中文社区无感 |

**建议：项目名 `Quanterdeck`，仓库 `openquanter/quanterdeck`，发行包 `oq-deck`，CLI `oq-deck up`。**
理由：主仓文风克制（"P&L you cannot explain is not P&L"），后台是面向新手的产品面，允许有一点品牌温度，但不能跳脱。`Quanterdeck` 是这三者里唯一同时满足"一眼知道是 OpenQuanter 家的"和"能注册到域名/包名"的。

---

## 2. 产品定位（一句话）

> **让没有量化经验的人，在自己的机器上，30 分钟内跑出第一条净值曲线；
> 让有经验的人，永远不会因为点错一个按钮而丢掉挂单。**

两句话对应两类硬需求，也对应本项目仅有的两个差异化：**新手上手路径** 与 **上线纪律产品化**。
不做的事：不做云托管、不做 SaaS、不收管用户 API key、不做行情终端、不做社交跟单。
（前两条直接继承主仓 REQUIREMENTS §7 非目标。）

---

## 3. 三条不可协商的设计约束

### C1 观察者不改内核
主仓 FR-CORE-7：journal 读者（监控、分析、IPC 消费者）永远是观察者，不能影响内核状态。
→ 后台读取状态**只走只读通道**（journal / 日志 / 交易所 API），任何写操作必须走运行时自己的受控入口，不得直接改内存、不得直接改运行中进程的状态文件。
→ 具体推论：**永不写 `config/cta_strategy_data.json`**（那是重启交接的临时文件，不是权威源）。

### C2 危险操作编码为不可选，而不是写在文档里
生产环境已经用血换来的规则，必须固化在代码里：

| 规则 | 后台的实现方式 |
|---|---|
| daemon 只能 `manager.sh daemon restart`（SIGUSR1），绝不 stop+start | UI 只有"重启"一个按钮；`stop`/`start` 收进 Danger Zone，需输入实例名二次确认，并明示"会取消全部挂单、丢失订单认领" |
| 不备份不修改 | 任何配置写入前自动 `.bak.YYYYMMDD-HHMM`，UI 内可一键回滚 |
| 不回测不上线 | 策略实例的"启用实盘"开关，在没有关联的通过态回测记录时**置灰**（见 §6 上线门控） |
| 进程管理唯一入口 manager.sh | 适配器只允许调用白名单命令，禁止任意 shell 透传 |

### C3 密钥永不离开本机
- 默认 `bind 127.0.0.1`；监听非回环地址必须显式配置且强制设置密码 + TOTP，否则拒绝启动。
- API key 用主密钥（OS keyring 优先，回退 passphrase）加密落盘，**不进 git、不进日志、不进任何 API 响应**；前端只拿到掩码与权限位。
- 录入 key 时主动校验交易所权限：检测到"允许提币"直接红条阻断。
- 零遥测、零云端组件、无外呼（除交易所与用户显式配置的通知渠道）。

---

## 4. 架构：一层适配器决定项目寿命

后台面对的运行时有两个，且会长期并存：

- **v1 = Python 1.x**：`config/*.json` + `manager.sh` + `daemon.py` + `data/logs/` + `data/report/<ts>/`
- **v2 = Rust 2.0**：`oq` CLI（`backtest`/`sweep`/`live`/`replay`/`parity`/`data`）+ journal

如果后台直接贴着任一运行时写，另一边接入时就是重写。因此**后台内部只认一份契约**：

```
          ┌────────────────────────────────────────────┐
          │  Web SPA (React + TS + Tailwind)           │
          └───────────────┬────────────────────────────┘
                          │  REST + SSE（版本化 /api/v1）
          ┌───────────────┴────────────────────────────┐
          │  Deck Server (FastAPI)                     │
          │  · 认证/审计/配置版本库/任务队列/告警        │
          └───────────────┬────────────────────────────┘
                          │  Runtime Protocol（唯一契约）
          ┌───────────────┴────────────────────────────┐
          │  adapters/                                 │
          │   ├── legacy_py/   1.x：JSON + manager.sh  │
          │   └── oq_cli/      2.0：oq CLI + journal   │
          └────────────────────────────────────────────┘
```

`RuntimeAdapter` 协议（首版，10 个方法，刻意小）：

```python
class RuntimeAdapter(Protocol):
    def capabilities(self) -> Capabilities: ...          # 哪些功能可用，UI 据此显隐
    def list_services(self) -> list[Service]: ...        # daemon/monitor/mail/ticker/sync...
    def service_action(self, name, action) -> Result: ...# 白名单：status/restart/(danger)start|stop
    def list_strategies(self) -> list[StrategyInst]: ...
    def strategy_schema(self, cls) -> JSONSchema: ...    # 由 get_class_parameters() 派生
    def read_config(self, key) -> ConfigDoc: ...
    def write_config(self, key, doc, *, backup=True) -> Diff: ...
    def submit_backtest(self, spec) -> JobId: ...
    def job_events(self, job_id) -> Iterator[Event]: ...
    def live_state(self) -> LiveState: ...               # 持仓/挂单/成交/风控，只读
```

**关键收益**：`capabilities()` 让 UI 天然支持"2.0 还没做完"的状态——没有的能力不渲染，而不是报错。

### 主机边界
v0.x **只管本机**（后台与运行时同机，localhost 通信）。
但所有资源模型从第一天起带 `host_id` 字段，多主机（host-01/02/03）留作后续以 agent 模式接入，届时不需要改数据模型。

---

## 5. 仓库结构

```
quanterdeck/
├── README.md              README.zh-CN.md
├── LICENSE                # Apache-2.0，与公开主仓一致
├── CONTRIBUTING.md        # DCO 签名要求，与主仓一致
├── AGENTS.md              # <200 行，本地命令与不变量（对齐主仓 NFR-10）
├── CHANGELOG.md
│
├── apps/
│   ├── web/                       # 前端 SPA
│   │   ├── src/
│   │   │   ├── routes/            # 路由级页面，与 §7 页面清单一一对应
│   │   │   ├── components/        # 复用组件
│   │   │   ├── features/          # 按领域切：onboarding/strategy/backtest/live/config
│   │   │   ├── api/               # 由 OpenAPI 生成的 TS 客户端（勿手写）
│   │   │   ├── design/            # tokens.css、主题、图表配色
│   │   │   ├── i18n/              # zh-CN / en，文案 key 化
│   │   │   └── _incoming/         # ★ readdy 导出落地区，见 §8
│   │   ├── index.html  vite.config.ts  tailwind.config.ts
│   │
│   └── server/                    # 后端
│       └── oq_deck/
│           ├── main.py            # FastAPI app 装配
│           ├── api/v1/            # 路由：auth/services/strategies/configs/backtests/live/alerts
│           ├── domain/            # 纯逻辑，无 IO：门控规则、diff、schema 推导
│           ├── store/             # SQLite：用户/审计/任务/配置版本/门控记录
│           ├── jobs/              # 任务队列（进程池，非 celery）
│           ├── security/          # 密钥保险箱、TOTP、权限
│           ├── audit/             # 谁·何时·改了什么·diff·可回滚
│           └── settings.py
│
├── adapters/
│   ├── protocol.py                # RuntimeAdapter 协议 + 数据模型（单一真相源）
│   ├── legacy_py/                 # 1.x 适配器
│   │   ├── configs.py             # cta_strategy_setting*.json 读写 + 备份
│   │   ├── services.py            # manager.sh 白名单调用
│   │   ├── logs.py                # data/logs 尾随
│   │   └── reports.py             # data/report/<ts>/ 结果解析
│   └── oq_cli/                    # 2.0 适配器
│       ├── cli.py                 # oq backtest/sweep/live/... 封装
│       └── journal.py             # journal 只读消费
│
├── schemas/                       # JSON Schema：配置/策略参数/回测 spec（前后端共用）
├── examples/                      # 样例策略、样例配置、样例 tick 数据（可跑通全流程）
├── deploy/                        # Dockerfile、docker-compose.yml、systemd unit
├── docs/                          # 双语；含"5 分钟上手""安全模型""适配器如何写"
├── scripts/                       # dev.sh、gen-client.sh（OpenAPI→TS）、check-*.sh
└── tests/
    ├── unit/  integration/        # 适配器用 fixture 仓库（假的 openquanter 目录树）
    └── e2e/                       # Playwright：向导→回测→看到净值曲线
```

**几条结构上的取舍：**
- **单仓（monorepo），不拆前后端仓**。使用者克隆一次就能跑，这是"新手 30 分钟"的前提。
- **`adapters/` 与 `apps/server/` 平级**，不塞进 server 内部。它是本项目最可能被第三方扩展的部分（有人要接自己的运行时），平级放置是在结构上表态。
- **`schemas/` 独立**。前端表单、后端校验、CLI 三方共用一份，避免三处漂移。
- **前端不用 Next.js，用 Vite SPA**。生产形态是"后端把静态包一起 serve"，不需要 SSR，也不能要求用户装 Node 运行时。

---

## 6. 功能范围与优先级

### P0 — 没有它就不成立
1. **首次运行向导**：环境自检（Python/依赖/目录）→ 选运行时与路径 → 交易所与 API key（默认测试网）→ 连通性自检 → 拉样例数据 → 跑示例回测 → **看到净值曲线**。全程不需要碰命令行。
2. **服务与运行态总览**：10 个服务（daemon/monitor/mail/ticker/sync/…）的存活、启动时长、最近日志；daemon 只暴露"重启"。
3. **实盘只读监控**：持仓、挂单、成交流水、当日盈亏、风控状态；日志实时流（SSE）。
4. **配置管理**：JSON Schema 驱动的表单 + 原始 JSON 双视图；保存前显示 diff、自动备份、一键回滚、全量审计。

### P1 — 决定它是不是"给新手用的"
5. **策略工作台**：模板库（从 `core/app/cta_strategy/strategies` 与 `examples/strategies` 派生）→ Monaco 编辑器 → 保存即静态校验（接口是否实现、参数是否声明）→ 参数表单由 `get_class_parameters()` 自动生成。
6. **回测中心**：提交/排队/进度/中止；结果页给净值、回撤、成交明细；多次结果并排对比。
7. **参数扫描**：矩阵配置 → 并行执行 → 结果表格排序筛选 → **DSR / PBO 过拟合提示**（主仓蓝图 P0 项，在 UI 上呈现比在 CSV 里更有意义）。

### P2 — 纪律产品化，本项目真正的护城河
8. **上线门控（Paper-first gate）**：一个策略实例要从"草稿"走到"实盘"，必须依次穿过
   `回测通过 → 测试网/纸交易观察 N 天 → 人工二次确认 → 启用`。
   每一步的证据（回测 run id、观察期成交数、确认人）都记录在案，UI 上是一条可视化的流水线。
   **这是把 CLAUDE.md 里的三条铁律从"人要记得"变成"系统不允许"。**
9. **告警与通知**：阈值配置 + 渠道（邮件/Bark/Discord/飞书），带静默与去重。
10. **审计与回滚中心**：全量变更时间线，任何配置改动可回到任意历史版本。

### P3 — 以后
11. 多主机管理（agent 模式）；12. 归因报表（对接 2.0 的 unexplained residual）；13. 插件机制。

---

## 7. 页面清单与路由表（交给 readdy 的输入）

| 路由 | 页面 | 关键元素 | 优先级 |
|---|---|---|---|
| `/setup` | 首次运行向导 | 6 步步进器、每步单一焦点、失败可回退 | P0 |
| `/` | 总览 Dashboard | 净值卡、今日盈亏、服务健康栅格、告警条、快捷入口 | P0 |
| `/live` | 实盘 | 持仓表、挂单表、成交流水、风控状态灯 | P0 |
| `/live/logs` | 日志 | 实时流、级别筛选、关键字高亮、暂停/跟随 | P0 |
| `/services` | 服务管理 | 10 服务卡片、重启按钮、Danger Zone 折叠 | P0 |
| `/strategies` | 策略列表 | 实例卡（状态徽章：草稿/回测中/观察中/实盘）、门控进度条 | P1 |
| `/strategies/:id` | 策略详情 | 参数表单（自动生成）、门控流水线、历史回测、实例日志 | P1 |
| `/strategies/:id/edit` | 代码编辑器 | Monaco、校验面板、模板插入、"另存为新策略" | P1 |
| `/backtests` | 回测中心 | 任务队列、状态、耗时、结果入口 | P1 |
| `/backtests/:id` | 回测结果 | 净值曲线、回撤曲线、指标卡、成交明细表、对比加入按钮 | P1 |
| `/backtests/compare` | 结果对比 | 多曲线叠加、指标差异表 | P1 |
| `/sweeps` | 参数扫描 | 矩阵编辑器、进度、结果表、DSR/PBO 警示 | P1 |
| `/config` | 配置中心 | 左侧文件树、右侧表单/JSON 双视图、diff 抽屉 | P0 |
| `/config/history` | 变更历史 | 时间线、diff、回滚 | P2 |
| `/exchanges` | 交易所与密钥 | 掩码展示、权限位徽章、连通性自检、提币权限告警 | P0 |
| `/alerts` | 告警 | 规则列表、渠道配置、测试发送 | P2 |
| `/settings` | 系统设置 | 账号/2FA、语言、主题、运行时路径 | P0 |

**每个页面必须交付四态**：加载中（骨架屏）、空状态（含"下一步做什么"的引导）、错误态（含可执行的修复建议）、正常态。
**新手模式 / 专家模式**：全局开关。新手模式隐藏高级参数、每个字段带一句人话解释；专家模式全展开。这是"界面简洁"和"功能完整"唯一能同时成立的方式。

**设计语言取向**：数据密集但克制——深色为主、单一强调色、图表配色不超过 3 色、无渐变无阴影堆叠、表格优先于卡片。参考坐标：Linear 的密度 + Grafana 的信息层级。

---

## 8. readdy → 代码的落地约定

readdy 产出的是 React + Tailwind。为了让它的输出**可直接使用而不是重画**：

1. 给 readdy 的输入 = §7 的路由表 + 页面元素 + 四态要求 + 设计取向，一次说清。
2. 导出代码原样放入 `apps/web/src/_incoming/<page>/`，**不改**，先提交一次（保留设计原貌，便于日后对照）。
3. 第二次提交做"接线"：拆出 `components/`、去掉 Next 专有引用（`next/link`、`next/image`）、把假数据换成 `api/` 生成的客户端调用、把颜色/间距硬编码替换为 `design/tokens.css`。
4. `_incoming/` 目录在 CI 里排除 lint，但**禁止在生产构建中被引用**（脚本检查），确保它只是暂存区不是长期依赖。

给 readdy 的提示词模板会随本方案一起给出（待评审通过后补 §10）。

---

## 9. 里程碑

| 阶段 | 交付 | 验收标准 |
|---|---|---|
| **M0 骨架** | 仓库、CI、认证、适配器协议、`/` + `/services` 只读 | 干净机器上 `docker compose up` 后能看到真实服务状态 |
| **M1 配置** | `/config`、diff、备份、回滚、审计 | 改一次策略参数并回滚，全程无命令行，备份文件与手工格式一致 |
| **M2 回测** | `/backtests`、任务队列、结果页 | 提交一次回测并看到净值曲线，结果与 CLI 直跑逐笔一致 |
| **M3 上手** | `/setup` 向导、样例数据、示例策略 | **外部新手在干净机器上 30 分钟内看到第一条净值曲线**（找真人实测，不自评） |
| **M4 策略** | 编辑器、模板库、参数自动表单 | 新写一个策略并跑通回测，不打开 IDE |
| **M5 门控** | 上线门控流水线、实盘控制、告警 | 未通过回测的实例无法启用实盘（尝试时被系统拒绝并说明原因） |

M0–M2 是"能用"，M3–M5 是"新手能用"。**M3 的验收必须是外部真人**——自己测的冷启动时间永远是假的（主仓 ROADMAP 撤销 G11 时踩过同一个坑）。

---

## 10. 待定：首版对齐哪个运行时？

这是唯一影响结构的开放问题：

- **选项 A（推荐）先做 `legacy_py` 适配器**：1.x 是今天真正在跑的东西，能立刻自用、立刻收到真实反馈；2.0 适配器等 live path 成熟后按同一协议补。风险：1.x 是私有仓，公开的后台仓要能在**没有私有内容**的前提下自测（用 `examples/` 里的假目录树 fixture，符合主仓 §8 第 3 条）。
- **选项 B 先做 `oq_cli` 适配器**：与公开生态一致，但 2.0 live path 未完成，后台会有很长一段时间无实盘可管。
- **选项 C 两个都做**：工作量翻倍，M0 会拖长。

结构上三者都兼容（协议已抽象），差别只在**先写哪个、M0 何时能自用**。
