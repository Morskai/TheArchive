# v1.0 影片中文标题翻译

本文件维护翻译、凭据和翻译记忆契约。产品 **v1.0**、构建 **1.0.0**，应用、原资料目录及旧NSIS升级身份保持；未经用户确认不升产品版本。开发与验证边界见[开发指南](DEVELOPMENT.md)。

## 使用与数据边界

支持DeepL Free／Pro、Google Cloud Translation、Microsoft Azure Translator和百度翻译开放平台。先保存服务设置、配置系统凭据、测试连接，再手动检测、勾选并确认开始；扫描／启动不自动调用API。纯汉字可能为中文或日文，语言待确认默认不选。测试连接发送固定公开Hello，可能占服务额度，不保证免费。

候选支持即时原文／译名搜索、全部片库／来源筛选、互斥“已翻译／待确认”及折叠。两个状态都不选显示全部，搜索与片库及状态取交集；清空关键词恢复当前范围。筛选保留选择，全选仅改变当前可见可选项，任务处理全部已选。已翻译含原文变化项。检测提示在工具栏上方，统计仅待翻译／翻译记忆，独立清除按钮右侧；连接反馈在密钥区域。

译名显示优先于原始目录／文件标题，未翻译保留原展示。详情片名旁的小铅笔“编辑片名”进入单个编辑框，回车或点击外部保存；没有确认／取消按钮。有效编辑以user来源写入影片自身的翻译记忆，不修改共享记录，不改原文件。编辑及original选择只按稳定影片／已证明别名限定，不凭同编号传播。已编辑条目可在“已翻译”范围搜索，但不可勾选为API重译；清空编辑可恢复检测资格。全部删空以original选择记住还原当前括号外原片名，旧自动译名不会复活；之后仍可主动检测翻译。卡片与详情不显示原文行，原文仍为搜索与记忆证据。旧film_manual.title只为旧库／备份兼容保留，无显示／编辑／新写入入口。

自动API记忆的强匹配要求标准化番号与完整正文证据一致，满足时可跨新影片ID复用，不能只凭编号或相似度合并；无番号的自动记忆及user／original记忆仍要求同一identity_anchor（稳定影片／已证明别名），不能凭同编号传播人工编辑或原名选择。编号取首个非空半角[]；成对[]／【】内容均不作为片名或翻译正文，空白与标点变化可复用。未闭合括号保留。旧记忆正文证据若包含未知括号，可能显示原文变化，不自动联网重译。正文变化保留旧译名、标记待更新，明确主动重译才调用API；成功后重新入库沿用最新有效结果。

记忆独立于影片存亡，删影片重扫或移除来源不清长期记忆。“恢复初始状态”保留translation_memory全部12列原值：id、identity_anchor、normalized_code、original_title、normalized_title、body、fingerprint、source_language、translated_title、provider、translated_at、last_matched_at；清除影片与film_translations关联、来源、观察、film_aliases等旧身份锚点，不为记忆保留旧影片。再次主动扫描后，有编号自动记忆可在强证据一致时跨新filmId复用；无编号及user／original记忆虽然保留，因旧身份已清，现行匹配不会自动复用到新身份。此操作不放宽匹配，也不自动调用API。其余恢复范围与失败语义见[使用指南](USAGE.md#备份恢复与图片缓存)。

独立“清除翻译记忆”清全部自动／编辑记忆及关联（包括原片名还原选择），先保护、数量确认后事务执行；数量变化停止，不清评分／收藏／标签／演员资料或原件。它与保留完整记忆的“恢复初始状态”是两个操作。

作品info导入／导出及相关IPC已删除，不再通过作品文件传输片名或出演关系。演员actor.info扫描处理演员个人资料、照片路径及精确匹配的出演补充，详见[info格式说明](ACTOR_INFO.md)；不传输标题记忆，软件资料的完整恢复使用备份。

## 设置、凭据与备份

区域／APP ID为非秘密配置，可随数据库备份。旧备份恢复执行受保护事务迁移，保留人工资料、记忆和关联，不重建库。迁移与回退边界见[开发指南](DEVELOPMENT.md)。

API Key／百度密钥仅存Windows当前用户Credential Manager，四个固定目标为TheArchive/translation/{deepl,google,azure,baidu}；不存SQLite／JSON／Git／备份／日志。输入框读回当前服务凭据，默认掩码，可显示／隐藏，保存后持续显示；换服务先清空再读取。仅驻留组件，换电脑恢复需重配密钥，已保存译名可离线读取。同机备份恢复不创建／删除系统凭据；“恢复初始状态”明确删除这四个软件凭据及翻译配置，恢复中保护／回滚凭据、失败恢复未完成时阻断密钥读写。持久记录只保存原有凭据是否存在，临时保护仍在系统凭据库，不把Key落磁盘。

## 服务接入

| 服务 | 官方接口及配置 |
| --- | --- |
| DeepL | Free为https://api-free.deepl.com/v2/translate，Pro为https://api.deepl.com/v2/translate；Authorization头，目标ZH-HANS。见[官方参数](https://developers.deepl.com/api-reference/translate/request-translation)。 |
| Google | Basic v2 https://translation.googleapis.com/language/translate/v2，X-goog-api-key头，目标zh-CN／format=text，无service-account JSON。见[Basic API](https://docs.cloud.google.com/translate/docs/reference/rest/v2/translate)。 |
| Azure | https://api.cognitive.microsofttranslator.com/translate?api-version=3.0&to=zh-Hans；Ocp-Apim-Subscription-Key及资源区域头。global／空区域不发送区域头，区域资源需正确区域；只实现标准公共端点。见[Translate](https://learn.microsoft.com/en-us/azure/ai-services/translator/text-translation/reference/v3/translate)与[认证](https://learn.microsoft.com/en-us/azure/ai-services/translator/text-translation/reference/authentication)。 |
| 百度 | https://api.fanyi.baidu.com/api/trans/vip/translate；POST表单q、from=auto、to=zh、appid、salt、sign，按UTF-8正文执行MD5(appid+q+salt+密钥)签名，不发送原密钥。单条最多6000字节，串行批次请求间隔至少1050ms，兼容标准账号1QPS。见[官方接入说明](https://fanyi-api.baidu.com/doc_bd/21)。 |

只向服务发送待翻译正文及必需参数，不发路径、用户名、数据库ID、评分／收藏／历史／演员资料。固定HTTPS端点，禁止重定向及自动重试，连接8秒／总30秒；其他服务正文32KiB、响应256KiB上限。返回一条有效翻译才保存，结构错误、空译文及认证／额度／限流等只返回固定错误码，不暴露远端消息或密钥。各服务使用API单结果，没有网页候选选择流程；质量增强另行讨论。

批次同正文共享请求，逐条提交；错误停止后续请求，部分成功保留。网络等待不持有资料锁，提交重核原文／启用状态及请求开始时的已关联记忆ID；原文变化、编辑或清空、取消、恢复或清除后的迟到结果不提交，写入失败事务回滚。关闭界面不丢已保存结果，进程关闭时未提交请求不保证完成。

## 验证口径

影片身份恢复也保护人工片名：关联user／original记忆的临时影片不再属于“未编辑自动身份”，不因延期目录更新后的唯一文件名恢复而合并到旧影片。自动API译名仍沿用既有恢复规则；不改翻译请求或启用条件。

Provider请求、假响应、错误映射与密钥隔离对应Rust库测试；迁移、旧备份恢复、清除记忆、编辑范围、迟到结果及实际值恢复对应translation_memory_flow等合成测试。“恢复初始状态”的12列完整保留、旧锚点清空、新身份强匹配范围、凭据失败回滚与中断续行使用reset库测试及可注入假凭据接口。界面交互使用模拟IPC，不能代替真实服务、Windows凭据或安装版验证。入口、命令与当前验证边界见[开发指南](DEVELOPMENT.md)。

真实四服务连接／额度、Windows凭据读回／保存／删除及安装版检查须分别记录证据；当前未验证范围见[开发指南](DEVELOPMENT.md#当前验证边界)，不以合成测试或此前用户体验确认代替。
