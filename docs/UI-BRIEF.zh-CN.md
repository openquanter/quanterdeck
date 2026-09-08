# 设计任务书 · Quanterdeck

> 给 Readdy（或任何外部设计）的输入。**每一屏的数据字段都是真实的**——来自
> 已实现并测试过的 API，不是设想。
> 状态：v2，2026-09-08。v1 曾写"不要设计登录流程"，那是错的——理由与更正见 §5。

---

## 0. 一句话

给量化交易框架 OpenQuanter 用的自托管控制台。用户在自己机器上跑，浏览器打开
`127.0.0.1:8899`。它展示交易系统的运行结果、实盘与模型的差额、以及"这个结论
还成不成立"的判定。

**它的一部分职责是拒绝。** 很多屏的重点不是"显示数据"，而是"清楚地说明为什么
这个结论不能下"。

---

## 1. 设计语言

- **深色为主**，数据密集但克制。参考坐标：Linear 的密度 + Grafana 的信息层级。
- **单一强调色**，图表配色不超过 3 色。无渐变、无阴影堆叠、无圆角滥用。
- **表格优先于卡片**。这是给人纵向比较数字的界面。
- **数字用等宽 + tabular-nums**。哈希、价格、时间戳都要能对齐。
- **中文为主，英文为辅**。术语（run / journal / parity / residual）保留英文，
  不硬译。

现有 token（可改，但请保持"一个语义一个色"的结构）：

```
ground #0b0d10   surface #14171c   surface-raised #1b1f26   line #262b33
ink #e6e9ee      ink-muted #949bab
accent #4c8dff   good #2fbf71      warn #e0a33e             bad #e5484d
```

**颜色语义是硬约束，不是偏好：**

| 色 | 含义 | 绝不能用于 |
|---|---|---|
| good 绿 | 结论成立且通过 | 任何"无法判断"的情形 |
| bad 红 | 结论成立且失败（真的有回归） | 基准失效 |
| **warn 琥珀** | **无法判断——结论不成立** | 不能省略、不能降级成灰 |

理由见 §4 第一屏。这是整个产品最重要的一条视觉规则。

---

## 2. 全局结构

左侧固定侧栏（w-52）+ 右侧主区。侧栏顶部是产品名与版本。

**侧栏导航是动态的**：后端返回一份"能力表"，做不到的功能**不渲染入口**。所以
请设计成"条目数量可变"的列表，不要按固定 8 项排版。

导航项：总览 / 运行记录 / 归因 / 实盘对账 / Journal / 参数扫描 / 数据质量 / 设置

---

## 3. 每屏必须交付四态

| 态 | 要求 |
|---|---|
| 加载中 | 骨架屏，保持最终布局的形状，不要转圈 |
| 空 | 说明"下一步做什么"，不是"暂无数据" |
| 错误 | 说明**怎么修**，附机器细节（可折叠） |
| 正常 | — |

另有一个全局开关：**新手模式 / 专家模式**。新手模式隐藏高级字段、每个术语带一句
人话解释；专家模式全展开。请设计这个开关的位置与两种密度下的对照。

---

## 4. 页面清单

### 4.1 `/runs/compare` 对比（**最重要的一屏，请先做这个**）

比较两次运行的结果。有三种结论，**视觉上必须截然不同**：

| verdict.status | 含义 | 视觉 |
|---|---|---|
| `comparable` | 代码、数据、配置都一致，差异是行为差异 | 绿 / 红（看 `passes`） |
| `code_changed` | 只有代码变了，数据与配置未变——这正是 parity 想测的 | 绿 / 红 + 一行说明 |
| `invalidated` | **数据或配置变了。关于引擎的任何结论都不成立** | **琥珀**，附 `changed[]` 里的整句说明 |

`invalidated` 时后端强制 `passes=false`、`differences=0`。设计上**不能**让它看起来
像"没有差异 = 通过"。它应该看起来像"这个比较没做成"。

数据字段：
```
baseline, candidate            : string（run id）
verdict.status                 : comparable | code_changed | invalidated
verdict.conclusive             : boolean
verdict.changed                : string[]（整句中文说明，如"输入数据不同：该基准
                                  描述的是另一个实验，必须 rebase"）
passes                         : boolean
differences                    : number
first_divergence               : number | null（第几笔成交开始分歧）
matched_prefix                 : number
fill_counts                    : [基准成交数, 待测成交数]
pnl                            : [基准盈亏, 待测盈亏]
pnl_relative_error             : number | null（null = 基准盈亏为零，无法算相对误差）
```

交互：两个 run 选择器（下拉），选完自动比对。

---

### 4.2 `/runs` 运行记录列表

一个目录里所有 run 文件的表格。

**特殊要求：读不了的文件也要在表里**，占一整行，显示错误原因，并且**不计入合计**——
合计旁边要明确写出"N 个未能读取，未计入"。一个悄悄排除了文件的合计是读者无法核对的。

字段：
```
entries[]:
  state = "read":
    id, path, pnl, fills
    identity.code_commit / data_hash / config_hash / label
  state = "unreadable":
    id, path, error（英文技术描述，如 "the body hashes to 9d6c… and the file declares e8…"）
total_pnl : number
```

`identity` 三元组在列表里截断显示（10 字符），详情页显示全长。

---

### 4.3 `/runs/:id` 运行详情

上半：身份三元组（**全长显示**，可复制）+ 档位 + 已实现盈亏。
下半：成交明细表。

```
identity.code_commit / data_hash / config_hash : string（长哈希）
identity.label : string（保真档位，如 "L0"）
pnl   : number
fills[] : { ts(纳秒), symbol, side("buy"|"sell"), price_ticks, qty_lots, tag }
```

**`tag` 有三种状态，必须看得出区别**：`null`（无 tag，显示 `—`）、
`""`（空字符串 tag，显示 `""`）、有值。这在 run 格式里是被刻意区分的。

价格与数量是**整数 ticks / lots**，不是小数——请不要设计成带货币符号的样式。

---

### 4.4 `/` 总览

三个指标块 + 一个"本 deck 暂不支持"清单。

后端会返回每个能力的 `{available, reason}`。**`reason` 是给人看的整句**，要原样
显示，例如："归因需要一份实盘结果来与 run 对比，而这个 deck 还没有被给予可读取
的账户。" 请设计这个清单的样式——它不是错误，是诚实的边界说明。

---

### 4.5 `/attribution` 归因（**后端已实现**，产品的核心）

实盘盈亏减模型盈亏 = 差额，差额分解到五个成因，剩下的是**残差**。

```
live_run, model_run  : string
live_pnl, model_pnl  : number
gap                  : number   ← live - model，只由这两个数算出
components[] : {
  name        : "slippage" | "queue position" | "latency" | "funding vs model" | "fee tier"
  observed    : boolean   ← false 表示这个数需要对"某物值多少"作判断，不是纯观测
  amount      : number | null
  unavailable : string | null   ← 不可得的原因，整句，原样显示
}
residual        : number | null   ← 关键
residual_share  : number | null
method          : "run-files"     ← 证据来源。将来的 "shadow" 是更强的来源
missing_inputs  : string[]        ← 要让不可得的成因变得可得，还缺什么
matched_fills, unmatched_fills : number
```

**三种状态必须视觉上分开**，这是本屏的全部难点：

| 状态 | 例子 | 含义 |
|---|---|---|
| 已测得非零 | `fee tier = 1.5` | 这个成因贡献了 1.5 |
| **已测得为零** | `queue position = 0.0` | 确实测了，结果是零 |
| **不可得** | `slippage: 至少一笔匹配成交没有成交时的市场价` | 没测，不是零 |

后两者绝不能长得一样。真实返回中这两者会同时出现在一屏上。

**`residual` 为 `null` 时绝对不能显示成 0。** 任一成因不可得，残差就是"未知"。
一个由不完整分解算出的零残差等于宣称"全都解释清楚了"，那是这个产品最不能撒的谎。

`missing_inputs` 要显眼——它是"怎么才能看到完整答案"的操作指引，不是脚注。

建议形态：瀑布图（live → 各成因 → model），不可得的成因画成虚线/斜纹段，
残差单独一段并可标记为未知。

**`residual` 为 `null` 时绝对不能显示成 0。** 任一成因不可得，残差就是"未知"。
一个由不完整分解算出的零残差等于宣称"全都解释清楚了"，那是这个产品最不能撒的谎。
请为"残差未知"设计一个独立的视觉状态。

建议形态：瀑布图（live → 各成因 → model），残差单独一段并可标记为未知。

---

### 4.6 `/live` 实盘对账（**后端已实现**）

进程以为自己持有什么（从它自己的 journal 重建）vs 交易所实际持有什么
（操作者粘贴 `oq-recon --record` 的输出）。

```
GET  /api/v1/journals              → journal 列表，读不了的也在列表里带原因
GET  /api/v1/journals/:id/belief   → 该进程相信自己持有什么
POST /api/v1/journals/:id/reconcile{ venue_record } → 两边的差别
```

belief 字段：
```
symbol, position_lots, entry_ticks, resting[]  : 持仓与挂单
price_scale, qty_scale
adopted     : boolean  ← 这次运行接手了一个不是它开的仓
hedged      : boolean  ← 账户同时持有多空两腿；净额会把其中一腿藏起来
undecodable : number   ← journal 里解不开的帧数
```

对账结果：
```
believed, venue : 两边各自的账户快照 { symbol, read_at_ms, legs[], orders[] }
differences[]   : 每条一句话，来自框架自己的比较
agrees          : boolean
undecodable     : number
hedged          : boolean
```

**三个设计要点：**
- `undecodable > 0` 时必须显著警告：**从一份有解不开的帧的 journal 重建出来的
  belief 是有洞的，即使 `agrees=true` 也可能只是碰巧对上**
- `hedged=true` 时不能只显示一个净持仓数字——净额会藏掉一条腿
- `venue_record` 是操作者**粘贴**进来的文本（控制台不持有任何交易所凭证，
  将来也不会）。请设计这个粘贴框和它的说明

---

### 4.7 其余（先占位，出稿顺序靠后）

| 路由 | 页面 | 里程碑 |
|---|---|---|
| `/journal` | journal 回放：一次运行按发生顺序的决策序列 | M1 |
| `/sweeps` | 参数扫描结果表 + **DSR / PBO 过拟合提示** | M3 |
| `/data` | 数据质量：capture → ingest → 特征化，book-check / trade-check 的 break | M3 |
| `/strategies` | 策略列表与上线门控流水线（见 §5） | M4 |
| `/settings` | 只读/写入开关、路径、语言、主题 | M1 |
| `/setup` | 首次运行向导（6 步步进器，每步单一焦点，失败可回退） | M4 |

---

## 5. 认证与安全（必须设计，不能跳过）

**这个控制台需要登录，无条件需要**，不是"只在非回环监听时才需要"。理由：回环
不是安全边界（同机其他进程/用户都能访问）、DNS rebinding 能让操作者浏览器里的
恶意页面访问 `127.0.0.1`、`ssh -L` 一转发"回环"就名存实亡、开了写入之后它能下单。

### 5.1 `/setup` 首次运行（3 步）

deck 第一次启动、还没有密码时，会在**启动它的那个终端**里打印一个一次性令牌。
这是唯一的引导入口。

| 步 | 内容 |
|---|---|
| 1 | 粘贴一次性令牌。文案要说明它在哪（终端里）、为什么这样设计（读它需要本机访问权限）、以及重启即失效 |
| 2 | 设置密码。**规则只有长度（≥12）**，不要设计"必须含大写/数字/符号"的复选框——那种规则产出的是写在便签上的密码。请设计一个鼓励"用一句只有你记得的话"的输入框 |
| 3 | 展示生成的 password hash 与 TOTP secret（含二维码），并明确：**这两样不要截图、不要粘进聊天、不要提交进 git** |

### 5.2 `/login`

- 密码框；**当 deck 启用了 TOTP 时**多一个 6 位验证码框（后端会告诉前端要不要显示）
- **失败提示必须含糊**："密码或验证码不正确"——不能区分"密码错"和"验证码错"，
  那是攻击者用来定位的
- 5 次失败锁定 15 分钟。锁定状态要有独立的界面，显示还剩几分钟
- 不要"记住我"，不要第三方登录

### 5.3 会话状态

- 空闲 1 小时、绝对 12 小时过期；进程重启即全部失效
- 会话过期要有一个**不丢失当前上下文**的重新登录方式（模态框，而不是跳走）
- 右上角需要一个能看到"当前会话/登出"的位置

### 5.4 拒绝页

两种非常规拒绝需要专门设计，因为它们不是用户的错、但需要用户理解：

| 情形 | HTTP | 界面要说什么 |
|---|---|---|
| Host 不在白名单（DNS rebinding 防护触发） | 421 | 列出本 deck 应答的名字；若在用反向代理，提示加进 `OQ_DECK_EXTRA_HOSTS` |
| 跨站写入被拒（Origin 不符） | 403 | 说明这是防跨站请求的保护，不是操作失败 |

### 5.5 写入模式

deck 默认**只读**。开启写入是个显式动作，界面上要有明确的模式指示（不是一个
不起眼的开关）。危险操作在写入模式下仍需二次确认。

---

## 6. 上线门控流水线（`/strategies/:id` 的核心组件）

五个阶段，只能一步步走，不能跳：

```
draft → backtested → observing → confirmed → live
```

每一步不可用时，后端返回**一句具体的原因**，例如：
- "还没有为这套配置跑过回测"
- "72 小时观察窗还剩 48 小时"
- "观察期产生 0 笔成交；至少需要 1 笔才能说明这个策略做过任何事"
- "没有人签字确认"

**请设计成"带原因的流水线"**，而不是一排灰掉的按钮。原因就在控件旁边，不在
tooltip 里。

另有一个状态：配置变更会**作废已有证据**，把实例打回 draft——包括已经在跑实盘的。
这个"回退"需要一个明确的视觉表达。

---

## 6.1 交付形式

- React + Tailwind（v4，用 CSS 变量 token）
- 每屏交付四态
- 产出原样放入 `web/src/_incoming/<page>/`，不要预先适配我们的代码结构

## 7. 不要做的事

- 不要设计**注册**流程（单操作者自托管，没有多用户，没有"忘记密码"邮件）
- 不要设计"实时行情"面板（不是行情终端）
- 不要用红绿表示涨跌（红绿在这里表示"结论成立且通过/失败"）
- 不要把 `invalidated` 画成红色或灰色
- 不要在任何界面上回显密码、TOTP secret 或会话令牌（一次性令牌与 secret 只在
  §5 的两个专门位置出现，且带"不要截图、不要粘进聊天"的提示）
