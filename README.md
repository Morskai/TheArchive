<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="100" alt="TheArchive 应用图标" />
</p>

<h1 align="center">TheArchive</h1>
<p align="center"><strong>私人影像收藏室</strong></p>
<p align="center">把散落在硬盘里的影片，整理成自己的收藏。</p>

<p align="center"><strong>简体中文</strong> · <a href="README.en.md">English</a></p>

<p align="center">
  <img alt="React 19" src="https://img.shields.io/badge/React-19-292826?style=flat-square&amp;logo=react&amp;logoColor=61DAFB" />
  <img alt="TypeScript 5.9" src="https://img.shields.io/badge/TypeScript-5.9-292826?style=flat-square&amp;logo=typescript&amp;logoColor=3178C6" />
  <img alt="Vite 7" src="https://img.shields.io/badge/Vite-7-292826?style=flat-square&amp;logo=vite&amp;logoColor=AC80FF" />
  <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri-2-292826?style=flat-square&amp;logo=tauri&amp;logoColor=FFC131" />
  <img alt="Rust" src="https://img.shields.io/badge/Rust-292826?style=flat-square&amp;logo=rust&amp;logoColor=FFFFFF" />
  <img alt="SQLite" src="https://img.shields.io/badge/SQLite-292826?style=flat-square&amp;logo=sqlite&amp;logoColor=76B8DB" />
</p>

<p align="center">
  <a href="#下载安装">下载安装</a> ·
  <a href="#功能亮点">功能亮点</a> ·
  <a href="#界面预览">界面预览</a> ·
  <a href="#开始使用">开始使用</a> ·
  <a href="#数据与隐私">数据与隐私</a> ·
  <a href="#本地开发">本地开发</a> ·
  <a href="#完全-ai-开发">AI 开发说明</a>
</p>

![TheArchive 影片库界面](docs/images/library.png)

TheArchive 是一款 Windows 本地影视资料管理软件。通过主动扫描收录影片，用封面、标签、评分和收藏整理片库；独立管理演员资料，在浏览、翻牌和播放之间自然切换。

## 功能亮点

| 🎞️ 整理影片 | 👤 管理演员 |
| --- | --- |
| 主动扫描本地目录，管理多片库与视频版本；按搜索、标签、评分和收藏筛选。 | 独立维护姓名、别名、照片与个人资料，关联出演影片，导入和导出 actor.info。 |
| **🃏 翻牌发现** | **▶️ 随时播放** |
| 从自己的演员库中翻牌，每轮最多八位；揭晓后可进入详情。 | 内置播放器与自定义外部播放器；支持进度、倍速、字幕、音轨和独立窗口。 |
| **🗂️ 维护收藏** | **🌐 按需翻译** |
| 标签、封面比例与卡片密度可调整，提供资料备份、恢复和问题检查。 | 手动翻译片名；界面支持简体中文、繁体中文、英语和日语。 |

## 界面预览

<table>
  <tr>
    <td width="50%"><img src="docs/images/actors.png" alt="演员库：资料与出演关系" /><p align="center">演员库 · 资料与出演</p></td>
    <td width="50%"><img src="docs/images/flip.png" alt="演员翻牌：随机发现收藏" /><p align="center">翻牌 · 重新发现收藏</p></td>
  </tr>
</table>

<sub>截图使用虚构演示资料，不包含真实片库、个人路径或账号凭据。浏览器演示用于展示界面，实际扫描和播放在 Windows 桌面程序中使用。</sub>

## 开始使用

### 下载安装

**[下载 Windows x64 安装包](https://github.com/Morskai/TheArchive/releases/download/v1.0.0-preview.1/TheArchive_1.0.0_x64-setup.exe)** · [查看发布说明与校验信息](https://github.com/Morskai/TheArchive/releases/tag/v1.0.0-preview.1)

下载后运行安装程序，再从开始菜单或桌面快捷方式打开 TheArchive。需要 WebView2 运行时；影片和照片仍保存在你自己的目录中。

当前提供 **v1.0 Windows x64 安装包**。安装包与对应源码、SHA-256 及已验证范围在发布页列出；实际安装／覆盖升级和真实资料保留仍需进一步验证。页面下方的 Source code 压缩包是源码，不是安装程序。

### 从目录到片库

1. **添加目录**：在“设置”中选择存放影片的文件夹，可以添加多个片库。
2. **主动扫描**：读取目录与视频信息，建立影片卡片。同一目录的多个视频归为版本或分集，子目录分别收录。
3. **整理收藏**：修改片名、编号，添加标签、评分和收藏，选择首选播放版本。
4. **补充演员**：手动创建资料，或扫描包含 actor.info 的目录，再维护出演关系。
5. **浏览与播放**：在影片库筛选收藏，也可以从演员详情或翻牌结果重新发现影片。

更多操作见 **[使用指南](docs/USAGE.md)**。

### 目录示例

不需要把整个硬盘整理成固定层级；分类目录和影片目录可以混合存放。例如：

~~~text
我的影片/
├─ 剧情/
│  └─ [FL-101] 夜色之间/
│     ├─ poster.jpg
│     ├─ 正片.mp4
│     └─ 导演剪辑版.mkv
└─ [FL-102] 微光来信/
   ├─ poster.png
   └─ 微光来信.mp4
~~~

上例会收录两张影片卡片，“夜色之间”包含两个视频版本。封面优先读取同目录的 poster 图片；仅有一个视频时，也可使用视频同名图片。支持 JPG、JPEG、PNG、WebP 和 AVIF。

演员资料单独管理，扫描影片不会凭文件夹名称自动创建演员。导入演员时，每位演员使用一份 actor.info 和可选照片，示例见[演员资料格式](docs/ACTOR_INFO.md)。

## 数据与隐私

| 内容 | 保存与处理方式 |
| --- | --- |
| 影片与媒体文件 | 原件只读。软件内编辑资料或删除影片记录，不改名、移动或删除媒体原文件。 |
| 标签、评分、收藏与出演关系 | 保存在本地软件数据库中，可通过软件备份恢复。 |
| 演员照片 | 保存所选原图的路径与指纹，并使用显示缓存；原图仍需自行保管。 |
| 翻译密钥 | 保存在当前 Windows 用户的系统凭据中，不写入数据库、日志或软件备份。 |
| 片名翻译请求 | 只在手动确认后发送正文与必要参数；不发送完整片库或媒体文件。 |
| 软件备份 | 包含软件资料及非秘密设置，不包含媒体原件、照片原图、图片缓存或翻译密钥。 |

文件变化后需要再次主动扫描；启动软件、切换页面和重新连接硬盘不会自动扫描。备份软件资料时，也应另行备份自己的媒体与照片。

## 使用前了解

- **浏览器演示与桌面程序**：浏览器展示虚构资料，适合查看界面；真实片库扫描、系统凭据和本地播放需要 Windows 桌面程序。
- **平台范围**：当前开发与打包面向 Windows x64，未提供 macOS、Linux 或移动端版本。
- **翻译服务**：支持 DeepL、Google Cloud Translation、Microsoft Azure Translator 和百度翻译，需自行申请服务并配置凭据，可能产生费用。启动和扫描不会自动调用 API；片名翻译目标为简体中文，不随界面语言切换。
- **播放历史**：记录成功发起的播放，不表示已经看完，也不等同于观看进度。
- **验证范围**：项目仍在完善；真实大型片库、最终安装与覆盖升级、部分媒体格式及翻译服务的实测边界见[开发指南](docs/DEVELOPMENT.md#当前验证边界)。

## 本地开发

前端使用 **React + TypeScript + Vite**，桌面层使用 **Tauri + Rust + SQLite**。

### 浏览器演示

准备满足 Vite 7 要求的 Node.js 与 pnpm，克隆项目后运行：

~~~powershell
git clone https://github.com/Morskai/TheArchive.git
cd TheArchive
pnpm install --frozen-lockfile
pnpm dev
~~~

### Windows 桌面开发

还需 Rust 1.90+、MSVC C++ 构建工具、Windows SDK 和 WebView2。完成环境准备后启动桌面程序：

~~~powershell
pnpm desktop:dev
~~~

打包还需 NASM、固定版本的播放器运行时及完整 Rust 依赖缓存，统一入口为：

~~~powershell
pnpm desktop:build --bundles nsis
~~~

桌面开发会使用应用正常资料位置，测试恢复或清除操作前需要隔离测试资料。完整准备步骤、测试方法和产物核对见 **[开发指南](docs/DEVELOPMENT.md)**。

### 项目结构

~~~text
src/             React 页面、组件、数据接口与界面语言
src-tauri/src/   Rust 桌面服务、数据库、扫描、播放与翻译
src-tauri/tests/ 后端集成测试
tests/           前端逻辑、浏览器与原生交互验证
scripts/         播放器准备与安装包构建
docs/            使用和开发文档、界面截图
~~~

## 文档

| 文档 | 内容 |
| --- | --- |
| [使用指南](docs/USAGE.md) | 扫描、资料、播放、翻译与备份 |
| [开发指南](docs/DEVELOPMENT.md) | 环境、构建、源码地图与验证边界 |
| [演员资料格式](docs/ACTOR_INFO.md) | actor.info 字段、匹配与导出规则 |
| [翻译说明](docs/TRANSLATION.md) | 服务配置、凭据与翻译记忆 |

## 完全 AI 开发

**TheArchive 是一个完全 AI 开发项目。** 从应用代码、交互与视觉设计，到图标、图片、测试和项目文档，均由 **GPT-6.1 Sol** 完成。作者负责提出需求、给出反馈并验收效果。

欢迎通过 [Issues](https://github.com/Morskai/TheArchive/issues) 反馈具体场景和问题。

## 许可与第三方组件

本项目自身的源码、文档与创作资产采用 **[MIT 许可证](LICENSE)**。内置播放器采用可替换的 LGPL libmpv，其许可独立适用；来源与许可见 [播放器说明](src-tauri/player-licenses/NOTICE.txt)。安装模板及测试素材的许可保留在各自目录中。
