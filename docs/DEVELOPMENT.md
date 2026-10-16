# 开发指南

## 范围与版本

TheArchive 面向 Windows x64，产品 v1.0，构建 1.0.0，当前 SQLite schema 23。本仓库以当前源码快照作为首次公开版本，不包含此前私有开发提交、旧审计材料或本机资料。

公开安装包、对应源码提交、校验值及验证范围统一保存在 [GitHub Releases](https://github.com/Morskai/TheArchive/releases)。产品版本和应用升级身份保持不变；发布包不代表安装／覆盖升级已通过实测。

前端：React / TypeScript / Vite。桌面：Tauri / Rust / SQLite。浏览器提供虚构数据演示；实际扫描、系统凭据、播放器与文件操作依赖桌面层。

## 环境准备

- Node.js：使用满足 Vite 7 要求的版本（20.19+、22.12+ 或相应更新版本）；运行下述 TypeScript 测试建议 Node.js 22.18+。
- pnpm：依赖以 pnpm-lock.yaml 为准，使用 pnpm install --frozen-lockfile 安装，不随意更新锁文件。
- Windows 桌面：Rust 1.90+、MSVC C++ 构建工具、Windows SDK、WebView2。
- 构建工具：NASM 必须位于 PATH 或 .local-tools/nasm-3.01/nasm.exe。
- 首次准备需要联网获取依赖与固定播放器运行时；离线构建需要已准备好的 Rust 依赖缓存。

## 启动

浏览器演示：

~~~powershell
pnpm install --frozen-lockfile
pnpm dev
~~~

桌面开发：

~~~powershell
pnpm desktop:dev
~~~

桌面开发使用应用正常资料位置，并不是隔离测试环境。不要在真实资料上试验删除、恢复、重置或故障注入。

桌面命令会通过 scripts/prepare-player.mjs 下载 src-tauri/player-runtime.json 指定的 LGPL libmpv，并核对归档与 DLL 的 SHA-256；运行时位于忽略的 .local-tools/mpv/，无需提交 DLL。Windows 自带 tar 需支持解开相应 7z 归档。

## Windows 安装包

首次在联网环境准备依赖和运行时：

~~~powershell
cargo fetch --manifest-path src-tauri/Cargo.toml --locked
node scripts/prepare-player.mjs
~~~

准备好 NASM，并将预期产品源码提交到 Git，再运行：

~~~powershell
pnpm desktop:build --bundles nsis
~~~

统一入口固定 Windows x64、release、NSIS、单并发与 Cargo 离线模式。它会检查源码是否提交、运行时哈希及安装包是否为本次产物。安装包与 installer-receipt.json 位于 src-tauri/target/release/bundle/nsis/；日志在 src-tauri/target/build-installer.log。

核对 receipt 的 complete 状态、源码提交、本次写入时间、字节数和 SHA-256。生成安装包不等于实际安装、覆盖升级或真实片库验证通过。Tauri/NSIS 首次打包还可能需要获取其工具缓存。

## 测试入口

~~~powershell
node --experimental-strip-types --test --test-isolation=none tests/*.test.ts
cargo test --manifest-path src-tauri/Cargo.toml --offline -j 1
pnpm build
~~~

Rust 检查要求本地依赖缓存齐全。缺缓存时先处理环境，不删锁文件或清真实资料。前端单测、合成 SQLite 测试、浏览器模拟 IPC、原生 probe 和最终安装验证属于不同证据，不互相替代。

tests/*.browser.mjs 使用 Playwright 与 Edge；需另行准备 Playwright，可通过 PLAYWRIGHT_MODULE_DIR 指向其安装目录。浏览器测试文件内注明其合成夹具与输出位置。原生 probe 只允许使用独立 SQLite、合成媒体及独立 WebView2 资料目录。

## 源码地图

| 位置 | 主要职责 |
| --- | --- |
| src/App.tsx、src/components/ | 页面、资料编辑、播放层与交互 |
| src/data/、src/i18n/ | 前端数据接口、状态规则与四语言资源 |
| src-tauri/src/source_scan.rs、catalog.rs | 主动扫描与影片目录 |
| actor_management.rs、actor_info.rs、actors.rs | 演员匹配、导入导出与个人资料 |
| storage.rs、storage_session.rs、lifecycle.rs | 数据库、会话代际与任务租约 |
| reset.rs、image_session.rs、covers.rs | 恢复初始状态与图片缓存 |
| embedded_player.rs、mpv.rs、playback.rs | 播放会话与原生引擎 |
| translation*.rs | 服务、凭据、任务与记忆 |
| diagnostics.rs、diagnostic_commands.rs | 固定字段诊断与导出 |
| tests/、src-tauri/tests/ | 前端规则、浏览器与后端集成回归 |

## 维护时保留的边界

- 媒体与外部 info 原件只读；用户明确导出时才在选定位置生成副本。测试不得读取真实片库或系统密钥。
- 只有主动扫描核对来源；不恢复目录监听、后台轮询或启动自动扫描。每目录直属视频一卡，演员资料独立于影片扫描。
- 身份恢复需要成功扫描证据及唯一完整视频文件名，不得覆盖人工片名、原名选择或出演关系；歧义不能静默合并。
- 扫描在普通资料 Mutex 外枚举，锁内保护并事务提交；取得扫描租约后核对 ReadyStorage generation。独占任务先取得租约再取资料锁，不反转顺序。恢复成功使旧请求失效，失败不提升代际。
- 生命周期、资料、图片的锁顺序保持生命周期独占 → 资料锁 → 图片写入锁；释放图片写入锁后再清 ImageSession。
- 恢复初始状态使用持久恢复记录；未完成回滚或清理时不能开放正常资料写入。cleanup_pending 和 recovery_required 必须如实报告，不能用删库掩盖。
- 演员照片保存路径和指纹，不保存永久照片 BLOB；actor.info 缺省、空值、精确匹配和导出拥有权规则以[格式说明](ACTOR_INFO.md)为准。
- 翻译只允许明确手动请求，密钥只保存在系统凭据中；取消、恢复、重置或人工编辑后的迟到结果不能写回。详见[翻译说明](TRANSLATION.md)。
- 图片按需读取有效缓存或保存路径，失败本次运行停止重试；只在成功扫描、明确保存或清缓存的对应范围解锁，不全库预载。
- 诊断使用固定字段与白名单，不记录任意异常原文、路径、片名、姓名、请求体或密钥。日志故障不得改变业务结果，导出拒绝目录重定向和硬链接。
- productName 为 TheArchive，identifier 保持 com.localfilmlibrary.preview；旧安装识别、资料目录和兼容迁移不得因公开仓库改名而改变。

## 当前验证边界

此前私有开发中有自动测试、合成场景、浏览器模拟 IPC 和部分独立原生 probe 证据；本公开仓库不复制旧运行日志，也不把它们表述为本次新环境验收。

最终安装／覆盖升级与真实资料保留、Windows 11、真实大型片库、USB／网络盘、实际视频编码／硬解／HDR，以及四个翻译服务与 Windows 系统凭据的完整实测仍需分别验证。发布源码并不表示这些项目已经完成。

修改产品功能后执行相关回归与 NSIS 构建；纯文档和仓库展示修改只核对内容、链接、隐私与源码一致性，不机械重打软件。
