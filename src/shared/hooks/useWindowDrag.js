import { useEffect, useRef } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { startCustomDrag, stopCustomDrag } from '@shared/api'
import { isWindows } from '@shared/utils/platform'

// 自定义窗口拖拽 Hook
export function useWindowDrag(options = {}) {
  const { excludeSelectors = [], allowChildren = false } = options
  const elementRef = useRef(null)
  const isDraggingRef = useRef(false)
  const dragEndCleanupRef = useRef(null)
  const isStoppingRef = useRef(false)

  useEffect(() => {
    const element = elementRef.current
    if (!element) return

    const unlistenPromise = getCurrentWindow().listen('drag-ended', () => {
      document.body.style.userSelect = ''
      document.body.style.cursor = ''
      isDraggingRef.current = false
      isStoppingRef.current = false
      if (dragEndCleanupRef.current) {
        dragEndCleanupRef.current()
        dragEndCleanupRef.current = null
      }
    })

    const handleMouseDown = async (e) => {
      if (!allowChildren && e.target !== element) {
        return
      }

      for (const selector of excludeSelectors) {
        if (e.target.closest(selector)) {
          return
        }
      }

      if (e.buttons !== 1) {
        return
      }

      startDrag(e)
    }

    const startDrag = async (initialEvent) => {
      if (isDraggingRef.current) return
      isDraggingRef.current = true

      try {
        document.body.style.userSelect = 'none'
        document.body.style.cursor = 'move'

        const isWin = await isWindows()
        if (!isWin && !dragEndCleanupRef.current) {
          const appWindow = getCurrentWindow()
          let moveEndTimer = null
          let fallbackTimer = null

          const requestStop = async () => {
            if (!isDraggingRef.current || isStoppingRef.current) return
            isStoppingRef.current = true
            try {
              await stopCustomDrag()
            } catch (err) {
              console.warn('停止拖拽失败:', err)
              document.body.style.userSelect = ''
              document.body.style.cursor = ''
              isDraggingRef.current = false
              isStoppingRef.current = false
            }
          }

          const unlistenMove = await appWindow.onMoved(() => {
            if (fallbackTimer) {
              clearTimeout(fallbackTimer)
              fallbackTimer = null
            }
            if (moveEndTimer) clearTimeout(moveEndTimer)
            moveEndTimer = setTimeout(requestStop, 180)
          })

          const handleMouseUp = () => requestStop()
          document.addEventListener('mouseup', handleMouseUp, true)

          fallbackTimer = setTimeout(requestStop, 1200)

          dragEndCleanupRef.current = () => {
            if (moveEndTimer) clearTimeout(moveEndTimer)
            if (fallbackTimer) clearTimeout(fallbackTimer)
            try {
              unlistenMove()
            } catch (_) {}
            document.removeEventListener('mouseup', handleMouseUp, true)
          }
        }

        await startCustomDrag(initialEvent.screenX, initialEvent.screenY)

        initialEvent.preventDefault()
      } catch (error) {
        console.error('启动拖拽失败:', error)
        isDraggingRef.current = false
        isStoppingRef.current = false
        document.body.style.userSelect = ''
        document.body.style.cursor = ''
        if (dragEndCleanupRef.current) {
          dragEndCleanupRef.current()
          dragEndCleanupRef.current = null
        }
      }
    }

    element.addEventListener('mousedown', handleMouseDown)

    return () => {
      element.removeEventListener('mousedown', handleMouseDown)
      unlistenPromise.then(unlisten => unlisten())
      if (dragEndCleanupRef.current) {
        dragEndCleanupRef.current()
        dragEndCleanupRef.current = null
      }
    }
  }, [excludeSelectors, allowChildren])

  return elementRef
}
