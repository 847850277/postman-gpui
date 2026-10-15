# Postman GPUI 使用指南

[项目首页](../README-zh.md) · [English guide](guide.md)

安装、桌面功能、Flow DSL、开发与当前限制。下文命令默认在仓库根目录执行。

## 项目组成

| 组件 | 当前能力 | 入口 |
| --- | --- | --- |
| 原生桌面应用 | HTTP 请求编辑、响应查看、本地历史和会话 Flow 草稿 | `cargo run --locked` |
| Flow DSL 与 CLI | 编译、执行 `.http` 请求和 `.http.yml` 流程 | [postman-g](../crates/postman-cli/README.md) |
| Flow MCP Server | 为 Agent 提供 Schema、示例、校验、结构检查和 YAML 生成 | [postman-flow-mcp](../crates/postman-flow-mcp/README.md) |
| HTML UI 原型 | 独立的 Home、HTTP 和 Flows 界面，使用模拟响应 | [预览说明](../prototypes/README.md) |

原生桌面应用从 Home 进入独立的 HTTP 和 Flows 页面。Flows 已支持原生流程库、
顺序画布和支持原生表达式的 HTTP Request 详情编辑；实际流程执行仍通过 CLI 或 Rust API 完成。

应用、HTML 原型及 CLI/Flow 文档中的可运行 HTTP 示例统一使用
[HTTPBingo](https://httpbingo.org/)。HTML 原型使用对应的本地模拟响应；原生客户端和
CLI 执行请求时会实际访问配置的地址。

## 桌面 HTTP 功能

- 支持 GET、POST、PUT、PATCH、DELETE、HEAD 和 OPTIONS
- 支持查询参数、自定义请求头、Basic/Bearer 认证和 Cookie
- 支持 JSON、Raw、URL-encoded、Multipart 请求体及文件上传
- 支持重定向策略、响应解压、超时和取消
- 展示响应状态、响应头和格式化响应体，并支持快速复制
- 支持多标签页、全局搜索和可回放的 SQLite 历史记录
- HTTP/Flows 共用环境设置，持久化保存并切换匹配请求的基础地址
- 支持请求命名保存、重启后重新打开，以及未保存修改的关闭确认
- 基于 GPUI Kit 的 cURL 导入预览、新建请求和 Code 导出/复制
- 支持跨平台键盘操作、文本选择和剪贴板行为

## 安装

从 [GitHub Releases](https://github.com/847850277/postman-gpui/releases) 下载对应平台的安装包：

| 平台 | 桌面安装包 | 支持范围 |
| --- | --- | --- |
| macOS | 通用架构 `.dmg` 或打包后的 `.app` | Intel 与 Apple 芯片，macOS 10.15.7+ |
| Windows | NSIS `.exe` 安装程序 | Windows 10+，x86_64 |
| Linux | `.AppImage` 或 `.deb` | x86_64，支持 Vulkan 的 Wayland/X11 桌面 |

Release 同时提供 macOS、Windows 和 Linux 的独立 `postman-g` CLI 压缩包。
CLI 安装方式、各平台依赖、预发布未签名提示和 Linux 运行库见[安装指南](installation.md)。
下载后使用对应 Release 的 `SHA256SUMS` 校验文件。

## 从源码运行

仓库通过 `rust-toolchain.toml` 固定 Rust 版本：

```bash
git clone https://github.com/847850277/postman-gpui.git
cd postman-gpui
cargo run --locked
```

Linux 需要先安装[安装指南](installation.md#linux)列出的 GPUI 开发依赖。

GPUI Kit 迁移直接在 `cargo run --locked` 打开的真实应用中逐步进行，
从 Home 进入 HTTP 即可查看请求编辑区。Body 支持 None、JSON、Raw、URL encoded、
Form-data 和 Binary，切换类型保留各自草稿。请求表格、Body 编辑器和响应面板
使用 Kit 滚动条，支持滚轮、拖动滑块和点击轨道。搜索、认证、Options、描述和表格
单元格使用 Kit Input 管理输入法、选区、撤销及原生编辑菜单。JSON/Raw Body、Scripts 和 Tests
使用 Kit Textarea；响应正文使用只读 Kit Editor，支持 JSON 高亮、Pretty 行号和 Raw 换行。
右键菜单可复制选区，响应工具栏复制完整原始正文。使用
`cargo test --locked --test ui_shell --test ui_kit --test ui_layout`
验证页面导航、原生控件交互和布局。

HTTP 和 Flows 共用基于 GPUI Kit 的环境选择器与 **Settings…** 弹窗。首次启动没有预置环境，
通过 **Create environment…** 新建名称和 HTTP(S) 基础地址（可带路径）。名称不能重复，
地址不能包含凭据、查询参数或片段。取消会放弃编辑；保存或切换环境时，先将配置写入本地
`postman-gpui/environments.json`，成功后再更新匹配的请求草稿，保留路径、查询参数顺序及重复项。
其他请求字段、无关地址、正在发送的请求快照和历史记录保持不变。保存失败可以重试；配置损坏时
可修复后重新加载，原文件不会被自动覆盖。Flows 共用环境设置；切换环境不会改写流程步骤中的字面量 URL。
使用 `cargo test --locked --test ui_environments` 验证环境交互、持久化及真实本地 HTTP 行为。

应用默认浅色主题，可在侧栏手动切换暗色并记住选择。首页区分 HTTP 和 Flows；
切换页面保留当前请求草稿、响应和正在执行的请求。原生窗口最小为 768 × 600。

请求与响应区在可用编辑宽度达到 900 逻辑像素时左右排列，较窄时上下排列。
工具栏可固定上下布局或恢复自动布局；拖动分隔线调整比例，双击恢复默认 46%。
分隔线支持方向键、Shift+方向键、Home/End 和 Enter 打开面板尺寸对话框；布局偏好保存到本地。

History 弹窗按本地日期分组，并支持筛选和回放存储的请求、响应，不会自动发送。
单条删除和 **Clear…** 均需确认；清空会包含筛选后未显示的记录，不影响已保存请求或打开的草稿。
全局搜索（Cmd/Ctrl+K）区分当前标签、已保存请求和历史记录，支持方向键与 Enter。
Cmd/Ctrl+Shift+F 打开历史，Cmd/Ctrl+/ 打开快捷键说明，Esc 关闭最上层弹窗并恢复焦点。

**Save / Cmd+S / Ctrl+S** 将命名请求保存到本地 SQLite，可从首页或搜索重新打开。
关闭有修改的请求时，可选择保存、放弃修改或取消。发送请求不代表已保存；未保存的草稿不跨重启恢复。
明确填写的凭据会以未加密形式保存，详见[请求保存说明](saved-requests.md)。

HTTP 侧栏 **Import** 预览内联 POSIX cURL 命令并创建未发送的新标签；**Code** 导出当前有效请求。
这些操作不会执行命令或读取文件。支持范围、文件引用与重定向版本要求见 [cURL 说明](curl.md)。

在本机生成对应平台的安装包：

```bash
cargo install cargo-packager --version 0.11.8 --locked
python3 scripts/release.py package
```

在 macOS 上生成 Intel 与 Apple 芯片通用安装包：

```bash
python3 scripts/release.py package --universal-macos
```

## 原生 Flow 编辑器（P5.2 — 步骤配置）

通过 **Flows → New flow** 创建空白会话草稿，Kit 弹窗验证名称；取消或 Esc 不会创建文档。
可在标题栏重命名、筛选流程库，也可从首页继续真实的会话流程。
**Add step → New HTTP request** 追加并选中新步骤。Kit 详情区即时同步步骤名称、方法、URL、Headers
和 Body；步骤 ID 不随名称变化。每个步骤独立保留文本、错误、选择和撤销历史，切换步骤、流程、页面、
主题及拖拽布局均不会重新初始化编辑器。

URL 和 **Headers · N** 提供 **Literal / Expression** 切换。字面量中的 `{{...}}` 始终是普通文本。
表达式使用原生 JSON 格式，例如 `{"concat":[{"input":"base_url"},{"literal":"/post"}]}`。
字面量 Headers 每行按第一个冒号分隔名称和值，保留顺序、重复项和空值；表达式模式接受有序的
`{name, value}` 文本表达式数组。只有能够无损表示时才允许切回字面量。
可用变量仅展示已声明的输入名称及前面步骤的导出，使用稳定步骤 ID，不显示默认值或运行时敏感值。

**Body expression** 编辑原生 Body JSON 对象：`none`、`json`（带类型的 JSON 表达式）、
`json_template`、`raw`、`url_encoded` 或 `file`（文本表达式）。例如
`{"kind":"json","value":{"literal":{"enabled":true,"count":3}}}` 会保留布尔和数字类型。
清空 Body 或 Headers 会移除对应内容；编辑 `file` 仅修改路径表达式，不读取文件。
无效输入保留原文并在字段旁提示错误，计入会话编辑；文档暂时保留该字段最后的有效值，修正后即时更新。
其他字段仍可编辑。API 引用和循环步骤的请求配置保持只读，名称仍可修改。

固定的 **Request / Checks / Exports** 标签保留各页滚动位置，聚焦标签后可用方向键和 Home/End 切换。
**Checks** 支持多个独立的状态码卡片（100–599），清空只移除该项，无效输入保留最后的有效定义。
其他原生检查以只读定义展示，保持顺序和表达式。多状态码卡片是对原型单卡片的无损扩展。

**Exports** 支持有序的 Name/JSONPath 行、添加/删除和原生 **Sensitive** 复选框。
名称不能为空、步骤内不能重复；路径复用引擎规则，支持 `$`、`$.json.id`、`$.items[0].id`，不支持通配符和过滤器。
未完成或无效的行保留输入和最后有效定义，其他行可继续编辑。有效变更会刷新节点徽标及已打开的后续步骤变量提示，
不重置其文本、撤销记录。重命名或删除导出不会改写已有引用；Sensitive 仅保存敏感标志，不提取或显示响应值。

**Request → Run condition** 编辑原生 `when` JSON，例如
`{"eq":[{"output":{"step":"query-status","name":"status"}},{"literal":2}]}`。
支持全部原生条件运算符和带类型操作数，空白移除条件，无效非空文本保留最后有效条件。
这里只验证语法和结构，不解析引用、不计算条件、不发送请求；API 引用和循环配置仍只读。

点击节点后，方向键和 Home/End 可选择其他节点。流程库、详情区和 Console 均支持拖拽尺寸，
方向键及 Shift+方向键调整、双击重置、Enter 打开数值尺寸弹窗；Esc 撤销本次拖拽。
窗口扩大后恢复偏好尺寸，Flow 布局偏好仅在本次会话有效。窄窗口用 **Choose flow** 替代流程库，
更窄时通过 **Canvas / Details** 切换画布和详情。

详情栏可以继续扩大至超过原先的 480 逻辑像素上限，同时为画布保留空间。底部 **Console** 提供
Logs、Steps、Outputs 页签，可折叠到标题栏或在编辑区内最大化，恢复时保留原有高度。
日志支持级别筛选、复制可见内容、清空和自动滚动；向上回看时暂停自动跟随。

固定的 `flow ›` 输入栏支持 `help`、`clear`、`logs`、`steps`、`outputs`。Enter 或 Cmd/Ctrl+Enter 执行，
上下键浏览历史并恢复未提交的输入，Tab 补全唯一前缀，Esc 清空输入。每个 Flow 独立保留最多 500 条
日志和 50 条命令，均仅在本次会话有效。清空日志不会删除命令历史或文档编辑。
此处不执行系统 Shell；`check`、`run`、`stop` 会明确提示原生能力尚未接入，Steps/Outputs 保持未运行，
Console 命令不会发送网络请求。

步骤详情的 Request / Checks / Exports 都提供**上移、下移、删除步骤**。上移下移改变执行顺序，
保留选中的步骤 ID、未完成草稿和文本撤销历史；画布方向键仍只用于选择步骤。首尾步骤的对应移动按钮禁用。

删除前会显示步骤名称和 ID，取消或 Esc 不改变文档。确认后选择下一步，没有下一步则选择前一步；
删掉最后一步后焦点回到 Add step。删除循环节点会一并删除其中的子步骤，仅清理被删步骤的编辑器、
订阅和布局；Inputs 运行参数及 Console 历史保留，会话内不复用已删 ID。
API 引用和循环节点可整体排序或删除，配置仍只读。变量提示随顺序更新，已有引用不会自动改写；
删除生产者或把消费者移到前面可能留下无效引用，完整编译诊断将在 P5.3 接入。
本批不含拖拽排序和结构操作撤销；文本撤销重做继续可用。

Flow 草稿**不会跨重启保存**。YAML、工具栏 Check、文件 Export、Run 暂不可用。Request 编辑保留文档原有的
条件、检查、导出、请求选项/认证、API 目录、嵌套步骤、输入输出和布局信息。
JSON 结构或 URL 格式提示不等于流程编译成功。后续 P5.2 补充其他检查类型编辑和 HTTP Add to flow，
P5.3 接入编译诊断与文件保存，P6 接入执行。

使用 `cargo test --locked -p postman-gpui --lib app::postman_app::flows` 和
`cargo test --locked --test ui_flows` 验证原生控件、文档保留及布局。

**Inputs** 提供 **Run inputs / Definitions** 两个视图，空流程也可使用。Definitions 可新增、
重命名、删除输入，设置默认值和敏感标记。没有默认值表示必填；JSON `null` 是明确的默认值。
Text 模式保留字符串原样，JSON 模式保留数字、布尔值、null、数组和对象；只有 JSON 字符串
能无损切回 Text。非法名称、重复名称和错误 JSON 会保留草稿及上一次有效定义。重命名或删除
更新变量提示，但不会改写已有引用。

运行参数优先级是：**显式 Override → 所选环境的 base_url（仅同名已声明输入）→ 默认值 → 必填缺失**。
Reset 移除运行覆盖值并恢复继承。Text 的空文本是空字符串，JSON `null`、`false`、`0` 都是有效值；
空白或错误 JSON 会阻止参数准备，不会偷偷回退到旧值。切换环境保留显式覆盖值及错误草稿。
运行参数与文档默认值分开，不改变文档修订，不写入环境、历史或文件，重启后不保留。

敏感值默认隐藏，可显式揭示后编辑；离开编辑上下文会再次隐藏。敏感默认值仍属于文档，
敏感标记用于脱敏，并非加密。返回 Step details 或点击节点可继续原有 Request / Checks / Exports
草稿。本批仅准备输入参数，Check、保存/导出和真实运行仍属后续阶段。

## Flow DSL 与 CLI

无界面 Runner 将 `.http` 文件和原生 `.http.yml` 流程编译为统一的 `postman-flow` 计划，通过共享的
`postman-http` / `postman-request` 传输层执行。未被错误策略处理的传输、断言或循环失败会返回非零退出码。

Flow v1 支持顺序 HTTP 步骤、带类型的输入输出、JSONPath 提取、断言、条件步骤、有界
`for_each` / `repeat_until` 循环及可复用 API 定义。报告保留 JSON 类型，并对标记为敏感的输出脱敏。

在仓库根目录使用内置 API 目录示例：

```bash
# 仅解析和编译，不发送请求
cargo run --locked -p postman-cli -- run \
  crates/postman-flow/examples/flows/httpbingo_catalog.http.yml --check

# 向 HTTPBingo 发送请求，并输出 JSON 报告
cargo run --locked -p postman-cli -- run \
  crates/postman-flow/examples/flows/httpbingo_catalog.http.yml \
  --input client=hello --json
```

安装独立二进制后可直接使用 `postman-g run <文件或目录>`。`--input` 和 `--var` 是别名。
目录会递归发现请求文件并按路径排序执行；同一次运行中的文件共享 Cookie 会话。

详细语法见 [CLI 说明](../crates/postman-cli/README.md)、[Flow v1 规范](../crates/postman-flow/README.md)、
[JSON Schema](../crates/postman-flow/flow-v1.schema.json) 和[流程示例](../crates/postman-flow/examples/flows)。

## 供 Agent 生成 Flow 的 MCP Server

`postman-flow-mcp` 通过 MCP stdio 暴露 Flow v1 Schema、完整参考、已验证示例、编译诊断、结构检查和
规范化 YAML 创建能力。Agent 提交结构化 JSON 文档，服务端编译通过后才写入文件，并将全部文件访问
限制在配置的工作目录中；该服务不会执行网络请求。

```bash
cargo run --locked -p postman-flow-mcp -- --root /absolute/path/to/flow-project
```

六个 MCP 工具和客户端配置见 [MCP Server 说明](../crates/postman-flow-mcp/README.md)。

## HTML UI 原型

在仓库根目录启动预览服务，无需安装前端依赖或构建：

```bash
python3 -m http.server 4173 --bind 127.0.0.1
```

打开[原型页面](http://127.0.0.1:4173/prototypes/request-workspace.html#home)。首页区分 HTTP 请求和
Flows 两个入口，布局自适应窗口大小，默认白色主题，可手动切换暗色。HTTP 界面支持 cURL 导入导出和
环境设置；Flow 界面支持步骤编辑、检查、运行预览和 `.http.yml` 导出。

Send 和 Run preview 使用本地模拟数据，Flow 检查仅包含浏览器端的部分规则，编辑数据只保留在当前页面
会话中。原型尚未连接 Rust 运行时，也不替代原生客户端 E2E 测试。交互方式、支持语法和限制见
[原型说明](../prototypes/README.md)。

## 验证

```bash
cargo fmt -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-targets --all-features
python3 -m unittest discover -s scripts/tests
```

公共 HTTPBingo 场景需要联网，单独运行：

```bash
cargo httpbingo-scenarios
```

HTML 解析器和模型测试需要 Node.js 18 或更新版本：

```bash
node --test prototypes/curl-request.test.cjs prototypes/flow-model.test.cjs
```

桌面请求场景见[测试说明](../tests/cases/README.md)。Vaultwarden、Meilisearch、Qdrant 的依赖准备和
专项测试入口见 [Flow E2E 说明](../crates/postman-flow-e2e/README.md)。这些测试需要按文档配置对应服务，
不能仅凭一次普通 workspace 测试通过就认定服务集成场景已执行。

## 当前限制

- 响应体仍使用缓冲后的文本模型，尚未提供原始字节响应查看、响应原子保存和流式响应的增量进度。
- 原生 Flow 编辑器支持会话内 Request 草稿；实际 Flow 执行请使用 CLI。
- 请求需要显式保存；崩溃或强制退出后不会恢复未保存的草稿。
- 原生 cURL 导入支持文档列出的内联请求；文件与 multipart 命令只支持导出，详见 [cURL 说明](curl.md)。

## 文档

- [CHANGELOG](../CHANGELOG.md) 与[开放问题](https://github.com/847850277/postman-gpui/issues)
- [cURL 导入导出](curl.md)与[请求保存说明](saved-requests.md)
- [编辑器状态与同步约定](editor-state.md)
- [发布操作手册](releasing.md)
- [跨平台冒烟测试清单](release-smoke-test.md)

## 本地数据与隐私

已完成请求的历史记录保存在操作系统本地应用数据目录下的
`postman-gpui/request-history.sqlite3`。已知凭据和 Cookie 在持久化前会被移除；取消请求和传输失败
不会写入历史。

## 许可证

[MIT](../LICENSE)
