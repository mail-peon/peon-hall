import { flushPromises, mount } from '@vue/test-utils'
import { defineComponent } from 'vue'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { useRelayPolling } from '../useRelayPolling'
import type { Heartbeat, Snapshot } from '../../types'

const ipc = vi.hoisted(() => ({ snapshot: vi.fn(), heartbeat: vi.fn(), serviceStatus: vi.fn() }))
vi.mock('../../ipc', () => ({ ...ipc, asCommandError: (error: unknown) => error }))

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => { resolve = done })
  return { promise, resolve }
}

function mountPolling() {
  const deps = {
    applySnapshot: vi.fn(), applyHeartbeat: vi.fn(), applyService: vi.fn(),
    applyDetached: vi.fn(), onRefreshError: vi.fn(), onLoaded: vi.fn(),
  }
  let polling!: ReturnType<typeof useRelayPolling>
  const wrapper = mount(defineComponent({ setup() {
    polling = useRelayPolling(deps)
    return () => null
  } }))
  return { wrapper, deps, get polling() { return polling } }
}

beforeEach(() => {
  vi.useFakeTimers()
  vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible')
  vi.resetAllMocks()
})
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks() })

it('合并同时到达的窗口焦点和可见性刷新', async () => {
  const pending = deferred<Snapshot>()
  ipc.snapshot.mockReturnValue(pending.promise)
  const state = mountPolling()
  await flushPromises()
  window.dispatchEvent(new Event('focus'))
  document.dispatchEvent(new Event('visibilitychange'))
  await flushPromises()
  expect(ipc.snapshot).toHaveBeenCalledOnce()
  state.wrapper.unmount()
  pending.resolve({} as Snapshot)
  await flushPromises()
  expect(state.deps.applySnapshot).not.toHaveBeenCalled()
})

it('强制刷新期间丢弃旧心跳并串行读取新快照', async () => {
  const pending = deferred<Heartbeat>()
  ipc.snapshot.mockResolvedValue({} as Snapshot)
  ipc.heartbeat.mockReturnValue(pending.promise)
  const state = mountPolling()
  await flushPromises()
  vi.advanceTimersByTime(2000)
  await flushPromises()
  expect(ipc.heartbeat).toHaveBeenCalledOnce()
  const refreshing = state.polling.refresh(true)
  expect(ipc.snapshot).toHaveBeenCalledOnce()
  pending.resolve({} as Heartbeat)
  await refreshing
  expect(state.deps.applyHeartbeat).not.toHaveBeenCalled()
  expect(ipc.snapshot).toHaveBeenCalledTimes(2)
  state.wrapper.unmount()
})
