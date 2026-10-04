# Omarchy Child Protect Guardian 🛡️

**🌐 语言：** [English](README.md) · [Polski](README.pl.md) · [Español](README.es.md) · [Deutsch](README.de.md) · [Français](README.fr.md) · **中文**

> 面向 **Omarchy Linux**（Arch + Hyprland）的家长控制与儿童上网安全系统，
> 核心功能是**远程批准应用安装**——家长在手机上通过推送审批放行，
> 就像确认一次 2FA 登录一样简单。

**状态：** 🌱 概念草案（第 0 阶段——收集创意）。见 [`AGENTS.md`](AGENTS.md)。

宏大目标：比 Microsoft Family Safety、Apple Screen Time、Google Family Link 和 Qustodio
**更方便、更有效**，同时保持**隐私**（无遥测，不把孩子的数据上传到企业云）。

---

## 为什么在 Linux/Omarchy 上能做得更好

竞品在 Windows/macOS/Android 上**做不到**、而我们能做到的：

1. **Zero-Bypass（真正无法绕过）。** 在 Windows/macOS 上，孩子可以在任务管理器里结束进程或重置权限。
   而这里守护进程由 Polkit 保护、孩子没有 `sudo`、cgroups v2 让它无法被杀死。网络过滤在内核层
   （`nftables`/eBPF）完成，免费 VPN、代理和 Tor 都绕不过策略。
2. **合成器层级的控制（Hyprland/Wayland）。** 我们可以*冻结*进程（`SIGSTOP`）、对未授权窗口加模糊、
   阻止录屏/共享屏幕——无需在浏览器里注入侵入式扩展。
3. **零开销、100% 隐私。** 没有臃肿软件和遥测；策略在本地离线执行。云端**仅用于转达**家长的决定。
4. **全系统过滤，而非仅限某个浏览器。** 一套 DNS/网络策略覆盖每一个应用（游戏、启动器、即时通讯），而不只是 Chrome。

## 产品支柱

- **远程安装批准** —— `pacman`/`yay`/`flatpak` 的安装请求被挂起；家长收到带有可读应用说明的推送，
  点击*允许 / 拒绝*。
- **时间预算与作息** —— 通过 cgroups 冻结 + Hyprland 空闲强制执行。
- **全系统内容过滤** —— DNS + eBPF，强制 SafeSearch / 受限模式。
- **给家长的可读报告** —— 孩子做了什么、申请了什么（没有企业式监控）。
- **加密签名的审批** —— 即使推送代理被攻破，也无法伪造“允许”。

## 文档

| 文档 | 内容 |
|---|---|
| [`docs/CONCEPT.en.md`](docs/CONCEPT.en.md) | 完整概念、优势、使用场景、非显而易见的点子 |
| [`docs/ARCHITECTURE.en.md`](docs/ARCHITECTURE.en.md) | 架构：守护进程、拦截点、推送审批、安全性 |
| [`docs/ROADMAP.md`](docs/ROADMAP.md) | 构建阶段（MVP → v1），拆分为可验证的小步骤 |

---

## 👤 作者

**由 [wasyleque](https://github.com/wasyleque) 创建。**

## ❤️ 支持项目

如果 Omarchy Child Protect Guardian 对你有帮助，欢迎通过 **PayPal** 支持开发：
**[wasyl@o2.pl](https://www.paypal.com/donate/?business=wasyl@o2.pl&item_name=Omarchy+Child+Protect+Guardian)**

每一份支持都帮助它保持免费、私密与开放。

## 🤝 参与贡献 & 分享创意

本项目依靠社区创意成长。**我们诚挚邀请你：**
- 💡 **分享创意** —— 用 `idea` 标签新建一个 [Issue](../../issues)（已有模板），
- 🛠️ **共建功能** —— 从[路线图](docs/ROADMAP.md)中挑一项并提交 PR，
- 🌍 **翻译文档** —— 支持更多语言。

见 [`CONTRIBUTING.md`](CONTRIBUTING.md)。再小的点子也不嫌小——让我们一起打造任何平台上最好的儿童安全工具。

## 许可证

待定（建议：GPL-3.0——安全工具的价值源于开放与可审计）。
