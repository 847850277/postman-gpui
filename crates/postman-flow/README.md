# postman-flow

原生流程文档使用 .http.yml。文件和未来 GUI 都生成格式无关的 FlowDefinition；
编译得到不可修改的 FlowPlan，然后注入 transport 和会话输入执行。

~~~rust
pub fn parse_flow_yaml(source: &str) -> Result<FlowDocument, DocumentError>;
pub fn write_flow_yaml(document: &FlowDocument) -> Result<String, DocumentError>;

pub fn compile_flow(
    source: &FlowDefinition,
    api_catalog: &ApiCatalog,
    environment: &CompileEnvironment,
) -> Result<FlowPlan, Vec<Diagnostic>>;

pub fn execute_flow<T: HttpTransport>(
    plan: FlowPlan,
    transport: T,
    session: FlowSessionEnvironment,
) -> Result<impl Stream<Item = Result<FlowEvent, FlowError>> + Send, FlowError>;
~~~

## 运行 YAML

产品入口是 `postman-g`（crate：`postman-cli`）。`.http` 与 `.http.yml` 都编译为同一份 FlowPlan：

~~~sh
cargo run --locked -p postman-cli -- run \
  crates/postman-flow/examples/flows/httpbingo_catalog.http.yml --check

cargo run --locked -p postman-cli -- run \
  crates/postman-flow/examples/flows/httpbingo_catalog.http.yml \
  --input client=hello
~~~

`--input` 与 `--var` 等价。对象、数组、布尔、null、带引号的 JSON 字符串，以及无前导零的整型
会保留 JSON 类型；`00123` 仍是文本。未声明的输入会被忽略。`--check` 只编译，不发请求。

`run_yaml` 示例仍可用于直接驱动本 crate 的 API，行为与上面的 `postman-g run` 相同。

examples/flows 中有 6 份文档：

- httpbingo_minimal.http.yml：两步 UUID 提取和复用。
- httpbingo_order_chain.http.yml：三步订单场景回显。
- httpbingo_multi_hop.http.yml：五步级联和跨步骤引用。
- httpbingo_dynamic_chain.http.yml：8 步，多个 ID 来自独立 /uuid 响应。
- httpbingo_json_values.http.yml：特殊字符和所有 JSON 类型的跨请求传递。
- httpbingo_catalog.http.yml：API catalog、具名返回值和画布坐标。

前五份文档有与 Rust 构造方式的编译结果等价测试。HTTPBingo 提供回显和 UUID，
这些场景验证编排，不执行真实订单、支付、清算或履约业务。

## YAML v1 文档

~~~yaml
schema_version: 1
flow:
  name: uuid
  inputs:
    - name: host
      default: https://httpbingo.org
  steps:
    - id: seed
      request:
        kind: http
        method: GET
        url:
          concat:
            - input: host
            - literal: /uuid
      checks:
        - kind: status
          equals: 200
      exports:
        - name: id
          path: $.uuid
  outputs:
    - name: id
      value:
        output: { step: seed, name: id }
editor:
  nodes:
    seed: { x: 100, y: 150 }
~~~

顶层字段为 schema_version、flow、可选 apis 和可选 editor。
flow 包含 name、inputs、steps、outputs。步骤的 name 可省略，默认采用 id。
步骤顺序就是执行顺序；引用只能指向更早步骤声明的输出。
Headers 使用 name/value 列表，保留顺序和重名 header。

未声明 default 的输入为必填；default: null 表示默认值为 JSON null。
运行输入覆盖默认值，包括显式 null。sensitive: true 可用于输入和节点输出，
流程返回值继承来源的敏感标记，其 Debug 输出会遮盖内容。
文档写回保留显式声明的默认值；运行会话输入不会被写入文档。

editor.nodes 按稳定步骤 ID 保存坐标，不参与编译。草稿可以保存为空步骤或
未完成的引用；运行前必须通过编译。文档保存定义、catalog 和布局，
不保存 Plan、运行输出、选择状态或撤销历史。

## 引用与 JSON

文本表达式使用单一明确的分支：

~~~yaml
literal: ordinary text
~~~

~~~yaml
input: host
~~~

~~~yaml
output: { step: seed, name: id }
~~~

~~~yaml
concat:
  - literal: 'Bearer '
  - output: { step: login, name: token }
~~~

字符串不会隐式解释 {{...}} 或其他插值语法。concat 只拼接明确的文本表达式，
不进行 URL 编码；需要编码的值由调用者提供。

结构化 JSON body：

~~~yaml
body:
  kind: json
  value:
    object:
      order_id: { output: { step: order, name: id } }
      quantity: { input: quantity }
      enabled: { literal: true }
      optional: { literal: null }
      label:
        string:
          concat:
            - literal: 'Order: '
            - output: { step: order, name: id }
      items:
        array:
          - literal: 1
          - literal: "00123"
      ordinary_data:
        literal: { input: this-is-data-not-a-reference }
~~~

JSON 表达式分支为 literal、input、output、object、array 和 string。
输入／输出引用保留 JSON 类型；string 显式把文本拼接结果编码为 JSON 字符串。
普通对象数据用 literal 包裹，避免和表达式关键字冲突；对象键不执行插值。

其他 body 模式：

~~~yaml
body:
  kind: json_template  # 也支持 raw、url_encoded
  value:
    literal: '{"client":"already escaped"}'
~~~

json_template 保留原始文本模板语义，不自动为插入值增加 JSON 引号或转义。
静态非法 JSON 在编译时拒绝，依赖运行输入的非法 JSON 在发请求前形成步骤失败。
body 可省略，或写 kind: none。

## 检查与返回值

~~~yaml
checks:
  - kind: status
    equals: 200
  - kind: jsonpath
    path: $.json.quantity
    equals: { input: quantity }
~~~

jsonpath 的 equals 是类型明确的 JSON 表达式，字符串 "123" 与数字 123 不同。
`.http` 的 `@assert jsonpath` 在编译时降成同一种检查。

JSONPath 支持根 $、点分隔对象键和非负数组索引，如 $.items[0].id，
不支持通配符或过滤表达式。节点 exports 提取值，流程 outputs 声明返回哪些
输入或节点输出。

响应上的所有检查都会执行；任一检查或输出提取失败则停止流程。
每一步的输出只在该步所有检查与提取都成功后一起提交。
最终事件为 FlowFinished { success, outputs }，失败时 outputs 为空。
HTTP 错误状态需通过显式状态检查判断；transport 失败是正常的失败运行结果。

## API catalog

可在文档顶层 apis 声明复用请求，也可以由宿主向 compile_flow 传入独立 catalog。
示例见 httpbingo_catalog.http.yml。

API 定义的 parameters 是局部名称；定义内的 input 引用这些参数。
调用步骤使用 request: { kind: api, api: ID, bindings: ... }，bindings 是调用方作用域的
文本表达式。编译时只替换一次，保留调用方的运行输入／步骤输出引用。
结构化 JSON 中的单独引用保留类型，复合文本绑定显式产生 JSON 字符串。
缺参、多余参数、未知 API、定义内非法跨步骤引用均产生编译诊断。

Plan 拥有展开后的请求快照；修改源定义、catalog 或布局不会改变已编译的 Plan。
同一个 Plan 可 clone 后使用不同会话运行，变量绑定与输出彼此独立。

## 错误、保存和当前范围

- DocumentError：YAML 语法错误提供行列；版本和结构错误提供文档字段路径。
- Vec<Diagnostic>：一次返回编译发现的错误，包含归一化定义模型的字段路径、
  步骤 ID，以及适用时的 API ID。当前没有逐 token 的编译错误到原文行列映射。
- FlowError::InvalidInputs：缺少必填输入或传入未声明输入。
- 事件流中的 InvariantViolation：已编译计划的内部约束遭到破坏。

未知字段、未知版本、重复映射键、自定义 YAML tag、非字符串映射键及非有限数值会被拒绝。
YAML merge key 不会被展开；在 literal JSON 对象中 << 只是普通键。
文档限制为 2 MiB、96 层结构嵌套，编译器限制 JSON 表达式最多 64 层，
并允许通过 CompileEnvironment::max_steps 限制步骤数。

write_flow_yaml 使用规范化格式输出，并验证输出可再次读取。
它保留定义和布局语义，不保留原注释、空白、键顺序或原数字写法。
GUI 若需要保留用户手写的原文格式，需要另行保存语法树／源码映射。

首版执行顺序 HTTP 流程。分支、循环、并行、重试、API 类型 schema、任意精度数值、
完整 JSONPath，以及 GUI 接入不在本次实现内。`postman-g`（`postman-cli`）已作为 `.http` / `.http.yml`
的无界面宿主。

执行返回静态分派的 impl Stream，没有为流额外分配 Box，也不要求 transport 为 'static。
宿主自行选择运行时，用 std::pin::pin! 配合 .next() 或直接消费 .try_collect()。
创建流不发请求；停止轮询不再推进后续步骤，丢弃整个流会释放本地执行状态，
但无法撤销已发出的 HTTP 操作。

## 验证

~~~sh
cargo test --locked -p postman-flow --all-targets
cargo clippy --locked -p postman-flow --all-targets -- -D warnings
cargo fmt -p postman-flow -- --check
~~~

普通测试全部离线。运行 run_yaml 示例才会访问指定文档中的 HTTP 服务；
静态检查使用 --check。
