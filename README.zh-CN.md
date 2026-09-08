# Quanterdeck

**OpenQuanter 的自托管控制台。密钥留在你自己的机器上。**

[English](README.md) · 中文

> ⚠️ 早期开发中（v0.0.1，M0）。接口不稳定。本项目不构成投资建议，风险自负。

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

M0。已能对一个 run 文件目录做到：

- 列出全部运行记录，**包括读不了的那个**——附上原因，而不是从列表里消失
- 逐条展示身份三元组、成交、已实现盈亏；无 tag 与空 tag 的成交显示不同
- 比对两次运行，并区分三种结论：可比对 / 仅代码变更 / **基准已失效**
- 能力自省：做不到的功能不渲染控件，并说明为什么做不到
- 默认只读、默认只听回环；监听其他地址而没有密码与第二因素时**拒绝启动**

未做：journal 回放、实盘对账、归因视图、参数扫描、数据质量、首次运行向导。

## 上手

```bash
git clone https://github.com/openquanter/quanterdeck
cd quanterdeck

cd web && npm install && npm run build && cd ..   # 构建期才需要 Node
export OQ_DECK_RUNS_DIR=/path/to/your/runs
cargo run -p oq-deck                              # http://127.0.0.1:8899
```

没有自己的 run 文件时，用仓库里的示例：

```bash
export OQ_DECK_RUNS_DIR=$PWD/examples/fixtures/runs
```

这些示例由框架自己的写入器生成（`cargo run -p oq-deck-core --example make_fixtures`），
所以格式一定合法——其中一个是**故意截断**的，用来看列表如何呈现读不了的文件。

## 文档

| 文档 | 内容 |
|---|---|
| [STACK.zh-CN.md](docs/STACK.zh-CN.md) | 技术选型的理由、被否决的方案，以及 v1 错在哪里 |
| [UI-BRIEF.zh-CN.md](docs/UI-BRIEF.zh-CN.md) | 交给设计的页面清单：每屏的数据、状态与判定 |
| [AGENTS.md](AGENTS.md) | 本地命令与八条不变量 |

## 许可

Apache-2.0。贡献需 DCO 签名（`git commit -s`），见 [CONTRIBUTING.md](CONTRIBUTING.md)。
