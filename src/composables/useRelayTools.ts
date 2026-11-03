import type { Ref } from 'vue'
import { onBeforeUnmount, ref } from 'vue'
import * as ipc from '../ipc'
import { doctorText } from '../locales/zh-CN'
import type { DoctorReport, Notice } from '../types'
import { doctorAsText as doctorAsTextPure } from './relay-pure'
import { messageFor, noticeFromError } from './relay-feedback'

const COPIED_MS = 1500

export function useRelayTools(relayUrl: Ref<string>, notice: Ref<Notice | null>) {
  const addressCopied = ref(false)
  const doctorOpen = ref(false)
  const doctorReport = ref<DoctorReport | null>(null)
  const doctorLoading = ref(false)
  const doctorError = ref<string | null>(null)
  const doctorCopied = ref(false)
  let addressTimer: ReturnType<typeof setTimeout> | undefined
  let doctorTimer: ReturnType<typeof setTimeout> | undefined
  onBeforeUnmount(() => {
    clearTimeout(addressTimer)
    clearTimeout(doctorTimer)
  })
  async function loadDoctor() {
    doctorLoading.value = true
    doctorError.value = null
    try {
      // 两路并发：burrow 自己的 doctor + peon-hall 的扩展连通性检查
      const [coreReport, reachable] = await Promise.all([
        ipc.doctor(),
        ipc.checkExtensionReachable(relayUrl.value),
      ])
      doctorReport.value = {
        checks: [
          ...coreReport.checks,
          reachable,
        ],
      }
    } catch (error) {
      doctorReport.value = null
      doctorError.value = messageFor(ipc.asCommandError(error))
    } finally {
      doctorLoading.value = false
    }
  }

  function setDoctorOpen(open: boolean) {
    doctorOpen.value = open
    // 展开时没有结果就现拉一次（上次拉失败的话，再展开会重试）
    if (open && doctorReport.value === null && !doctorLoading.value) void loadDoctor()
  }

  function openDoctor() {
    setDoctorOpen(true)
  }

  /** 诊断结果转纯文本（贴 issue 用）。 */
  function doctorAsText(report: DoctorReport): string {
    return doctorAsTextPure(report, doctorText)
  }

  // ---- 复制 / 打开目录 ----

  async function copyAddress() {
    if (relayUrl.value === '') return
    try {
      // 复制的必须是发现文件里那个地址本身，一个字都不加工（验收 #7）
      await ipc.copyText(relayUrl.value)
      addressCopied.value = true
      clearTimeout(addressTimer)
      addressTimer = setTimeout(() => {
        addressCopied.value = false
      }, COPIED_MS)
    } catch (error) {
      notice.value = noticeFromError(ipc.asCommandError(error))
    }
  }

  async function copyDoctor() {
    const report = doctorReport.value
    if (report === null) return
    try {
      await ipc.copyText(doctorAsText(report))
      doctorCopied.value = true
      clearTimeout(doctorTimer)
      doctorTimer = setTimeout(() => {
        doctorCopied.value = false
      }, COPIED_MS)
    } catch (error) {
      notice.value = noticeFromError(ipc.asCommandError(error))
    }
  }

  async function openConfigDir() {
    try {
      await ipc.openConfig()
    } catch (error) {
      notice.value = noticeFromError(ipc.asCommandError(error))
    }
  }

  async function openLogsDir() {
    try {
      await ipc.openLogs()
    } catch (error) {
      notice.value = noticeFromError(ipc.asCommandError(error))
    }
  }

  return { addressCopied, copyAddress, doctorOpen, doctorReport, doctorLoading, doctorError,
    doctorCopied, setDoctorOpen, openDoctor, loadDoctor, copyDoctor, openConfigDir, openLogsDir }
}
