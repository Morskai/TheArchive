export interface FilmVersion {
  id: string
  label: string
  size: string
  format: string
  note: string
}

export interface Film {
  id: string
  code: string
  codePlaceholder?: boolean
  actorIds?: string[]
  realFilmId?: string
  title: string
  actor: string
  library: string
  sources?: { id: string; name: string }[]
  tags: string[]
  rating: number | null
  favorite: boolean
  createdAt: string | null
  modifiedAt?: string | null
  lastLaunchedAt: string | null
  hasCover: boolean
  artwork: number
  note: string
  fileSize: string | null
  directory: string | null
  versions: FilmVersion[]
  preferredVersionId: string | null
}

// Fictional records for layout and interaction review only. No disk data is read.
const demoFilmRows: Omit<Film, 'fileSize' | 'directory' | 'versions' | 'preferredVersionId'>[] = [
  { id: 'f01', code: 'DEMO-101', title: '夜色之间', actor: '林澄', library: '主收藏', tags: ['中文字幕', '剧情'], rating: 4, favorite: true, createdAt: '2025-11-14', lastLaunchedAt: '2026-08-21', hasCover: true, artwork: 1, note: '一张用暖光与窗影构成的演示封面。' },
  { id: 'f02', code: 'DEMO-102', title: '微光来信', actor: '周遥', library: '主收藏', tags: ['剧情'], rating: 4, favorite: false, createdAt: '2025-09-02', lastLaunchedAt: null, hasCover: true, artwork: 2, note: '用于查看长短不同的资料在详情中的层次。' },
  { id: 'f03', code: 'DEMO-103', title: '海岸线以北', actor: '苏梨', library: '备用片库', tags: ['中文字幕', '旅行'], rating: null, favorite: false, createdAt: '2024-06-19', lastLaunchedAt: null, hasCover: true, artwork: 3, note: '未评分项目会在评分排序时排在后面。' },
  { id: 'f04', code: 'DEMO-104', title: '最后一班电车', actor: '林澄', library: '主收藏', tags: ['剧情', '夜景'], rating: 5, favorite: true, createdAt: '2026-02-08', lastLaunchedAt: '2026-09-05', hasCover: true, artwork: 4, note: '用来检查收藏和最近启动排序的演示记录。' },
  { id: 'f05', code: 'DEMO-105', title: '白昼慢行', actor: '沈弥', library: '备用片库', tags: ['中文字幕', '旅行'], rating: 3, favorite: false, createdAt: '2025-02-12', lastLaunchedAt: '2026-03-10', hasCover: true, artwork: 5, note: '展示标签并列时的卡片排版。' },
  { id: 'f06', code: 'DEMO-106', title: '无声的房间', actor: '周遥', library: '主收藏', tags: ['夜景'], rating: null, favorite: false, createdAt: null, lastLaunchedAt: null, hasCover: false, artwork: 6, note: '没有封面、评分和日期的演示资料。' },
  { id: 'f07', code: 'DEMO-107', title: '雨后放映', actor: '沈弥', library: '主收藏', tags: ['中文字幕', '剧情'], rating: 4, favorite: true, createdAt: '2026-04-01', lastLaunchedAt: null, hasCover: true, artwork: 7, note: '封面使用抽象图形，不对应真实影片图片。' },
  { id: 'f08', code: 'DEMO-108', title: '蓝调时刻', actor: '苏梨', library: '备用片库', tags: ['夜景'], rating: 3, favorite: false, createdAt: '2024-12-16', lastLaunchedAt: '2026-01-16', hasCover: true, artwork: 8, note: '演示资料仅服务于界面判断。' },
  { id: 'f09', code: 'DEMO-109', title: '沿途的光', actor: '林澄', library: '主收藏', tags: ['旅行'], rating: null, favorite: false, createdAt: '2025-07-07', lastLaunchedAt: null, hasCover: true, artwork: 9, note: '用于检视紧凑密度下的标题截断。' },
  { id: 'f10', code: 'DEMO-110', title: '幕间回声', actor: '沈弥', library: '备用片库', tags: ['中文字幕', '剧情'], rating: 4, favorite: false, createdAt: null, lastLaunchedAt: '2026-05-18', hasCover: true, artwork: 10, note: '没有创建日期时会排在日期排序末尾。' },
  { id: 'f11', code: 'DEMO-111', title: '风停之后', actor: '叶岚', library: '主收藏', tags: ['剧情'], rating: null, favorite: false, createdAt: '2025-03-15', lastLaunchedAt: null, hasCover: true, artwork: 1, note: '' },
  { id: 'f12', code: 'DEMO-112', title: '远山回信', actor: '唐宁', library: '主收藏', tags: ['旅行'], rating: 4, favorite: false, createdAt: '2024-09-06', lastLaunchedAt: null, hasCover: true, artwork: 2, note: '' },
  { id: 'f13', code: 'DEMO-113', title: '月下长街', actor: '许知遥', library: '备用片库', tags: ['夜景'], rating: 3, favorite: false, createdAt: '2025-12-18', lastLaunchedAt: null, hasCover: true, artwork: 3, note: '' },
  { id: 'f14', code: 'DEMO-114', title: '未寄的信', actor: '顾晚', library: '主收藏', tags: ['中文字幕', '剧情'], rating: 5, favorite: false, createdAt: '2026-02-11', lastLaunchedAt: null, hasCover: true, artwork: 4, note: '' },
]

const demoSizes = ['2.4 GB', '1.8 GB', '3.1 GB', '2.7 GB', '1.6 GB', '2.2 GB', '2.9 GB', '1.9 GB', '2.5 GB', '3.0 GB', '2.0 GB', '2.6 GB', '1.7 GB', '2.3 GB']

export const demoFilms: Film[] = demoFilmRows.map((film, index) => {
  const mainVersion: FilmVersion = { id: `${film.id}-main`, label: '主版本', size: demoSizes[index], format: 'MP4', note: film.lastLaunchedAt ? '最近有启动记录' : '暂无启动记录' }
  const versions: FilmVersion[] = [mainVersion]
  if (index === 0 || index === 3) versions.push({ id: `${film.id}-sub`, label: '中文字幕版本', size: '2.1 GB', format: 'MP4', note: '中文字幕' })
  const actorId = ({ '林澄': 'a01', '沈弥': 'a02', '周遥': 'a03', '苏梨': 'a04', '叶岚': 'a05', '唐宁': 'a06', '许知遥': 'a07', '顾晚': 'a08' } as Record<string, string>)[film.actor]
  return { ...film, actorIds: actorId ? [actorId] : [], code: film.code.replace('DEMO-', 'FL-'), fileSize: demoSizes[index], directory: `${film.actor} / ${film.code.replace('DEMO-', 'FL-')}`, versions, preferredVersionId: mainVersion.id }
})

export interface FilmSource {
  listFilms(): Promise<Film[]>
}

export const demoFilmSource: FilmSource = {
  async listFilms() { return demoFilms },
}
