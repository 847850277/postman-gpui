# Postman GPUI

[![下载量](https://img.shields.io/github/downloads/847850277/postman-gpui/total?v=08dc9a261f6d48efbf347e101b3f9707)](https://github.com/847850277/postman-gpui/releases)

Postman GPUI 使用 Rust 和 GPUI 构建原生跨平台 HTTP 客户端，并提供用于编排可重复执行的
HTTP 流程的 Flow DSL。桌面应用用于交互式请求调试，`postman-g` 用于在命令行检查和执行请求文件与流程。

[下载 macOS、Windows 与 Linux 版本](https://github.com/847850277/postman-gpui/releases) · [安装说明](docs/installation.md) · [English](README.md)

<picture>
  <source media="(prefers-reduced-motion: reduce)" srcset="assets/readme/tour-zh-static.svg">
  <img src="assets/readme/tour-zh.svg" alt="界面导览：首页、HTTP、Body、历史、搜索、环境、cURL、保存、Cookie、快捷键、分屏与 Flows">
</picture>

画面为界面示意，使用示例数据。当前源码已支持 Flow 文件、Check/YAML、真实 Run/Cancel 和详细运行结果。
发布包可能尚未包含这些功能，请以所选版本的标签、发布说明和构建信息为准。候选包验证见[发布流程](docs/releasing.md)。

[使用与开发指南](docs/guide-zh.md) · [Flow CLI](crates/postman-cli/README.md) · [MCP](crates/postman-flow-mcp/README.md) · [更新日志](CHANGELOG.md) · [MIT 许可](LICENSE)
