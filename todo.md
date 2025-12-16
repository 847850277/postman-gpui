# Todo List

## 已完成 ✅
- [x] 一个文件拆分为按模块
- [x] Response 改造为组件 (issue #7)
- [x] 将发送请求逻辑拆分到 http 模块
- [x] **优化和删除重复文件 (issue #9)**
  - [x] 删除 `src/ui/components/body_editor.rs`
  - [x] 删除未使用的 `src/ui/components/text_input.rs`
  - [x] 删除空文件 `src/utils/helpers.rs`
- [x] **统一 Request 模型**
  - [x] 创建 `src/models/request.rs` 统一的 Request 模型
  - [x] 删除重复的 `http/request.rs`
  - [x] 更新 `collection.rs` 使用统一模型
  - [x] 完善 `models/mod.rs` 导出
  - [x] 在 `RequestExecutor` 中添加 `execute_request` 方法支持新模型
  - [x] 为 Request 模型添加完整的单元测试

---

## 代码架构优化

### 高优先级 🔴

#### 1. ~~删除冗余和未使用的模块~~ ✅ 已完成
- [x] 删除 `src/ui/components/body_editor.rs` - 功能已被 `body_input.rs` 完全覆盖
- [x] 检查并删除未使用的 `src/ui/components/text_input.rs`
- [x] 删除空文件 `src/utils/helpers.rs` 或添加实际内容
- [x] 完善 `src/models/mod.rs` - 当前为空文件，需要正确导出模块

#### 2. ~~统一 Request 模型~~ ✅ 已完成
**问题：** `models/collection.rs` 和 `http/request.rs` 中定义了两个不同的 Request 结构体
- [x] 在 `src/models/` 中创建统一的 `request.rs` 模块
- [x] 重构 `http/request.rs`，使用 `models::Request` 或明确区分用途
- [x] 更新所有引用，确保使用统一的 Request 模型
- [ ] 考虑将 `http/response.rs` 也移到 `models/` 下（可选）

#### 3. ~~完善 models 模块~~ ✅ 已完成
- [x] 在 `models/mod.rs` 中添加：
  ```rust
  pub mod request;
  pub mod response;
  pub mod collection;
  ```
- [x] 确保所有数据模型都通过 models 模块导出

---

### 中优先级 🟡

#### 4. 添加统一的错误处理模块
- [ ] 创建 `src/errors/mod.rs`
- [ ] 定义统一的 `AppError` 枚举：
  ```rust
  pub enum AppError {
      HttpError(reqwest::Error),
      ValidationError(String),
      ParseError(String),
      UrlEmpty,
      NetworkError(String),
  }
  ```
- [ ] 为 `AppError` 实现 `Display` 和 `From` traits
- [ ] 更新 `RequestExecutor` 使用统一的错误类型
- [ ] 更新 UI 组件的错误显示逻辑

#### 5. 重组 UI 组件结构
- [ ] 创建子目录结构：
  ```
  ui/components/
  ├── input/           # 输入相关组件
  │   ├── mod.rs
  │   ├── url_input.rs
  │   ├── body_input.rs
  │   └── header_input.rs
  ├── common/          # 通用组件
  │   ├── mod.rs
  │   ├── dropdown.rs
  │   └── button.rs
  └── display/         # 展示组件
      ├── mod.rs
      ├── response_viewer.rs
      └── method_selector.rs
  ```
- [ ] 更新 `ui/components/mod.rs` 的模块导出
- [ ] 更新所有引用路径

#### 6. 添加配置管理模块
- [ ] 创建 `src/config/mod.rs`
- [ ] 定义应用配置结构：
  ```rust
  pub struct AppConfig {
      pub request_timeout: Duration,
      pub default_headers: HashMap<String, String>,
      pub window_size: (u32, u32),
      pub theme: Theme,
  }
  ```
- [ ] 实现配置的加载和保存功能
- [ ] 支持从配置文件（如 `config.toml`）读取

---

### 低优先级 🟢

#### 7. 添加统一的事件系统
- [ ] 创建 `src/events/mod.rs`
- [ ] 定义统一的 `AppEvent` 枚举
- [ ] 替换各组件独立的事件类型（`UrlInputEvent`, `BodyInputEvent` 等）
- [ ] 实现事件总线或发布-订阅模式

#### 8. 添加测试模块
- [ ] 为 `http/executor.rs` 添加单元测试
- [ ] 为 `http/client.rs` 添加集成测试
- [ ] 为 UI 组件添加测试（如果 GPUI 支持）
- [ ] 添加端到端测试

#### 9. 完善文档
- [ ] 为公共 API 添加 rustdoc 文档注释
- [ ] 创建 `docs/` 目录，添加架构文档
- [ ] 更新 `README.md`，添加模块说明
- [ ] 为每个主要模块添加使用示例

#### 10. 性能优化
- [ ] 考虑使用异步任务池处理 HTTP 请求（而不是每次创建新的 Runtime）
- [ ] 优化大响应体的显示（添加分页或虚拟滚动）
- [ ] 添加请求缓存机制

---

## 功能增强

### 新功能
- [ ] 支持更多 HTTP 方法（PUT, DELETE, PATCH 等）
- [ ] 添加请求历史记录
- [ ] 支持保存和加载请求集合
- [ ] 添加环境变量支持
- [ ] 实现请求/响应的导入导出功能
- [ ] 添加代码生成功能（生成 curl 命令、各语言的 HTTP 请求代码）

### UI/UX 改进
- [ ] 添加快捷键支持
- [ ] 实现深色/浅色主题切换
- [ ] 优化响应体的 JSON 格式化显示
- [ ] 添加语法高亮
- [ ] 实现响应体的搜索功能

---

## 技术债务
- [ ] 移除 `PostmanApp` 中的 `println!` 调试输出，使用 proper logging
- [ ] 考虑使用 `tracing` 或 `log` crate 进行日志管理
- [ ] 评估是否需要状态管理库（如果应用变得更复杂）
- [ ] 考虑使用依赖注入来管理 `RequestExecutor` 等服务

---

## 注意事项
- 所有重构应该保持向后兼容或有清晰的迁移路径
- 每次重构后确保所有测试通过
- 及时更新文档以反映代码变更