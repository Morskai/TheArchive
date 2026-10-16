# 应用图标

TheArchive 的品牌图标由 GPT-6.1 Sol 根据作者需求完成。应用主图标为带黑色圆角底板的金色 A，界面使用透明金色 A 标识。

| 文件 | 用途 |
| --- | --- |
| app-icon.png | 应用图标母版 |
| 32x32.png、128x128.png、128x128@2x.png、icon.ico | Windows 主程序、托盘与主安装器 |
| film-empty.png | 影片库空状态 |
| scan-diagnostic.png、scan-diagnostic.ico | 独立扫描诊断程序 |
| uninstall.png、uninstall.ico | 卸载器 |

SVG 源文件保留为对应资源参考；当前诊断和卸载图标使用 PNG／ICO。windows/set-diagnostic-icon.ps1 在打包前更新 Cargo 生成程序资源，不操作已安装程序或用户资料。更换名称和图标不得改变应用 identifier 或旧安装升级识别。
