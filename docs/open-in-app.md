# 工作目录的外部打开方式

源码参考 DSH `c291e7961a515f6d7af9304e7fd1d257929aef26`，许可见 [DSH-LICENSE](third-party/DSH-LICENSE)。这是打开会话工作目录的功能；在终端应用中打开目录不会接管 OMP 会话。OMP 会话续接使用独立的“运行管理 → 复制命令”功能。

## DSH 如何生成列表

列表来自编译期应用目录，再动态筛出执行 Host 上已安装的条目。它不是系统全部应用列表，也没有通用的“扫描所有应用并判断能否打开目录”机制。

- `packages/host/open-in-app/src/catalog.ts`：应用 ID、平台、已知安装名称、启动适配及菜单顺序。
- `resolver.ts`：macOS 检查 `/Applications`、`~/Applications` 下的已知名称，包括部分别名。Finder、Terminal 使用系统位置；Xcode 从 `xcode-select -p` 推导应用包。改名或放在非标准位置的软件一般不会出现。
- Windows 根据每项定义检查 PATH、App Paths、卸载注册表中指向的实际程序、已知安装目录，JetBrains 选择最新版本目录；Linux 根据 PATH、已知脚本及 XDG desktop entry 检测。并非所有 macOS 条目都有其他平台适配。SSH Host 不提供菜单。
- `index.ts`：首次请求时懒加载，Host 插件生命周期缓存结果；缺失可执行程序时重新解析该条目并重试一次。图标按应用缓存；不是每次展开菜单重扫。
- `icons.ts`：macOS 读取 `CFBundleIconFile`，缺失时寻找 Resources 下的 `.icns`，用 `sips` 转 PNG。Windows 提取可执行文件图标，Linux 读取桌面主题图标。取不到图标显示通用图形。
- `packages/client/ui-open-in-app/src/client/controller.ts`：页面级加载一次列表，`dsh.open-in-app.choice` 保存选择。`OpenInAppAction.tsx` 显示分体按钮，主按钮立即打开；箭头列出安装项；选择同时记忆并启动。上次应用不可用时回退第一项，250ms 后显示等待，失败红框和错误提示持续两秒，运行期间拦截重复操作。
- 启动路径来自当前会话摘要的 `cwd`。macOS 通常调用 `open -a <bundle> <cwd>`，Finder 走系统目录打开，Xcode 优先 `xed`，失败后 `open -a`。通过参数数组启动，不拼 shell；外部进程环境去除凭据。

## 本客户端实现

首期遵循项目 macOS 发布范围，完整收录 DSH 的 28 个 macOS 项及别名：访达、Cursor、VS Code、VS Code Insiders、Windsurf、Zed、Sublime Text、Xcode、Android Studio、IntelliJ IDEA、PyCharm、WebStorm、PhpStorm、GoLand、Rider、RustRover、Fork、Sourcetree、GitHub Desktop、Tower、GitKraken、SmartGit、Sublime Merge、Ghostty、Warp、iTerm2、kitty、终端。其他平台返回空列表，不显示不可用入口。

`crates/host-core/src/open_in_app.rs` 负责检测、图标和启动；Tauri 提供 `open_in_app_apps`、`open_in_app_icon`、`open_in_app`。任务标题栏的 `OpenInApp` 使用既有 Radix 菜单与 Tooltip，真实图标按页面缓存，图标失败保留通用图标及原因。应用选择在浏览器本地保存，主按钮使用选中的安装项。

与 DSH 的适配差异：

- 前端只传 `taskId`、目录项 `appId`；Host 通过 `validate_task_roots` 校验目录信任、归档状态与 worktree 身份，打开第一个实际执行根，不接受任意路径、应用包或命令。多目录任务与 OMP 一致，以主执行目录为 cwd。
- Finder 显式指定系统 Finder 应用包，避免默认目录关联被其他软件改变。
- 新增“刷新应用列表”，便于安装/卸载后更新；每次启动也检查缓存应用包是否仍存在，不存在则重新检测。
- 原生命令只继承必要系统环境变量，不继承 OMP/provider 凭据。系统命令最多等待 5 秒，非零退出和超时均报错。
- 图标使用应用缓存目录内的 PNG；不将系统应用文件路径交给前端。菜单操作不要求 OMP 正在运行。

## 验证入口与边界

- `cargo test -p host-core --lib open_in_app`：别名和用户目录发现、缺失应用、参数边界、未知应用 ID、原生命令失败。
- `cargo test -p host-core --lib open_in_app::tests::installed_mac_apps -- --ignored --nocapture`：执行机器上的真实安装检测及图标提取。
- `cargo test -p host-core --lib open_in_app::tests::live_task_launch_and_rejection -- --ignored --nocapture`：创建独立 Git fixture 与隔离任务，真实打开 Finder/已安装 Warp，并验证归档和不存在的任务不能打开。会打开外部窗口。
- `pnpm --filter @cool-pi/desktop exec vitest run src/components/workbench/open-in-app.test.tsx`：选择恢复、失效选择回退、重复点击保护、失败反馈与空目录。
- `OPEN_IN_APP_ONLY=1 pnpm --filter @cool-pi/desktop ui:acceptance`：运行中的 Vite 页面，六种视口/主题、菜单边界、选择与刷新、busy/错误状态截图。应用列表和图标来自明确的 Host 夹具，不代表第三方软件窗口验收。

系统打开命令成功表示 macOS 接受了请求，不证明每款 IDE/终端都已把窗口导航到目标目录；未安装的软件只能验证目录配置。构建也不等同替换用户已安装的客户端。
