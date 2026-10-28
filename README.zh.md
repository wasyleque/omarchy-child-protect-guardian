# Omarchy Child Protect Guardian 🛡️

**🌐 语言：** [English](README.md) · [Polski](README.pl.md) · [Español](README.es.md) · [Deutsch](README.de.md) · [Français](README.fr.md) · **中文**

> 面向 **Omarchy Linux**（Arch + Hyprland）的家长控制与儿童上网安全系统，
> 核心功能是**远程批准应用安装**——家长在手机上通过推送审批放行，
> 就像确认一次 2FA 登录一样简单。

> **隐私保护：** 开启**载荷加密**（`[ntfy].encrypt_payload`）后，中转服务器只能看到密文——软件包名称、来源和完整命令均使用家长公钥密封（已在真机上完成端到端验证）。无论是否开启加密，所有审批决定都会进行加密签名。若追求极致隐私，您还可以运行**带有认证/ACL 的自托管 ntfy**。

宏大目标：比 Microsoft Family Safety、Apple Screen Time、Google Family Link 和 Qustodio
**更方便、更有效**，同时保持**隐私**（无遥测，不把孩子的数据上传到企业云）。

---

## 🚀 安装与更新

> **按安全顺序排列的完整部署指南：** **[`packaging/README.md`](packaging/README.md)**（硬件基线 → 守护进程 → pacman 钩子 → nftables 出站规则 → fapolicyd → 系统加固 → 更新）。简明步骤：

**在孩子的电脑上（Arch / Omarchy）：**
```bash
git clone https://github.com/wasyleque/omarchy-child-protect-guardian
cd omarchy-child-protect-guardian/daemon && cargo build --release && cd ..
sudo packaging/install.sh                   # 默认安全测试模式（仅拦截 'pacman -S sl'）；就绪后加上 --enforce
sudo packaging/setup-child.sh <孩子用户名>   # 受限儿童账户（无 sudo）+ 配对二维码
```
`install.sh --enforce` 还会安装并启用 **DNS 内容过滤**（色情/分类屏蔽列表 + SafeSearch）以及 **nftables** 出站规则，并在重启后依然生效。只需一条可逆的向导命令即可按需开启过滤模式：
```bash
sudo /usr/lib/guardian/guardian-dns-mode.sh child-only      # 仅过滤孩子账户（推荐）
sudo /usr/lib/guardian/guardian-dns-mode.sh whole-machine   # 过滤整台电脑（包含家长账户）
```
接着配置其余与主机相关的防护层——**fapolicyd** 执行白名单（`guardian-fapolicyd-setup.sh`，真正强制拦截所*必需*）、浏览器/网络管理器锁定以及硬件基线——详见 [`packaging/README.md`](packaging/README.md)。随时可运行 `sudo packaging/smoke-test.sh <孩子用户名>` 进行端到端自检（甚至会验证儿童账户是否已成功拦截色情域名）。

**在家长手机上：** 从 **[Releases 页面](https://github.com/wasyleque/omarchy-child-protect-guardian/releases/latest)** 安装 Android 应用并扫描配对二维码——详见 [📱 Android 家长端应用](#-android-家长端应用)。没有 Android 手机？请看 [🍏 在 iPhone/iPad、Windows、Mac 或 Linux 上使用家长端应用](#-在-iphoneipadwindows-mac-或-linux-上使用家长端应用无需-android)。

**后续更新：** `git pull` → `cargo build --release` → `sudo packaging/install.sh` → `sudo systemctl restart guardiand`（在 fapolicyd 强制模式下，请先重新信任新编译的二进制文件）——完整步骤见 [`packaging/README.md` → *7. Updating*](packaging/README.md#7-updating-to-a-new-version)。手机端 APK 可**直接覆盖安装**（保留配对信息，无需重新配对）。

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
- **时间预算与作息** —— 通过 cgroups 冻结 + Hyprland 空闲检测强制执行。
- **全系统内容过滤** —— 本地过滤解析器屏蔽色情/分类域名（StevenBlack 列表，每日自动更新），并在 Google/YouTube/Bing 上强制开启 SafeSearch，覆盖*每一个*应用而非仅限浏览器。安装后开机自启；一条命令即可选择**仅限儿童**或**整机过滤**，且完全可逆。DoH/DoT/VPN 绕过路径在内核层（`nftables`）被封堵。
- **给家长的可读报告** —— 孩子做了什么、申请了什么（没有企业式监控）。
- **加密签名且可选加密的审批** —— 即使推送中转服务器被攻破，也无法伪造“允许”；开启载荷加密后，服务器甚至看不到软件包名或命令。
- **受限远程 root（Intent Binding）** —— 一次批准仅授予*完全一致的单次操作*权限，绝不提供 shell；与家长实际看到的内容加密绑定。
- **AI 风险助手** —— 按需提供通俗易懂的风险评估（红绿灯 + 权限翻译）；仅使用公开的应用元数据，孩子的任何数据都不会离开设备。
- **可选的本地 AI 自主判断（测试版，默认关闭）** —— 设备上的小型模型（无需联网、仅用 CPU）可以*替你*回应安装请求：你可以选择 `ask`（默认——所有请求仍交给你）、`quiet`（仅自动批准明显安全的应用）或 `auto`（模型也会拒绝有风险的应用）。任何存疑的情况都会回到你这里，每晚你都会收到一份有保证的当日决定摘要，一条命令即可关闭。参见 [`docs/AI-MODELS.md`](docs/AI-MODELS.md)。
- **LACS 开放年龄分级** —— 超越 PEGI/ESRB 的社区多维年龄分级，覆盖 Linux/AUR/Flathub 应用。
- **温和统计与 AI 家长日报** —— 使用时长、应用与网站分类，以及引导亲子沟通的每日摘要——而非监控档案。

## 文档

| 文档 | 内容 |
|---|---|
| [`docs/CONCEPT.en.md`](docs/CONCEPT.en.md) / [`docs/KONCEPCJA.md`](docs/KONCEPCJA.md) | 完整概念、优势、使用场景、非显而易见的点子 |
| [`docs/ARCHITECTURE.en.md`](docs/ARCHITECTURE.en.md) / [`docs/ARCHITEKTURA.md`](docs/ARCHITEKTURA.md) | 架构：守护进程、拦截点、推送审批、安全性 |
| [`docs/ROADMAP.md`](docs/ROADMAP.md) | 构建阶段（MVP → v1），拆分为可验证的小步骤 |
| [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md) | 防绕过设计：攻击者层级、每种绕过途径及其缓解措施、诚实的边界说明 |
| [`docs/REVIEW-RESPONSE.md`](docs/REVIEW-RESPONSE.md) · [`docs/REVIEW-RESPONSE-2.md`](docs/REVIEW-RESPONSE-2.md) | 对两次外部安全审计的回应（每项发现 → 修复/状态） |
| [`android-app/`](android-app/) | **原生 Android 家长端应用**（Kotlin）——推荐的家长客户端；设备端 Ed25519 签名、后台推送 |
| [`parent-app/`](parent-app/) | 跨平台家长审批应用（可安装 PWA）——设备端 Ed25519 签名 |
| [`packaging/`](packaging/) | 部署：安装脚本、systemd 单元、pacman 钩子、nftables 出站规则、fapolicyd 白名单、加固审计 |
| [`docs/TESTING.md`](docs/TESTING.md) | 在干净系统上的分步测试计划（逐层验证 + 红队绕过测试） |

## 📱 Android 家长端应用

推荐的家长客户端是**原生 Android 应用**（Kotlin）——从 **[Releases 页面](https://github.com/wasyleque/omarchy-child-protect-guardian/releases/latest)** 下载并扫描二维码即可完成配对。界面支持 **English · Polski · Español · Deutsch · Français · 中文**（自动跟随手机系统语言）。

<p align="center">
  <img src="docs/screenshots/android-app.png" alt="Guardian Parent — Android app" width="320">
</p>

主要功能：

- **批准或拒绝安装** —— 当孩子运行 `pacman`/`yay`/`flatpak`/`snap`/`nix` 时，安装会被挂起，您的手机会收到包含应用名称、完整命令以及 🟢/🟡/🔴 **AI 风险指示灯**的推送；您可以直接在通知栏*或*应用内列表中点按**允许 / 拒绝**。每次决定都在设备端签名，即使推送服务器被攻破也无法伪造“允许”。
- **一部手机管理多个孩子** —— 添加每个孩子（独立电脑或共用电脑）并随时切换；每条请求和通知都会标注孩子的名字。
- **免输入的屏幕时间表** —— 通过下拉菜单设置每日限额、通过时间选择器设置允许时段，可每天相同或按星期分别设置，签名后直接下发到电脑。
- **立即锁定** —— 随时在手机上一键锁定（或解锁）孩子的电脑。
- **奖励额外时间** —— 一键为指定孩子增加 +15 / +30 分钟。
- **每日 AI 日报** —— 可选的每日摘要，汇总今日使用的应用、是否有违规尝试，并在出现令人担忧的搜索主题（毒品、自残、武器等）时发出明确警告——由您自己机器上的**本地**模型生成，任何数据都不会离开您的局域网。

## 🍏 在 iPhone/iPad、Windows、Mac 或 Linux 上使用家长端应用（无需 Android）

没有 Android 手机？家长端应用同样可以作为**可安装的网页应用（PWA）**运行——同样在设备本地完成签名，只需用浏览器打开即可。无需 App Store，无需注册账号。

1. **在浏览器中打开应用链接**（您通过 HTTPS 托管的 [`parent-app/`](parent-app/) 副本——例如免费的 GitHub Pages 地址；配置说明见 [`parent-app/README.md`](parent-app/README.md)）。
2. **安装到设备：**
   - **iPhone / iPad：** 点按 **共享 → 添加到主屏幕**，之后始终从主屏幕上的新图标打开。（Apple 仅允许以这种方式打开的网页应用发送通知——需 iOS 16.4 或更高版本。）
   - **Windows / Mac / Linux：** 点击浏览器地址栏（Chrome/Edge）中的**安装**图标将其安装为桌面应用，或直接使用浏览器标签页。
3. **配对：** 扫描孩子电脑上显示的同一个二维码（或手动粘贴一次服务器地址与主题）。
4. **开启后台通知：** 点按 **Enable background notifications**，这样即使关闭应用，收到安装请求时手机也会响铃提醒。点击通知即可打开应用并查看经过签名验证的真实请求。

您可以安全地获得核心功能：**经过签名的允许 / 拒绝**（被劫持的中转服务器无法伪造）、私钥永不离开您的设备，且除实时请求流外均可离线工作。**Android 原生应用仍然是功能最完整的客户端**（包含时间表编辑器、立即锁定、额外时间发放、每日日报视图以及 AI 服务器设置）；网页版则专注于**审批与警报**。请根据您手中的设备自由选择。

## 状态

✅ **已可用 —— 正在积极测试中，暂勿作为孩子唯一的保护手段。** 核心功能已构建并通过单元与端到端测试，已在真实硬件上运行（推送审批循环、屏幕时间、fapolicyd、nftables 和审计链均按设计工作），原生 Android 应用也已安装并正常使用。两次外部安全审计（2026-10-09、2026-10-14）和三次真机测试（2026-10-21/23）推动修复了多项实际强制执行中的问题。**在您的机器上完成完整的硬件基线配置并启用 fapolicyd 强制模式验证之前，请将其视为强有力的辅助工具，而非唯一的防线。** 详见安全审计 [`guardian-security-review.md`](guardian-security-review.md) + [`guardian-security-review-2.md`](guardian-security-review-2.md)、回应文档 [`docs/REVIEW-RESPONSE.md`](docs/REVIEW-RESPONSE.md) + [`docs/REVIEW-RESPONSE-2.md`](docs/REVIEW-RESPONSE-2.md) 以及真机测试报告 [`guardian-test-report-2026-10-21.md`](guardian-test-report-2026-10-21.md) + [`guardian-test-report-2026-10-23.md`](guardian-test-report-2026-10-23.md)。请参照 [`docs/TESTING.md`](docs/TESTING.md) 进行部署。

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
