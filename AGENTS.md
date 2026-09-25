# AGENTS.md

给在本仓库工作的人和 agent。保持在 200 行以内。

## 这是什么

Quanterdeck 是 OpenQuanter **2.0** 的自托管控制台。它是框架的**消费者**：单向
依赖公开 crate，框架不知道它存在，也不该为它改变。

1.x 不在范围内。

## 本地命令

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo run -p oq-deck-core --example make_fixtures   # 重新生成 run fixture

cd web && npm install && npm run build              # 产物在 web/dist
```

跑起来看：

```bash
export OQ_DECK_RUNS_DIR=/path/to/your/runs
export OQ_DECK_JOURNALS_DIR=/path/to/your/journals   # 实盘对账需要
export OQ_DECK_TICKS_DIR=/path/to/your/ticks         # markout 需要
export OQ_DECK_AGENT_SOCKET=/run/oq-agent/agent.sock # 运维功能需要，经 oq-agent
cargo run -p oq-deck                                 # http://127.0.0.1:8899
```

首次启动没有密码时，终端会打印一次性令牌，用它走 `/setup`。

## 不变量

改动若违反下列任何一条，即便测试通过也不应合入。

1. **不重写上游已有的解析器。** run 文件由 `oq_parity::wire` 读，判定由
   `oq_parity::manifest` 给。控制台自己解析会得到一个与写入方不一致的读法，
   那比读不了更糟。

2. **"无法判断"永远不能渲染成"一致"。** 基准失效时 `passes` 为假、差异为空、
   横幅是琥珀色不是红色——它不是回归，把它画成回归会让人去找一个不存在的 bug。

3. **残差不完整时是 `None`，不是 0。** 分解不完整而报一个零残差，等于宣称
   "全部都解释清楚了"，那正是它绝不能声称的。

4. **读的东西不改运行时。** 控制台是观察者。这是 2.0 的 FR-CORE-7，在这里
   对所有读取路径生效。

5. **能力只报真做得到的。** 报 `available: true` 的东西必须能用；报 false 的
   必须在 `reason` 里写清原因——那句话会原样显示给操作者。

6. **认证是无条件的，不是"非回环才要"。** 回环不是安全边界；理由见
   docs/SECURITY.zh-CN.md。默认只读、默认只听回环，放宽任何一条都必须是显式
   动作且有名字。

   检查顺序不可调换：`Host` → `Origin`（写入）→ 会话 → 写入模式。
   过不了 `Host` 的请求不得走到会话查找。

7. **run id 来自 URL，永远不当路径拼接。** 只在目录列表里匹配，不 join。

8. **`oq-deck-core` 不依赖 web 框架。** 它是控制台里对交易有主张的那部分，
   必须能在不启动服务器的情况下测试。

9. **密码、TOTP secret、会话令牌、一次性令牌永不进日志**，也不出现在任何
   非专门用途的 API 响应里。

10. **已测得为零 ≠ 不可得。** `Attributed::Explained(Cash(0))` 与
    `Attributed::Unavailable` 是相反的事实；接口与界面都必须分开呈现。

## 上游依赖

`Cargo.toml` 把框架**钉在某个 commit** 上，不浮动。读 run 文件的人必须和写它
的人对同一个格式达成一致，而"那天早上的 main"不是一种一致。

升级 = 改 rev + 跑 `make_fixtures` + 看 `git diff examples/fixtures/runs`。
有 diff 说明格式动了，那是要读的，不是要 commit 掉的。

**MSRV 是 1.89，跟上游一致。** 上游用了 `File::try_lock`（1.89 稳定），并如实声明了 1.89；
之前上游声明 1.85 却用了 1.88 的 let-chain，这里只好单独写 1.88，那个问题已经不存在了。

## 提交

- Apache-2.0；每个 commit 需 DCO 签名：`git commit -s`
- commit message 用英文：`type: short description`
- 不得在 commit 或 PR 中署名任何 AI 助手（CI 检查）
- 不提交交易所凭证、行情数据、含实盘参数的策略、部署拓扑
