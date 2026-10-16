import assert from 'node:assert/strict'
import test from 'node:test'
import { actorLibraryComponent } from './helpers/actorLibraryComponent.ts'
import { demoActors } from '../src/data/actors.ts'

const actors = Array.from({ length: 80 }, (_, index) => ({ ...demoActors[index % demoActors.length], id: `id-${index}`, name: `演员${index}`, realActorId: `id-${index}`, tags: index < 40 ? ['精选'] : [], favorite: index < 20 }))

test('photo filter clears selection, intersects favorites and survives detail return and photo updates', async () => {
  const photos = actors.map((actor, index) => ({ ...actor, hasPortrait: index % 2 === 0 }))
  const ui = actorLibraryComponent(photos)
  try {
    await ui.flush(); ui.button('演员管理').onClick(); await ui.flush()
    ui.control('全选当前筛选结果').onChange({ target: { checked: true } }); await ui.flush()
    ui.update({ missingPhotosOnly: true }); await ui.flush()
    assert.equal(ui.nodes(node => node.props?.['data-actor-id']).length, 40)
    assert.match(ui.text(), /已选\s+0/)
    ui.nodes(node => node.props?.active !== undefined)[0].props.onClick(); await ui.flush()
    assert.equal(ui.nodes(node => node.props?.['data-actor-id']).length, 10)
    ui.button('退出演员管理').onClick(); await ui.flush()
    ui.nodes(node => node.props?.['data-actor-id'] === 'id-1')[0].props.onClick(); await ui.flush()
    ui.update({ detailOpen: false, actors: photos.map(actor => actor.id === 'id-1' ? { ...actor, hasPortrait: true } : actor) }); await ui.flush()
    assert.equal(ui.nodes(node => node.props?.['data-actor-id']).length, 9)
    ui.update({ actors: photos.map(actor => ({ ...actor, hasPortrait: true })) }); await ui.flush()
    assert.match(ui.text(), /暂无照片缺失的演员/)
    ui.button('清除筛选').onClick(); await ui.flush()
    assert.equal(ui.nodes(node => node.props?.['data-actor-id']).length, 80)
  } finally { ui.dispose() }
})

test('actor selection mode selects all filtered cards and preserves selection across sort and density', async () => {
  const ui = actorLibraryComponent(actors)
  try {
    await ui.flush()
    ui.button('演员管理').onClick(); await ui.flush()
    assert.equal(ui.button('删除演员').disabled, true)
    ui.control('全选当前筛选结果').onChange({ target: { checked: true } }); await ui.flush()
    assert.match(ui.text(), /已选\s+80/)
    assert.equal(ui.control('全选当前筛选结果').checked, true)
    ui.control('演员排序依据').onChange({ target: { value: 'filmCount' } }); await ui.flush()
    ui.button('紧凑密度').onClick(); await ui.flush()
    assert.match(ui.text(), /已选\s+80/)
    const first = ui.nodes(node => node.props?.['data-actor-id'] === 'id-0')[0]
    first.props.onClick(); await ui.flush()
    assert.match(ui.text(), /已选\s+79/)
    assert.equal(ui.control('全选当前筛选结果')['aria-checked'], 'mixed')
    assert.deepEqual(ui.detailChanges, [])
    ui.update({ query: '演员7' }); await ui.flush()
    assert.match(ui.text(), /已选\s+0/)
    ui.control('全选当前筛选结果').onChange({ target: { checked: true } }); await ui.flush()
    assert.match(ui.text(), /已选\s+11/)
    ui.button('退出演员管理').onClick(); await ui.flush()
    ui.nodes(node => node.props?.['data-actor-id'] === 'id-7')[0].props.onClick(); await ui.flush()
    assert.deepEqual(ui.detailChanges, [true])
  } finally { ui.dispose() }
})

test('actor delete cancellation and failed protection preserve selection; success submits exact deduplicated ids once', async () => {
  const ui = actorLibraryComponent(actors)
  try {
    await ui.flush(); ui.button('演员管理').onClick(); await ui.flush()
    ui.nodes(node => node.props?.['data-actor-id'] === 'id-3')[0].props.onClick(); await ui.flush()
    ui.confirm(false); await ui.button('删除演员').onClick(); await ui.flush()
    assert.equal(ui.commands.filter(item => item.command === 'delete_actors').length, 0)
    assert.match(ui.text(), /已选\s+1/)
    ui.confirm(true); ui.fail('delete_actors'); await ui.button('删除演员').onClick(); await ui.flush()
    assert.match(ui.text(), /资料保护/)
    assert.match(ui.text(), /已选\s+1/)
    await ui.button('删除演员').onClick(); await ui.flush()
    assert.deepEqual(Array.from(ui.commands.filter(item => item.command === 'delete_actors').at(-1)!.args.actorIds), ['id-3'])
    assert.equal(ui.nodes(node => node.props?.['data-actor-id'] === 'id-3').length, 0)
    assert.match(ui.confirms[0], /1 位演员/)
    assert.match(ui.confirms[0], /全部出演关系/)
  } finally { ui.dispose() }
})

test('actor tags refresh without duplicate names, rename associations and combine with favorites', async () => {
  const ui = actorLibraryComponent(actors)
  try {
    await ui.flush(); ui.button('标签编辑').onClick(); await ui.flush()
    const manager = () => ui.nodes(node => node.props?.subject === '演员')[0].props
    await manager().onAdd('新标签'); await ui.flush()
    assert.equal(manager().tags.filter((name: string) => name === '新标签').length, 1)
    await manager().onRename('精选', '常看'); await ui.flush()
    assert.ok(manager().tags.includes('常看'))
    assert.equal(ui.nodes(node => node.props?.className === 'actor-card-tags' && node.props.children.some((child: any) => child.props.children === '常看')).length, 40)
    manager().onClose(); await ui.flush()
    const filters = () => ui.nodes(node => typeof node.props?.onToggle === 'function' && Array.isArray(node.props?.selected))[0].props
    filters().onToggle('常看'); await ui.flush()
    assert.equal(ui.nodes(node => node.props?.['data-actor-id']).length, 40)
    ui.button('演员管理').onClick(); await ui.flush()
    ui.control('全选当前筛选结果').onChange({ target: { checked: true } }); await ui.flush()
    assert.match(ui.text(), /已选\s+40/)
    ui.nodes(node => node.props?.active !== undefined)[0].props.onClick(); await ui.flush()
    assert.match(ui.text(), /已选\s+0/)
    assert.equal(ui.nodes(node => node.props?.['data-actor-id']).length, 20)
    ui.control('全选当前筛选结果').onChange({ target: { checked: true } }); await ui.flush()
    filters().onToggle('已看'); await ui.flush()
    assert.match(ui.text(), /已选\s+0/)
    assert.equal(ui.nodes(node => node.props?.['data-actor-id']).length, 0)
  } finally { ui.dispose() }
})
