import assert from 'node:assert/strict'
import test from 'node:test'
import { createPressFeedback } from '../src/data/motion.ts'

function button() {
 const classes = new Set<string>()
 const animations: Array<{ cancelled: number; onfinish: (() => void) | null; oncancel: (() => void) | null; cancel: () => void }> = []
 const target = { classList: { add: (name: string) => classes.add(name), remove: (name: string) => classes.delete(name) } as unknown as DOMTokenList, animate() {
  const animation = { cancelled: 0, onfinish: null as (() => void) | null, oncancel: null as (() => void) | null, cancel() { this.cancelled++; this.oncancel?.() } }
  animations.push(animation); return animation as unknown as Animation
 } }
 return { target, animations, classes }
}

test('rapid repeat replaces both effects; stale completion cannot release the new feedback', () => {
 const feedback = createPressFeedback(), item = button()
 feedback.press(item.target); feedback.press(item.target)
 assert.ok(item.animations.slice(0, 2).every(animation => animation.cancelled === 1))
 item.animations[0].onfinish?.(); item.animations[1].onfinish?.()
 assert.ok(item.classes.has('button-acknowledged'))
 feedback.clear(); assert.ok(item.animations.slice(2).every(animation => animation.cancelled === 1)); assert.equal(item.classes.size, 0)
})

test('burst stays bounded; finishing all effects releases the target and acknowledgement', () => {
 const feedback = createPressFeedback(), items = Array.from({ length: 10 }, button)
 items.forEach(item => feedback.press(item.target))
 assert.equal(items.filter(item => item.animations[0].cancelled).length, 2)
 items[2].animations[0].onfinish?.(); assert.ok(items[2].classes.size > 0)
 items[2].animations[1].onfinish?.(); assert.equal(items[2].classes.size, 0)
 feedback.clear(); assert.equal(items[2].animations[0].cancelled, 0)
 assert.ok(items.slice(3).every(item => item.animations.every(animation => animation.cancelled === 1)))
 assert.ok(items.every(item => item.classes.size === 0))
})

test('unavailable animation preserves a static acknowledgement and does not block the action', () => {
 const feedback = createPressFeedback(), item = button()
 const target = { ...item.target, animate() { throw new Error('not supported') } }
 assert.doesNotThrow(() => feedback.press(target)); assert.equal(item.classes.size, 1)
 assert.doesNotThrow(() => feedback.clear()); assert.equal(item.classes.size, 0)
})

test('reduced motion has no animation and releases the static acknowledgement', async () => {
 const feedback = createPressFeedback(), item = button()
 feedback.press(item.target, { motion: false }); assert.equal(item.animations.length, 0); assert.equal(item.classes.size, 1)
 await new Promise(resolve => setTimeout(resolve, 270)); assert.equal(item.classes.size, 0)
 feedback.clear()
})

test('keyboard focus is preserved and reduced feedback never removes the focus cue', () => {
 const feedback = createPressFeedback(), item = button()
 feedback.press(item.target, { preserveFocus: true }); assert.equal(item.animations.length, 1); assert.equal(item.classes.size, 0)
 item.animations[0].onfinish?.(); feedback.clear(); assert.equal(item.animations[0].cancelled, 0)
 feedback.press(item.target, { preserveFocus: true, motion: false }); assert.equal(item.animations.length, 1)
 feedback.clear(); assert.equal(item.classes.size, 0)
})

test('external cancellation cleans up the paired effect and permits another click', () => {
 const feedback = createPressFeedback(), item = button()
 feedback.press(item.target); item.animations[1].cancel()
 assert.equal(item.classes.size, 0); assert.equal(item.animations[0].cancelled, 1)
 feedback.press(item.target); assert.equal(item.animations.length, 4); feedback.clear(); assert.equal(item.classes.size, 0)
})
