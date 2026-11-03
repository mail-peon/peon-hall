import { flushPromises, mount } from '@vue/test-utils'
import { defineComponent, h, ref } from 'vue'
import { expect, it, vi } from 'vitest'
import { useCommandTerminal } from '../useCommandTerminal'

const mocks = vi.hoisted(() => ({ listen: vi.fn(), dispose: vi.fn() }))
vi.mock('../../ipc', () => ({ onCommandEvent: mocks.listen }))
vi.mock('@xterm/xterm', () => ({ Terminal: class {
  loadAddon() {}
  open() {}
  dispose() { mocks.dispose() }
  attachCustomKeyEventHandler() {}
} }))
vi.mock('@xterm/addon-fit', () => ({ FitAddon: class { fit() {} } }))
vi.mock('@xterm/addon-search', () => ({ SearchAddon: class {} }))

it('卸载后才完成的事件订阅会立即释放', async () => {
  let subscribed!: (stop: () => void) => void
  mocks.listen.mockReturnValue(new Promise((resolve) => { subscribed = resolve }))
  const wrapper = mount(defineComponent({ setup() {
    const host = ref<HTMLDivElement | null>(null)
    useCommandTerminal(host, () => true)
    return () => h('div', { ref: host })
  } }))
  await flushPromises()
  expect(mocks.listen).toHaveBeenCalledOnce()
  wrapper.unmount()
  const stop = vi.fn()
  subscribed(stop)
  await flushPromises()
  expect(stop).toHaveBeenCalledOnce()
  expect(mocks.dispose).toHaveBeenCalledOnce()
})
