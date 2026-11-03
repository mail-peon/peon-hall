import { onBeforeUnmount, onMounted } from 'vue'
import * as ipc from '../ipc'
import type { CommandError, Heartbeat, ServiceStatus, Snapshot } from '../types'

const POLL_MS = 2000
const POLL_BACKOFF_MS = 10_000
const SERVICE_STATUS_EVERY = 5

/** Serializes status reads and discards responses superseded by a full refresh. */
export function useRelayPolling(deps: {
  applySnapshot: (value: Snapshot) => void
  applyHeartbeat: (value: Heartbeat) => void
  applyService: (value: ServiceStatus) => void
  applyDetached: (error: CommandError) => void
  onRefreshError: (error: CommandError) => void
  onLoaded: () => void
}) {
  let timer: ReturnType<typeof setTimeout> | undefined
  let disposed = false
  let generation = 0
  let pollCount = 0
  let failures = 0
  let queue = Promise.resolve()
  let refreshFlight: Promise<void> | null = null

  function enqueue(work: () => Promise<void>): Promise<void> {
    const pending = queue.then(() => disposed ? undefined : work())
    queue = pending.catch(() => {})
    return pending
  }

  function refresh(force = false): Promise<void> {
    if (!force && refreshFlight) return refreshFlight
    const revision = ++generation
    const current = () => !disposed && revision === generation
    const pending = enqueue(async () => {
      try {
        const value = await ipc.snapshot()
        if (current()) {
          deps.applySnapshot(value)
          pollCount = 0
          failures = 0
        }
      } catch (error) {
        if (current()) deps.onRefreshError(ipc.asCommandError(error))
      } finally {
        if (current()) deps.onLoaded()
      }
    }).finally(() => {
      if (refreshFlight === pending) refreshFlight = null
    })
    refreshFlight = pending
    return pending
  }

  function clearTimer() {
    clearTimeout(timer)
    timer = undefined
  }

  function schedule() {
    clearTimer()
    if (disposed || document.visibilityState !== 'visible') return
    timer = setTimeout(() => void tick(), failures >= 3 ? POLL_BACKOFF_MS : POLL_MS)
  }

  async function tick() {
    timer = undefined
    if (disposed || document.visibilityState !== 'visible') return
    await enqueue(async () => {
      const revision = generation
      const current = () => !disposed && revision === generation
      pollCount += 1
      try {
        const beat = await ipc.heartbeat()
        if (!current()) return
        failures = 0
        deps.applyHeartbeat(beat)
        if (pollCount % SERVICE_STATUS_EVERY === 0) {
          try {
            const service = await ipc.serviceStatus()
            if (current()) deps.applyService(service)
          } catch {
            // A slow service query must not erase the last known service state.
          }
        }
      } catch (error) {
        if (current()) {
          failures += 1
          deps.applyDetached(ipc.asCommandError(error))
        }
      }
    })
    schedule()
  }

  function onVisible() {
    if (document.visibilityState !== 'visible') {
      clearTimer()
      return
    }
    void refresh()
    schedule()
  }

  onMounted(() => {
    void refresh()
    document.addEventListener('visibilitychange', onVisible)
    window.addEventListener('focus', onVisible)
    schedule()
  })
  onBeforeUnmount(() => {
    disposed = true
    generation += 1
    clearTimer()
    document.removeEventListener('visibilitychange', onVisible)
    window.removeEventListener('focus', onVisible)
  })

  return { refresh }
}
