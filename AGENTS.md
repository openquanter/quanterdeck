# AGENTS.md

给在本仓库工作的人和 agent。保持在 200 行以内。

## 本地命令

```bash
pip install -e ".[dev]"      # 后端
pytest                       # 全部测试，约 3 秒
ruff check src tests         # lint
cd web && npm run build      # 前端，产物落进 src/oq_deck/web/
scripts/check-adapter-deps.sh  # oq_adapters 是否混入了第三方依赖
```

跑起来看：

```bash
export OQ_DECK_RUNTIME_ROOT=/path/to/openquanter
export OQ_DECK_RUNTIME_PYTHON=/path/to/runtime/env/bin/python
oq-deck up
```

## 不变量

改动若违反下列任何一条，即便测试通过也不应合入。

1. **`oq_adapters` 只用标准库。** 它是外部扩展点，不该把本项目的技术选择
   强加给第三方。CI 检查。

2. **服务端进程不 import 运行时。** 1.x 的内省一律走 `probe.py` 子进程，用
   运行时自己的解释器执行。理由：版本解耦 + 故障隔离，见 docs/STACK.zh-CN.md §3。

3. **probe 的结果从文件读，不从 stdout 读。** 1.x 在 import 时会往 stdout 打
   横幅。这个坑已经踩过一次。

4. **restart 永远不能实现为 stop + start。** 两者对挂单的后果不同。

5. **配置写入必然伴随备份。** 备份与写入是一个操作，`backup=False` 只给测试用，
   服务端任何路由都不得传。

6. **`capabilities()` 只报真的做得到的。** 报 True 的能力必须能用。

7. **不读也不写 `cta_strategy_data.json`。** 那是重启交接文件，不是权威源；
   持仓的权威是交易所。

8. **默认只读、默认只听回环。** 放宽任何一条都必须是显式动作，且有名字。

## 提交

- Apache-2.0；每个 commit 需 DCO 签名：`git commit -s`
- commit message 用英文：`type: short description`
- 不得在 commit 或 PR 中署名任何 AI 助手（CI 检查）
- 不提交交易所凭证、行情数据、含实盘参数的策略、部署拓扑
