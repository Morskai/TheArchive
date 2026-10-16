<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="100" alt="TheArchive 应用图标" />
</p>

<h1 align="center">TheArchive</h1>
<p align="center"><strong>私人影像收藏室</strong></p>
<p align="center">把散落在硬盘里的影片，整理成自己的收藏。</p>

<p align="center">
  <img alt="平台 Windows x64" src="https://img.shields.io/badge/Windows-x64-292826?style=flat-square" />
  <img alt="版本 v1.0" src="https://img.shields.io/badge/version-v1.0-b99a60?style=flat-square" />
  <a href="LICENSE"><img alt="MIT License" src="https://img.shields.io/badge/license-MIT-59634f?style=flat-square" /></a>
  <img alt="完全 AI 开发" src="https://img.shields.io/badge/AI-GPT--6.1_Sol-77634b?style=flat-square" />
</p>

<p align="center">
  <a href="#功能亮点">功能亮点</a> ·
  <a href="#界面预览">界面预览</a> ·
  <a href="#开始使用">开始使用</a> ·
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

1. 在“设置”中添加影片目录，主动扫描收录影片。
2. 在影片库浏览、筛选、评分和收藏，按需维护片名与标签。
3. 在演员库添加资料或导入 actor.info，再关联出演影片。

更多操作见 **[使用指南](docs/USAGE.md)**。本次先发布源码；安装包以仓库后续发布的 Release 为准。

本地管理以媒体原文件只读为前提；扫描不会自动联网翻译。片名翻译需自行配置服务与密钥，软件备份不包含媒体原件或翻译密钥。

## 本地开发

前端使用 **React + TypeScript + Vite**，桌面层使用 **Tauri + Rust + SQLite**。

安装 Node.js 与 pnpm 后，可启动虚构数据的浏览器演示：

~~~powershell
pnpm install --frozen-lockfile
pnpm dev
~~~

桌面开发、Windows 构建工具和测试方法见 **[开发指南](docs/DEVELOPMENT.md)**。

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
