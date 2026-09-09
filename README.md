# cool-pi-desktop

面向 [Oh My Pi (OMP)](https://github.com/can1357/oh-my-pi) 的桌面客户端（可复用为网页端）。目标是提供接近 Codex App 的项目、任务、流式对话、工具执行、审批、Git/worktree、终端和后台任务体验。

## 关键约束

- OMP 必须使用用户执行主机上系统安装的 `omp`，客户端不内置、不静默下载 Agent Runtime。
- 桌面端与网页端共用 React/TypeScript UI；本机 Node Host 负责进程、PTY、文件、Git、SQLite 和 WebSocket。
- 多目录工作区显式区分 primary root 与 additional roots，不把 OMP 的目录参数误认为完全对等的多仓库 IDE workspace。

## 设计文档

- [OMP 桌面与网页客户端完整架构方案](docs/architecture/omp-desktop-client-design.md)

文档基于 OMP 固定源码提交进行静态评估；本项目尚未包含实现代码、依赖安装或真实模型运行验证。
