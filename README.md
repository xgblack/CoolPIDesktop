# 酷PI

基于用户本机安装的 [Oh My Pi (OMP)](https://github.com/can1357/oh-my-pi) 的桌面客户端，采用 **Tauri 2 + Rust Host + React/TypeScript**。当前面向 macOS 开发。

## 当前功能

- 项目与任务管理、目录信任、主目录与附加目录、SQLite 元数据持久化。
- OMP 启动与检测、流式对话、取消生成、停止/重启、独立任务进程、原会话继续与分页历史。
- Markdown 消息、工具输出、扩展审批交互、任务草稿隔离。
- 任务详情菜单下载 Session ZIP；详情和侧栏任务菜单复制实际 OMP 会话 ID。
- 浅色/深色/系统主题、可调整侧栏与窄窗口抽屉。
- 模型配置：Provider/模型增删改、认证配置、请求 ID 与显示名称分离、预置参数查询与填充、加载与连接验证。

Git/worktree、文件工作台、集成终端、完整独立网页端和 OMP 安装助手仍属于后续建设范围。当前不提供签名、公证安装包。

## 环境准备

- macOS，已安装 Xcode Command Line Tools（`xcode-select --install`）。
- Rust 工具链（支持 Edition 2024），Node.js 22.12+ 或更新的兼容版本。
- pnpm 10.23.0（版本声明见 `package.json`）。
- 系统安装的 OMP。代码最低接受版本和当前集成基线均为 18.1.20。

在仓库根目录安装依赖并检查工具：

```sh
pnpm install
cargo --version
node --version
omp --version
```

OMP 必须由用户安装；客户端不内置、不静默下载或替换 Agent Runtime。找不到 `omp` 时，可在设置的“外观与运行时”中填写其绝对路径。

## 启动桌面客户端

在仓库根目录执行：

```sh
pnpm --dir apps/desktop exec tauri dev
```

此命令自动启动 Vite、编译 Rust Host 并打开桌面窗口，首次编译耗时较长。结束开发运行时，在启动它的终端按 `Ctrl+C`。

`pnpm dev` 只启动前端开发服务器；直接打开浏览器没有 Tauri Host，不能据此验证完整桌面功能。如果 5173 端口已被另一个前端开发进程占用，请先在该进程的终端停止它，再启动 Tauri。

也可以构建后直接运行，无需单独启动 Vite：

```sh
pnpm --dir apps/desktop exec tauri build --debug
./target/debug/cool-pi-desktop
```

## 手动验证

### 1. 检测 OMP 与已有模型

1. 打开“设置 → 外观与运行时”，点击“检测 OMP”。路径留空时使用已保存路径或 PATH；显式指定路径优先。Finder 启动的 App 可能没有终端的 PATH，可使用“选择 OMP”通过系统文件选择器绑定绝对路径；也可点击“从登录 Shell 查找”进行一次性路径发现。发现或选择的路径仍需通过版本与 RPC 检测后才会保存。
2. 查看版本与检测结果。无模型时，仍可进入模型配置，不必先成功启动任务。
3. 打开“设置 → 模型配置”，选择已有 Provider，核对模型请求 ID、显示名称和参数。
4. 已有密钥只显示“已配置”，不会回显。点击“验证加载”检查 OMP 是否发现模型；再点击所需模型的“验证连接（会产生调用）”检查真实回复。连接验证会调用服务，可能产生费用。

Provider 和模型定义默认编辑 `~/.omp/agent/models.yml`；不存在时使用同目录的 `models.yaml`。仅存在旧 `models.json` 时，需先由 OMP 完成官方迁移。模型用途分配独立编辑 `~/.omp/agent/config.yml` 中的 `modelRoles`；仅当 `config.yml` 不存在且 `config.yaml` 已存在时使用后者。模型目录搜索命中、配置保存、OMP 加载和真实连接是不同结果，应分别检查。

### 2. 验证模型配置编辑

- 新建或编辑 Provider，设置基础地址、API 类型（如 `openai-completions` / `openai-responses`）、认证方式与请求模型 ID。
- 展开“从预置模型填充参数”，查询并预览参数，填充后检查：**请求 ID 与基础地址保持不变**，上下文等元数据得到更新。目录按所选 OMP 版本获取并缓存，不代表网关真实限额。
- 点击“保存配置”，检查自动加载结果；重载设置后确认修改保留。
- 无效 JSON、重复模型 ID、无效数值或外部配置修改冲突应阻止保存，编辑输入应保留供修正。
- 保存会重新排版 YAML，不保留注释；其他 Provider 和未由界面管理的字段会保留。使用真实配置验收时，请仅保存自己确实需要的更改。

“模型用途分配”支持三个 OMP 角色：

- **Tiny**：后台轻量任务，例如会话标题、记忆处理、自动思考判断和异常停止处理；不用于主要对话。未配置时由 OMP 回退到 `smol`。
- **Commit**：生成 Git 提交信息并更新变更日志。
- **Smol**：快速、低成本的轻量任务和子代理分发。

角色可从当前 OMP 发现的模型中选择，也可输入完整的 `provider/model-id` 或带 `:low` 等 thinking 后缀的选择器。暂未被 OMP 发现的已有值会保留并明确提示；清除按钮会删除对应角色 key，而不是写入空字符串。角色文件保留 `default`、其他角色和未知顶层配置。保存后可重载空闲任务，正在执行的任务不会被强制中断；环境变量或 OMP 启动参数可能优先于全局文件。

### 3. 验证真实对话与任务隔离

1. 添加一个可信项目目录，创建任务，点击“继续”加载会话。
2. 选择已配置模型（例如自己的 `coolstudio-llm/qwen3.7-flash`），发送简短消息，检查流式输出与完成状态。
3. 在较长回复期间点击“取消生成”，确认可以继续发送；停止并重新加载任务，确认历史仍在。
4. 创建第二个任务，分别输入不同草稿并切换，确认草稿、消息和模型状态不串到另一任务。
5. 模型配置保存后，可点击“重载空闲任务并刷新模型”。已落盘的空闲任务应继续原会话并刷新模型；正在运行或首次回复尚未落盘的任务会保留原进程，按提示稍后再次应用。

### 4. 下载会话与复制 ID

任务详情右上角的 `…` 菜单提供“下载会话 Session”和“复制会话 ID”；侧栏每个任务的 `…` 菜单也可以复制该任务的会话 ID，无需切换任务。这里复制的是 OMP session ID，与桌面 Task ID 不同。尚未绑定会话时禁用操作，剪贴板不可用时显示 ID 供手动复制。

下载通过系统保存窗口生成 `omp-session-<id>.zip`，包含原始 `session.jsonl`（全部已持久化消息、分支与压缩记录）和其引用的 `blobs/` 图片资源。保存窗口取消不会写文件；已有同名文件不会被覆盖，请换名保存。正在生成、压缩、有排队消息、待审批或由终端接管时，Host 拒绝导出；未落盘、身份不匹配、缺失资源、符号链接或超出大小限制时明确报错，不生成“部分成功”的归档。

参考 DSH 的日志归档方式，当前导出范围为一个已绑定的 OMP 会话，不包含独立子会话、工作区文件和未发送附件；JSONL 上限 64 MiB，原始资源总量上限 256 MiB。此功能需要原生 Tauri Host，浏览器夹具不执行真实保存。ZIP 不会自动导入或迁移运行环境：恢复时需将 `blobs/` 资源放入目标 OMP 的 blob store（默认 `~/.omp/agent/blobs`），准备原工作目录与模型后，再用 `omp --resume /绝对路径/session.jsonl` 读取。

### 隔离配置验收（可选）

如只想练习 Provider/模型增删改，不修改日常 OMP 配置，可从仓库根目录启动一个使用空配置目录的实例：

```sh
OMP_DEMO_DIR="$(mktemp -d /tmp/cool-pi-model-demo.XXXXXX)"
PI_CODING_AGENT_DIR="$OMP_DEMO_DIR" pnpm --dir apps/desktop exec tauri dev
```

在设置中确认配置路径指向该临时目录。此方式只隔离 OMP 配置；桌面客户端的项目/任务数据库仍使用原应用数据目录。不要在该实例中继续日常任务，也不要与另一个桌面实例同时操作同一任务。临时目录会保留；其中若保存了密钥，也应按敏感配置管理。

## 开发验证命令

在仓库根目录执行：

```sh
pnpm typecheck
pnpm test
pnpm build
cargo test --workspace
pnpm --dir apps/desktop exec tauri build --debug
git diff --check
```

`cargo test --workspace` 默认不执行标记为 ignored 的真实 OMP/付费模型测试。Tauri 构建与 Cargo 测试应顺序运行，避免不同构建模式的依赖产物冲突。

模型配置真实验证入口见 `crates/host-core/tests/model_config_live.rs`；浏览器布局与交互夹具见 `apps/desktop/scripts/model-ui-acceptance.mjs`。浏览器夹具使用模拟 Host，不能代替原生窗口与真实模型验收。

## 架构边界

Rust Host 负责本地持久化、OMP 进程与协议访问；OMP session 文件是对话事实来源，SQLite 保存产品元数据。一个活动顶层任务对应一个 OMP RPC 进程。主目录和附加目录保持明确区分，不将其宣传为统一多仓库 IDE 或安全沙箱。
