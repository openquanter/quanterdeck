# Quanterdeck

**OpenQuanter 的自托管控制台。密钥留在你自己的机器上。**

[English](README.md) · 中文

> ⚠️ 早期开发中（v0.0.1，M0）。接口不稳定。本项目不构成投资建议，风险自负。

---

## 这是什么

OpenQuanter 是一套量化交易框架，但它没有界面。要跑起来，你得改 JSON、记住
`manager.sh` 的用法、知道哪些操作会撤掉挂单。Quanterdeck 是补上这一层的控制台：

> **让没有量化经验的人，在自己的机器上，30 分钟内跑出第一条净值曲线；
> 让有经验的人，永远不会因为点错一个按钮而丢掉挂单。**

后半句和前半句一样重要。这个控制台的一部分职责是**拒绝**。

## 不做什么

- **不做云托管、不做 SaaS、不代管你的 API key。** 密钥加密存在本机，不出现在任何
  API 响应、任何日志、任何 git 提交里。上游框架把"代管用户密钥"列为非目标，
  这里也一样。
- **不做行情终端、不做社交跟单、不做策略市场。**
- **不替你判断策略好坏。** 它只保证你在上线前确实跑过回测，不保证那次回测有意义。

## 三条硬规则

写进代码，不是写进文档。

**1. 读的东西不改运行时。** 任何读取路径对运行中的系统零副作用。持仓的权威来源是
交易所，不是磁盘上的交接文件——所以控制台从不读、更不写 `cta_strategy_data.json`。

**2. 危险操作不是"可选项"，是"要打字确认"。** 守护进程只暴露「重启」（信号交接，
挂单保留）。停止与启动收在危险区里，要输入服务名才能执行，并把后果原文写出来：
撤销全部挂单、丢失订单认领。

**3. 改配置必先备份。** 备份和写入是同一个操作，没有开关能把备份关掉。读到的
etag 与磁盘不符时拒绝写入而不是合并——没有任何代码知道你想要的是哪一版。

## 状态

M0。已能对真实的 OpenQuanter 1.x 仓库做到：

- 读出运行时能力，界面据此渲染（做不到的功能不画按钮，而不是画一个按下会报错的）
- 列出并驱动 `manager.sh` 的十个服务
- 从策略类**自动生成参数表单**——字段来自运行时代码，不是这里维护的 schema
- 读写配置，带备份、diff 与冲突检测
- 只读模式：默认开启，任何写入路由返回 403

未做：回测提交、实盘只读视图、上线门控、首次运行向导、2.0 适配器。
路线见 [docs/BLUEPRINT.zh-CN.md](docs/BLUEPRINT.zh-CN.md)。

## 上手

```bash
git clone https://github.com/openquanter/quanterdeck
cd quanterdeck

# 后端
pip install -e ".[dev]"

# 前端（构建期才需要 Node；部署后的控制台不需要）
cd web && npm install && npm run build && cd ..

# 指向你的 OpenQuanter 仓库
export OQ_DECK_RUNTIME_ROOT=/path/to/openquanter
export OQ_DECK_RUNTIME_PYTHON=/path/to/that/repo/env/bin/python

oq-deck up          # http://127.0.0.1:8899
```

`OQ_DECK_RUNTIME_PYTHON` 是**运行时自己的**解释器，通常不是跑控制台的那个。
控制台从不 import 运行时的代码，理由见 [docs/STACK.zh-CN.md](docs/STACK.zh-CN.md) §3。

默认只监听 `127.0.0.1`，且处于只读模式。要监听其他地址，必须先设好密码和
第二因素，否则拒绝启动——一个能下单的控制台不会带着方便的默认值裸奔在网络上。

## 文档

| 文档 | 内容 |
|---|---|
| [BLUEPRINT.zh-CN.md](docs/BLUEPRINT.zh-CN.md) | 立项方案：定位、范围、页面清单、里程碑 |
| [STACK.zh-CN.md](docs/STACK.zh-CN.md) | 技术选型的理由，以及被否决的方案 |
| [ADAPTERS.zh-CN.md](docs/ADAPTERS.zh-CN.md) | 如何为自己的运行时写一个适配器 |

## 许可

Apache-2.0。贡献需 DCO 签名（`git commit -s`），见 [CONTRIBUTING.md](CONTRIBUTING.md)。
