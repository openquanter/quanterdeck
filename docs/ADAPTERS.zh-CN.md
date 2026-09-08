# 写一个适配器

一个适配器教会控制台如何和某一种运行时对话。本仓库自带两个：`legacy_py`
（OpenQuanter 1.x）与 `oq_cli`（2.0）。第三方不需要 fork 本仓库。

## 契约

实现 `oq_adapters.RuntimeAdapter`（一个 `typing.Protocol`），共 10 个方法。
`oq_adapters` **只依赖标准库**，所以你的适配器不会因为接入而继承 FastAPI 或
其他本项目的技术选择。CI 里 `scripts/check-adapter-deps.sh` 守这条线。

```python
from oq_adapters import Capabilities, RuntimeKind

class MyAdapter:
    def capabilities(self) -> Capabilities:
        return Capabilities(kind=RuntimeKind.LEGACY_PY, version="1.0",
                            services=True, config_read=True)
    ...
```

## 注册

发一个普通的 pip 包，声明 entry point：

```toml
[project.entry-points."quanterdeck.adapters"]
my_runtime = "my_pkg.adapter:MyAdapter"
```

装上即可被发现。`oq_adapters.available()` 列出全部，`load(name)` 取一个。

## 两条必须遵守的规则

**读不改状态。** 任何读取方法对运行中的系统必须零副作用。这是 2.0 里
"journal 读者是观察者" 的同一条规则，在这里对所有运行时生效。

**写不猜测。** `write_config` 收调用方读到的 etag；不匹配就抛 `ConflictError`，
不做合并。适配器不知道操作者想保留哪一版，猜错的代价由对方承担。

## `capabilities()` 要诚实

控制台的导航是从这里渲染的。把一个还没做的能力报成 `True`，得到的不是
"功能待完善"，而是一个按下去就报错的按钮。做不到就报 `False`，并在 `notes`
里写清原因——那句话会原样显示给操作者。

`oq_cli` 适配器现在几乎所有能力都是 `False`，`notes` 写着真实原因：
`oq` 命令的输出是给人看的，控制台需要 `--json` 才能读。这是目前 2.0 适配器
落地的**唯一阻塞项**，也是上游最值得先做的一件小事。

## 危险操作

`ServiceAction.RESTART` 与 stop+start 是**不同的操作**，不是别名。在 1.x 上
restart 是 SIGUSR1，会把挂单交接过去；stop 之后再 start 会撤销全部挂单并丢失
订单认领。适配器**不得**用 stop+start 模拟 restart。

不在 `SAFE_ACTIONS` 里的动作，收到 `confirmed=False` 时必须抛 `RefusedError`，
并在 `reason` 里说明会发生什么——不是"权限不足"，是"这会撤掉你所有挂单"。
