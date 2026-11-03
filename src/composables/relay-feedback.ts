import type { CommandError, Notice, NoticeAction } from '../types'
import { notice as noticeText } from '../locales/zh-CN'
import { messageFor as messageForPure, noticeFromError as noticeFromErrorPure } from './relay-pure'

export function messageFor(error: CommandError): string {
  return messageForPure(error, noticeText)
}
export function doctorAction(): NoticeAction {
  return { label: noticeText.needDoctor, kind: 'doctor' }
}
export function noticeFromError(error: CommandError): Notice {
  return noticeFromErrorPure(error, noticeText)
}
