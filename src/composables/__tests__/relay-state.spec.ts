import { flushPromises, mount } from '@vue/test-utils'
import { defineComponent } from 'vue'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { useRelayState } from '../useRelayState'
import type { Snapshot } from '../../types'

const mocks = vi.hoisted(() => ({ snapshot: vi.fn(), doctor: vi.fn() }))

vi.mock('../../ipc', () => ({
  snapshot: mocks.snapshot,
  doctor: mocks.doctor,
  checkExtensionReachable: async () => ({ level: 'ok', title: 'relay', detail: '' }),
  asCommandError: (error: unknown) => ({ code: 'failed', message: String(error) }),
}))

afterEach(() => vi.useRealTimers())

describe('状态提示的操作', () => {
  it('服务状态读取失败时点击诊断会打开诊断面板', async () => {
    vi.useFakeTimers()
    mocks.snapshot.mockResolvedValue({
      phase: 'serviceUnknown', service: null, serviceError: 'permission denied',
      process: null, controlError: null, discovery: 'missing', discoveryDetail: '',
      relayUrl: 'ws://127.0.0.1:41316/', urlSource: 'default',
      sidecarPresent: true, appVersion: '0.1.0', coreBound: '0.1.0',
    } satisfies Snapshot)
    mocks.doctor.mockResolvedValue({ checks: [] })
    let state!: ReturnType<typeof useRelayState>
    const wrapper = mount(defineComponent({
      setup() {
        state = useRelayState()
        return () => null
      },
    }))
    await flushPromises()
    expect(state.visibleNotice.value?.action?.kind).toBe('doctor')
    state.runNoticeAction()
    await flushPromises()
    expect(state.doctorOpen.value).toBe(true)
    expect(mocks.doctor).toHaveBeenCalledOnce()
    wrapper.unmount()
  })
})
