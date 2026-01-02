import { useState, useCallback, useMemo, useRef, useEffect } from 'react';
import { getLastScreenshotCaptures } from '@shared/api/system';
import { createStageRegionManager } from '../utils/stageRegionManager';

const workerUrl = new URL('../workers/bmpDecodeWorker.js', import.meta.url);

export default function useScreenshotStage() {
  const [screens, setScreens] = useState([]);
  const [stageSize, setStageSize] = useState({ width: 0, height: 0 });

  const stageRegionManager = useMemo(() => createStageRegionManager(screens), [screens]);
  const maxConcurrentWorkers = useMemo(() => {
    if (typeof navigator !== 'undefined' && navigator.hardwareConcurrency) {
      return Math.max(1, Math.min(4, navigator.hardwareConcurrency));
    }
    return 2;
  }, []);

  const workerPoolRef = useRef([]);
  const requestIdRef = useRef(0);
  const decodeQueueRef = useRef([]);

  const terminateWorkers = useCallback(() => {
    workerPoolRef.current.forEach((entry) => {
      try {
        entry.worker.terminate();
      } catch {
      }
    });
    workerPoolRef.current = [];
    decodeQueueRef.current = [];
  }, []);

  const removeWorkerEntry = (target) => {
    workerPoolRef.current = workerPoolRef.current.filter((entry) => entry !== target);
  };

  const createWorkerEntry = () => {
    if (typeof window === 'undefined' || typeof Worker === 'undefined') {
      throw new Error('Worker 不可用');
    }

    const worker = new Worker(workerUrl, { type: 'module' });
    const entry = {
      worker,
      busy: false,
      task: null,
    };

    worker.onmessage = (event) => {
      const { success, bitmap, error } = event.data || {};
      const currentTask = entry.task;
      entry.task = null;
      entry.busy = false;

      if (currentTask) {
        if (success) {
          currentTask.resolve(bitmap);
        } else {
          currentTask.reject(new Error(error || 'Worker 解码失败'));
        }
      }

      processDecodeQueue();
    };

    worker.onerror = (event) => {
      const currentTask = entry.task;
      entry.task = null;
      entry.busy = false;
      removeWorkerEntry(entry);
      try {
        worker.terminate();
      } catch {}
      if (currentTask) {
        currentTask.reject(new Error(event?.message || 'Worker error'));
      }
      processDecodeQueue();
    };

    workerPoolRef.current.push(entry);
    return entry;
  };

  const ensureWorkerPool = useCallback(() => {
    if (typeof window === 'undefined' || typeof Worker === 'undefined') {
      return false;
    }
    while (workerPoolRef.current.length < maxConcurrentWorkers) {
      try {
        createWorkerEntry();
      } catch (error) {
        console.error('[BMP Worker] 初始化失败:', error);
        break;
      }
    }
    return workerPoolRef.current.length > 0;
  }, [maxConcurrentWorkers]);

  const processDecodeQueue = useCallback(() => {
    if (!decodeQueueRef.current.length) return;

    workerPoolRef.current.forEach((entry) => {
      if (!decodeQueueRef.current.length) return;
      if (entry.busy) return;

      const task = decodeQueueRef.current.shift();
      if (!task) return;

      entry.busy = true;
      entry.task = task;
      try {
        entry.worker.postMessage({ id: task.id, url: task.url });
      } catch (error) {
        entry.busy = false;
        entry.task = null;
        task.reject(error);
        removeWorkerEntry(entry);
      }
    });

    if (decodeQueueRef.current.length && workerPoolRef.current.every((entry) => entry.busy)) {
      return;
    }
  }, []);

  const decodeWithWorker = useCallback((url) => {
    return new Promise((resolve, reject) => {
      if (!ensureWorkerPool()) {
        reject(new Error('Worker 不可用'));
        return;
      }
      const id = requestIdRef.current++;
      decodeQueueRef.current.push({ id, url, resolve, reject });
      processDecodeQueue();
    });
  }, [processDecodeQueue, ensureWorkerPool]);

  const loadImageFallback = useCallback((url) => {
    return new Promise((resolve, reject) => {
      const img = new window.Image();
      img.crossOrigin = 'anonymous';
      img.onload = () => resolve(img);
      img.onerror = (e) => reject(e);
      img.src = url;
    });
  }, []);

  const loadScreenBitmap = useCallback(async (url) => {
    try {
      return await decodeWithWorker(url);
    } catch (error) {
      console.warn('[Screenshot] Worker解码失败，使用 Image:', error);
      return loadImageFallback(url);
    }
  }, [decodeWithWorker, loadImageFallback]);

  useEffect(() => () => terminateWorkers(), [terminateWorkers]);

  const reloadFromLastCapture = useCallback(async () => {
    try {
      let infos = await getLastScreenshotCaptures();

      if (!infos || !infos.length) {
        return;
      }

      const ua = String(globalThis?.navigator?.userAgent || '').toLowerCase();
      const isMac = ua.includes('mac os x') || ua.includes('macintosh');

      const resolveMonitorIndex = async () => {
        try {
          const { getCurrentWebviewWindow } = await import('@tauri-apps/api/webviewWindow');
          const label = getCurrentWebviewWindow()?.label;
          if (label === 'screenshot') return 0;
          const match = String(label || '').match(/^screenshot-(\d+)$/);
          if (match) return Number(match[1]);
        } catch {
        }

        try {
          const params = new URLSearchParams(window.location.search);
          const val = params.get('monitor');
          if (val == null) return null;
          const idx = Number(val);
          return Number.isFinite(idx) ? idx : null;
        } catch {
          return null;
        }
      };

      const monitorIndex = await resolveMonitorIndex();
      if (monitorIndex != null) {
        const idx = Math.max(0, Math.floor(monitorIndex));
        const single = infos[idx] ? [infos[idx]] : [];
        if (!single.length) {
          console.warn('[Screenshot] monitorIndex 无效:', monitorIndex, 'infos.length=', infos.length);
          return;
        }
        infos = single;
      }

      const loadedScreens = await Promise.all(
        infos.map(async (m) => {
          const imageSource = await loadScreenBitmap(m.file_path);

          if (isMac) {
            const logicalX = m.logical_x ?? 0;
            const logicalY = m.logical_y ?? 0;
            const logicalW = m.logical_width ?? 0;
            const logicalH = m.logical_height ?? 0;

            return {
              image: imageSource,
              x: logicalX,
              y: logicalY,
              width: logicalW,
              height: logicalH,
              physicalX: m.physical_x,
              physicalY: m.physical_y,
              physicalWidth: m.physical_width,
              physicalHeight: m.physical_height,
              physicalOffsetX: 0,
              physicalOffsetY: 0,
              scaleFactor: m.scale_factor,
            };
          }

          const dpr = window.devicePixelRatio || 1;
          const cssX = (m.physical_x) / dpr;
          const cssY = (m.physical_y) / dpr;
          const cssWidth = m.physical_width / dpr;
          const cssHeight = m.physical_height / dpr;
          
          return {
            image: imageSource,
            // CSS坐标（用于Konva渲染）
            x: cssX,
            y: cssY,
            width: cssWidth,
            height: cssHeight,
            // 物理坐标（用于坐标显示、导出等）
            physicalX: m.physical_x,
            physicalY: m.physical_y,
            physicalWidth: m.physical_width,
            physicalHeight: m.physical_height,
            // 物理偏移（用于坐标转换）
            physicalOffsetX: 0,
            physicalOffsetY: 0,
            scaleFactor: m.scale_factor,
          };
        })
      );

      if (isMac) {
        const minX = Math.min(...loadedScreens.map((s) => s.x));
        const minY = Math.min(...loadedScreens.map((s) => s.y));
        const maxX = Math.max(...loadedScreens.map((s) => s.x + s.width));
        const maxY = Math.max(...loadedScreens.map((s) => s.y + s.height));

        const offsetX = isFinite(minX) ? minX : 0;
        const offsetY = isFinite(minY) ? minY : 0;

        loadedScreens.forEach((s) => {
          s.x -= offsetX;
          s.y -= offsetY;
        });

        setStageSize({ width: maxX - offsetX, height: maxY - offsetY });
      } else {
        const minX = Math.min(...loadedScreens.map((s) => s.x));
        const minY = Math.min(...loadedScreens.map((s) => s.y));
        const maxX = Math.max(...loadedScreens.map((s) => s.x + s.width));
        const maxY = Math.max(...loadedScreens.map((s) => s.y + s.height));

        const offsetX = isFinite(minX) ? minX : 0;
        const offsetY = isFinite(minY) ? minY : 0;

        loadedScreens.forEach((s) => {
          s.x -= offsetX;
          s.y -= offsetY;
        });

        setStageSize({ width: maxX - offsetX, height: maxY - offsetY });
      }

      setScreens(loadedScreens);
    } catch (error) {
      console.error('加载截屏数据失败:', error);
    }
  }, []);

  return { screens, stageSize, stageRegionManager, reloadFromLastCapture };
}
