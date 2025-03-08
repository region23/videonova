<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed, watch } from 'vue'
import { listen } from '@tauri-apps/api/event'
import DownloadProgress from './DownloadProgress.vue'
import TranscriptionProgress from './TranscriptionProgress.vue'
import TranslationProgress from './TranslationProgress.vue'
import TTSProgress from './TTSProgress.vue'
import MergeProgress from './MergeProgress.vue'
import TranslationComplete from './TranslationComplete.vue'
import ProgressStepper from './ProgressStepper.vue'

interface VideoInfo {
  title: string
  duration: number
  url: string
  thumbnail: string
  description: string
}

interface DownloadProgress {
  status: string
  progress: number
  speed?: string
  eta?: string
  component: string
}

// Добавляем интерфейс для прогресса транскрибации
interface TranscriptionProgress {
  status: string
  progress: number
}

// Добавляем интерфейс для прогресса перевода
interface TranslationProgress {
  status: string
  progress: number
}

// Добавляем интерфейс для прогресса озвучки
interface TTSProgress {
  status: string
  progress: number
  current_segment?: number
  total_segments?: number
}

// Добавляем интерфейс для результата финального объединения
interface MergeResult {
  output_path: string
  output_dir: string
}

// Добавляем интерфейс для прогресса финального объединения
interface MergeProgress {
  status: string
  progress: number
}

// Add defined events
const emit = defineEmits<{
  'merge-complete': [outputDir: string]
  'loading-state-change': [isLoading: boolean]
  'clear-video-info': []
  'video-info-ready-state-change': [isReady: boolean]
}>()

// Define props
const props = defineProps<{
  videoInfo?: VideoInfo | null
  transcriptionProgress?: any
  translationProgress?: any
  ttsProgress?: any
  mergeProgress?: any
  isLoading?: boolean
  youtubeUrl?: string
}>()

const audioProgress = ref<DownloadProgress | null>(null)
const videoProgress = ref<DownloadProgress | null>(null)
// Добавляем переменную для прогресса транскрибации
const transcriptionProgress = ref<TranscriptionProgress | null>(null)
// Добавляем переменную для прогресса перевода
const translationProgress = ref<TranslationProgress | null>(null)
// Добавляем переменную для прогресса озвучки
const ttsProgress = ref<TTSProgress | null>(null)
const mergeProgress = ref<MergeProgress | null>(null)
const translationComplete = ref(false)
const outputDirectory = ref<string | null>(null)
const isTranscribing = ref(false)
const isTranslating = ref(false)
const isTTSGenerating = ref(false)
const isMerging = ref(false)
const mergeError = ref<string | null>(null)

// Internal loading state that combines prop and detected loading
const internalIsLoading = ref(false);
const previousUrl = ref<string | null>(null);
const shouldHideVideoInfo = ref(false);
// Explicitly set initial state to not ready
const initialLoadDone = ref(false);

// Computed property to determine if video info is ready for processing
const isVideoInfoReady = computed(() => {
  // При первой загрузке всегда возвращаем false
  if (!initialLoadDone.value) {
    return false;
  }
  
  // Видео готово к обработке, если:
  // 1. Есть информация о видео
  // 2. Информация не должна быть скрыта
  // 3. Нет активной загрузки
  // 4. Есть URL видео
  return Boolean(
    props.videoInfo && 
    !shouldHideVideoInfo.value && 
    !internalIsLoading.value &&
    props.videoInfo.url
  );
});

// Watch for changes in video info ready state and emit event
watch(isVideoInfoReady, (newVal) => {
  console.log('Video info ready state changed to:', newVal);
  emit('video-info-ready-state-change', newVal);
});

// Watch for changes to videoInfo to detect loading and clearing
watch(() => props.videoInfo, (newVal, oldVal) => {
  console.log('VideoInfo changed:', { 
    newVal: newVal ? 'present' : 'null/undefined', 
    oldVal: oldVal ? 'present' : 'null/undefined',
    previousUrl: previousUrl.value,
    initialLoadDone: initialLoadDone.value
  });
  
  // If videoInfo is not null, we've loaded the info (either first time or after clearing)
  if (newVal !== null && newVal !== undefined) {
    console.log('Video info loaded or updated');
    internalIsLoading.value = false;
    previousUrl.value = newVal.url;
    shouldHideVideoInfo.value = false;
    // Mark initial load as done
    initialLoadDone.value = true;
    // Video is now ready for processing
    emit('video-info-ready-state-change', true);
  }
  
  // If videoInfo was not null and now it's null, it's been cleared
  else if (oldVal !== null && (newVal === null || newVal === undefined)) {
    console.log('Video info cleared');
    internalIsLoading.value = false;
    previousUrl.value = null;
    shouldHideVideoInfo.value = true;
    // Reset initial load flag
    initialLoadDone.value = false;
    // Video is no longer ready for processing
    emit('video-info-ready-state-change', false);
  }
  
  // Direct check - if videoInfo is null, ensure loading state is reset
  else if (newVal === null || newVal === undefined) {
    console.log('VideoInfo is null, ensuring loading state is reset');
    internalIsLoading.value = false;
    if (previousUrl.value) {
      shouldHideVideoInfo.value = true;
      // Reset initial load flag
      initialLoadDone.value = false;
      // Video is no longer ready for processing
      emit('video-info-ready-state-change', false);
    }
  }
}, { immediate: true });

// Watch for changes to isLoading prop
watch(() => props.isLoading, (newVal) => {
  if (newVal === true) {
    internalIsLoading.value = true;
  } else if (newVal === false) {
    internalIsLoading.value = false;
  }
}, { immediate: true });

// Watch for changes to the youtubeUrl prop
watch(() => props.youtubeUrl, (newUrl, oldUrl) => {
  console.log('YouTube URL prop changed:', { newUrl, oldUrl, previousUrl: previousUrl.value });
  
  // Если URL пустой или удален, сразу отправляем событие о неготовности видео
  if (!newUrl || newUrl.trim() === '') {
    console.log('Empty URL detected, video info is not ready');
    initialLoadDone.value = false;
    emit('video-info-ready-state-change', false);
  }
  
  if (newUrl && (newUrl.includes('youtube.com') || newUrl.includes('youtu.be'))) {
    // If we have a new YouTube URL and it's different from the previous one
    if (!previousUrl.value || !newUrl.includes(previousUrl.value)) {
      console.log('YouTube URL prop changed, setting loading state');
      resetState(); // Reset state first
      internalIsLoading.value = true;
      emit('loading-state-change', true);
      // Video is not ready while loading
      emit('video-info-ready-state-change', false);
    }
  } else if (!newUrl || newUrl.trim() === '') {
    // If the URL is cleared or empty
    console.log('YouTube URL prop cleared or empty, resetting state and hiding video info');
    internalIsLoading.value = false;
    previousUrl.value = null;
    shouldHideVideoInfo.value = true;
    initialLoadDone.value = false;
    emit('loading-state-change', false);
    emit('clear-video-info');
    // Video is not ready when URL is cleared
    emit('video-info-ready-state-change', false);
  }
}, { immediate: true });

// Add a watcher for shouldHideVideoInfo to log when it changes
watch(shouldHideVideoInfo, (newVal) => {
  console.log('shouldHideVideoInfo changed to:', newVal);
});

// Create a cleanup function for the event listener
let unlisten: (() => void) | null = null;
let urlInputListener: ((event: Event) => void) | null = null;

// Add watchers for state changes
watch(isTranscribing, (newVal) => {
  console.log('isTranscribing changed to:', newVal);
});

// Добавляем watch для проверки завершения загрузки
watch([() => audioProgress.value?.progress, () => videoProgress.value?.progress], 
  ([audioProgressValue, videoProgressValue]) => {
    if (audioProgressValue === 100 && videoProgressValue === 100) {
      console.log('Both downloads completed, resetting progress after delay');
      // Добавляем небольшую задержку перед сбросом, чтобы пользователь видел 100%
      setTimeout(() => {
        // Сбрасываем прогресс, если не начался следующий процесс
        if (!isTranscribing.value && !isTranslating.value && 
            !isTTSGenerating.value && !isMerging.value) {
          audioProgress.value = null;
          videoProgress.value = null;
        } else {
          // Если начался следующий процесс, просто скрываем индикаторы загрузки
          // без изменения переменных, что позволит корректно отображать шаги в степпере
          console.log('Next process already started, hiding download progress');
        }
      }, 1000);
    }
  }
);

watch(isTranslating, (newVal) => {
  console.log('isTranslating changed to:', newVal);
});

watch(isTTSGenerating, (newVal) => {
  console.log('isTTSGenerating changed to:', newVal);
});

watch(isMerging, (newVal) => {
  console.log('isMerging changed to:', newVal);
});

// Функция для логирования общего состояния компонента
function logComponentState(reason: string) {
  console.log(`State update (${reason}):`, {
    currentStep: currentStep.value,
    isTranscribing: isTranscribing.value,
    isTranslating: isTranslating.value, 
    isTTSGenerating: isTTSGenerating.value,
    isMerging: isMerging.value,
    audioProgress: audioProgress.value ? 
      `${audioProgress.value.progress}% - ${audioProgress.value.status}` : 'none',
    videoProgress: videoProgress.value ? 
      `${videoProgress.value.progress}% - ${videoProgress.value.status}` : 'none',
    transcriptionProgress: transcriptionProgress.value ? 
      `${transcriptionProgress.value.progress}% - ${transcriptionProgress.value.status}` : 'none',
    translationProgress: translationProgress.value ? 
      `${translationProgress.value.progress}% - ${translationProgress.value.status}` : 'none',
    ttsProgress: ttsProgress.value ? 
      `${ttsProgress.value.progress}%` : 'none',
    mergeProgress: mergeProgress.value ? 
      `${mergeProgress.value.progress}% - ${mergeProgress.value.status}` : 'none',
  });
}

// Add utility for throttling TTS updates (more aggressive than debounce)
// This ensures only one update per specified time window
const ttsUpdateQueue: TTSProgress[] = [];
let processingTTSQueue = false;

function queueTTSUpdate(progress: TTSProgress) {
  // Add to queue, replacing any previous item if same segment
  const existingIndex = ttsUpdateQueue.findIndex(p => 
    p.current_segment === progress.current_segment
  );
  
  if (existingIndex >= 0) {
    ttsUpdateQueue[existingIndex] = progress;
  } else {
    ttsUpdateQueue.push(progress);
  }
  
  // Start processing if not already running
  if (!processingTTSQueue) {
    processTTSQueue();
  }
}

function processTTSQueue() {
  if (ttsUpdateQueue.length === 0) {
    processingTTSQueue = false;
    return;
  }
  
  processingTTSQueue = true;
  
  // Process next item in queue
  const nextProgress = ttsUpdateQueue.shift();
  if (nextProgress) {
    // Apply the update
    setTimeout(() => {
      // Only update UI for full percent changes to reduce processing
      const shouldUpdate = 
        !ttsProgress.value || 
        Math.floor(ttsProgress.value.progress) !== Math.floor(nextProgress.progress) ||
        ttsProgress.value.current_segment !== nextProgress.current_segment ||
        nextProgress.progress === 100;
      
      if (shouldUpdate) {
        ttsProgress.value = nextProgress;
        
        // Update flags only at important points
        if (nextProgress.progress === 0) {
          console.log('TTS process starting');
          isTTSGenerating.value = true;
        } else if (nextProgress.progress === 100) {
          console.log('TTS process complete');
          // Schedule flag update and cleanup with delay
          setTimeout(() => {
            isTTSGenerating.value = false;
            setTimeout(() => ttsProgress.value = null, 2000);
          }, 500);
        }
      }
      
      // Schedule next item with a small delay to prevent UI blocking
      setTimeout(processTTSQueue, 100);
    }, 0);
  }
}

// Функция для отладки блокировки UI
function trackUIBlocking(operation: string) {
  const start = performance.now();
  console.log(`Start UI operation: ${operation}`);
  
  // Проверяем через 50ms, не блокирован ли UI
  setTimeout(() => {
    const end = performance.now();
    const elapsed = end - start;
    
    if (elapsed > 100) {
      console.warn(`UI might be blocked during ${operation}. Elapsed: ${elapsed.toFixed(2)}ms`);
    } else {
      console.log(`UI operation completed: ${operation}. Elapsed: ${elapsed.toFixed(2)}ms`);
    }
  }, 50);
}

// Setup progress listener
onMounted(async () => {
  console.log('=== VideoPreview Component Mounted ===');
  console.log('Initial States:', {
    isTranscribing: isTranscribing.value,
    isTranslating: isTranslating.value,
    isTTSGenerating: isTTSGenerating.value,
    isMerging: isMerging.value,
    currentStep: currentStep.value,
    audioProgress: audioProgress.value,
    videoProgress: videoProgress.value,
    translationComplete: translationComplete.value
  });

  // Explicitly set initial video info ready state to false
  emit('video-info-ready-state-change', false);

  // Add a method to detect when a URL is being entered
  urlInputListener = (event: Event) => {
    const target = event.target as HTMLInputElement;
    console.log('Input event detected:', { 
      value: target?.value, 
      previousUrl: previousUrl.value,
      hasValue: Boolean(target?.value),
      isEmpty: !target?.value || target.value.trim() === ''
    });
    
    // Если поле ввода пустое, сразу отправляем событие о неготовности видео
    if (target && (!target.value || target.value.trim() === '')) {
      console.log('Empty input detected, video info is not ready');
      initialLoadDone.value = false;
      emit('video-info-ready-state-change', false);
      // Также сбрасываем состояние и скрываем информацию о видео
      internalIsLoading.value = false;
      previousUrl.value = null;
      shouldHideVideoInfo.value = true;
      emit('clear-video-info');
    }
    // Если поле ввода содержит YouTube URL
    else if (target && target.value && (target.value.includes('youtube.com') || target.value.includes('youtu.be'))) {
      // If the input contains a YouTube URL and it's different from the previous one
      if (!previousUrl.value || !target.value.includes(previousUrl.value)) {
        console.log('YouTube URL detected in input, setting loading state');
        resetState(); // Reset state first
        internalIsLoading.value = true;
        // Video is not ready while loading
        emit('video-info-ready-state-change', false);
      }
    }
  };
  
  window.addEventListener('input', urlInputListener);

  unlisten = await listen<DownloadProgress>('download-progress', (event) => {
    const progress = event.payload;
    if (progress.component === 'audio') {
      audioProgress.value = progress;
      if (progress.progress === 100) {
        logComponentState('audio download complete');
      }
    } else if (progress.component === 'video') {
      videoProgress.value = progress;
      if (progress.progress === 100) {
        logComponentState('video download complete');
      }
    }
  });

  // Добавляем слушатель для события transcription-progress
  const unlistenTranscriptionProgress = await listen<TranscriptionProgress>('transcription-progress', (event) => {
    console.log('Transcription progress received directly in VideoPreview:', event.payload);
    transcriptionProgress.value = event.payload;
    isTranscribing.value = true;
    
    // Логируем состояние при первом получении прогресса транскрибации
    if (!transcriptionProgress.value || transcriptionProgress.value.progress === 0) {
      logComponentState('transcription started');
    }
    
    // Если прогресс достиг 100%, отмечаем транскрипцию как завершённую после небольшой задержки
    if (event.payload.progress >= 100) {
      logComponentState('transcription complete');
      setTimeout(() => {
        isTranscribing.value = false;
        
        // Принудительное очищение прогресса через 2 секунды
        setTimeout(() => {
          // Всегда сбрасываем прогресс, но сохраняем результат для степпера
          transcriptionProgress.value = null;
        }, 2000);
      }, 1000);
    }
  });

  // Добавляем слушатель для события translation-progress
  const unlistenTranslationProgress = await listen<TranslationProgress>('translation-progress', (event) => {
    console.log('Translation progress received directly in VideoPreview:', event.payload);
    translationProgress.value = event.payload;
    isTranslating.value = true;
    
    // Если прогресс достиг 100%, отмечаем перевод как завершённый после небольшой задержки
    if (event.payload.progress >= 100) {
      logComponentState('translation complete');
      setTimeout(() => {
        isTranslating.value = false;
        
        // Принудительное очищение прогресса через 2 секунды
        setTimeout(() => {
          // Всегда сбрасываем прогресс, но сохраняем результат для степпера
          translationProgress.value = null;
        }, 2000);
      }, 1000);
    }
  });

  // Добавляем слушатель для события tts-progress с новой оптимизацией
  const unlistenTTSProgress = await listen<TTSProgress>('tts-progress', (event) => {
    // Simply queue the update - processing happens asynchronously
    queueTTSUpdate(event.payload);
  });

  // Добавляем слушатель для события merge-progress
  const unlistenMergeProgress = await listen<MergeProgress>('merge-progress', (event) => {
    console.log('Merge progress received directly in VideoPreview:', event.payload);
    mergeProgress.value = event.payload;
    isMerging.value = true;
    
    // Если прогресс достиг 100%, НЕ отмечаем слияние как завершённое немедленно,
    // ждем события merge-complete
    if (event.payload.progress >= 100) {
      logComponentState('merge at 100%, waiting for merge-complete event');
    }
  });

  // Listen for merge-complete event
  const unlistenMergeComplete = await listen<MergeResult>('merge-complete', (event) => {
    console.log('Merge complete event received in VideoPreview:', event.payload);
    
    // Устанавливаем финальный статус прогресса
    mergeProgress.value = { 
      status: 'Processing complete',
      progress: 100.0
    };
    logComponentState('merge complete event received');
    
    // Отмечаем процесс слияния как завершенный через некоторое время
    setTimeout(() => {
      isMerging.value = false;
      translationComplete.value = true;
      
      // ВАЖНО: не обнуляем mergeProgress, чтобы сохранить отображение
      // прогрессбара на финальном экране
    }, 2000);
    
    // Устанавливаем путь к выходной директории
    outputDirectory.value = event.payload.output_dir;
    
    // Отправляем событие выше
    emit('merge-complete', event.payload.output_dir);
  });

  // Listen for merge-error event
  const unlistenMergeError = await listen<string>('merge-error', (event) => {
    console.error('Merge error received in VideoPreview:', event.payload);
    mergeError.value = event.payload;
    mergeProgress.value = {
      status: 'Failed',
      progress: 0
    };
    isMerging.value = false;
  });

  onUnmounted(() => {
    unlisten?.();
    unlistenTranscriptionProgress?.();
    unlistenTranslationProgress?.();
    unlistenTTSProgress?.();
    unlistenMergeProgress?.();
    unlistenMergeComplete?.();
    unlistenMergeError?.();
    
    // Clean up the URL input listener
    if (urlInputListener) {
      window.removeEventListener('input', urlInputListener);
    }
  });
});

// Add new computed property for steps status
const currentStep = computed(() => {
  // Если есть флаг завершения, но еще показывается процесс слияния,
  // продолжаем показывать шаг merge
  if (translationComplete.value && mergeProgress.value) {
    return 'merge';
  }
  
  // Приоритет отдаем текущим активным процессам
  // Сначала проверяем активные процессы вне зависимости от загрузки
  if (isTranscribing.value) return 'transcription'
  if (isTranslating.value) return 'translation'
  if (isTTSGenerating.value) return 'tts'
  if (isMerging.value) return 'merge'
  
  // Теперь проверяем загрузку
  // Если есть активные индикаторы загрузки, показываем шаг download
  if (audioProgress.value || videoProgress.value) {
    // Проверяем, не завершена ли загрузка (оба прогресса равны 100%)
    const isDownloadComplete = 
      (audioProgress.value?.progress === 100 && videoProgress.value?.progress === 100);
    
    // Если загрузка завершена, скоро прогрессы будут скрыты через setTimeout
    if (isDownloadComplete) {
      console.log('Download complete, progress will be reset soon');
    }
    
    return 'download'
  }
  
  // Если у нас есть какие-либо данные о прогрессе, но нет активного процесса,
  // показываем последний известный шаг
  if (mergeProgress.value) return 'merge';
  if (ttsProgress.value) return 'tts';
  if (translationProgress.value) return 'translation';
  if (transcriptionProgress.value) return 'transcription';
  
  return null
})

// Добавляем watch для проверки завершения загрузки и перехода к следующему шагу
watch([() => audioProgress.value?.progress, () => videoProgress.value?.progress], 
  ([audioProgressValue, videoProgressValue]) => {
    if (audioProgressValue === 100 && videoProgressValue === 100) {
      console.log('Both downloads completed, resetting progress after delay');
      // Добавляем небольшую задержку перед сбросом, чтобы пользователь видел 100%
      setTimeout(() => {
        // Сбрасываем прогресс, если не начался следующий процесс
        if (!isTranscribing.value && !isTranslating.value && 
            !isTTSGenerating.value && !isMerging.value) {
          audioProgress.value = null;
          videoProgress.value = null;
        } else {
          // Если начался следующий процесс, просто скрываем индикаторы загрузки
          // без изменения переменных, что позволит корректно отображать шаги в степпере
          console.log('Next process already started, hiding download progress');
        }
      }, 1000);
    }
  }
);

// Улучшенная обработка прогресса Transcription
watch(() => props.transcriptionProgress, (newProgress) => {
  if (newProgress) {
    trackUIBlocking('Transcription progress update');
    
    // Откладываем обновление состояния для предотвращения блокировки UI
    setTimeout(() => {
      transcriptionProgress.value = newProgress;
      isTranscribing.value = newProgress.progress < 100;
      
      // Обработка завершения
      if (newProgress.progress >= 100) {
        // Задержка перед очисткой
        setTimeout(() => {
          transcriptionProgress.value = null;
        }, 2000);
      }
    }, 0);
  }
}, { immediate: true })

// Оптимизация обработки пропсов TTS
watch(() => props.ttsProgress, (newProgress) => {
  if (newProgress) {
    trackUIBlocking('TTS progress props update');
    
    // Используем дебаунсинг для пропсов так же, как для событий
    // Это поможет избежать блокировки UI при частых обновлениях
    queueTTSUpdate({...newProgress});
  }
}, { immediate: true })

// Оптимизированный обработчик для Translation
watch(() => props.translationProgress, (newProgress) => {
  if (newProgress) {
    trackUIBlocking('Translation progress update');
    
    // Откладываем обновление состояния для предотвращения блокировки UI
    setTimeout(() => {
      translationProgress.value = newProgress;
      isTranslating.value = newProgress.progress < 100;
      
      // Обработка завершения
      if (newProgress.progress >= 100) {
        // Задержка перед очисткой
        setTimeout(() => {
          translationProgress.value = null;
        }, 2000);
      }
    }, 0);
  }
}, { immediate: true })

// Оптимизированный обработчик для Merge
watch(() => props.mergeProgress, (newProgress) => {
  if (newProgress) {
    trackUIBlocking('Merge progress update');
    
    // Откладываем обновление состояния для предотвращения блокировки UI
    setTimeout(() => {
      mergeProgress.value = newProgress;
      isMerging.value = true;
      
      // Обработка завершения
      if (newProgress.progress >= 100) {
        // При финальном шаге не очищаем состояние, 
        // а переходим к завершению
        setTimeout(() => {
          isMerging.value = false;
          
          // Проверяем, не находимся ли мы уже в завершенном состоянии
          if (!translationComplete.value) {
            translationComplete.value = true;
          }
        }, 1000);
      }
    }, 0);
  }
}, { immediate: true })

const steps = computed(() => {
  const stepsList = [
    { id: 'download', label: 'Download' },
    { id: 'transcription', label: 'Transcription' },
    { id: 'translation', label: 'Translation' },
    { id: 'tts', label: 'Voice Generation' },
    { id: 'merge', label: 'Final Processing' }
  ]

  return stepsList.map(step => ({
    ...step,
    status: getStepStatus(step.id)
  }))
})

const getStepStatus = (stepId: string): 'pending' | 'active' | 'completed' => {
  // Если обработка полностью завершена, все шаги помечаем как завершенными
  if (translationComplete.value) return 'completed'
  
  // Если процесс слияния достиг 100%, считаем все предыдущие шаги завершенными
  if (mergeProgress.value?.progress === 100) {
    const stepOrder = ['download', 'transcription', 'translation', 'tts', 'merge']
    const stepIndex = stepOrder.indexOf(stepId)
    if (stepId === 'merge') return 'active'
    return stepIndex < stepOrder.indexOf('merge') ? 'completed' : 'pending'
  }
  
  // Получаем текущий активный шаг
  const currentActiveStep = currentStep.value || ''
  
  // Стандартная логика определения статуса шага
  const stepOrder = ['download', 'transcription', 'translation', 'tts', 'merge']
  const currentStepIndex = stepOrder.indexOf(currentActiveStep)
  const stepIndex = stepOrder.indexOf(stepId)
  
  if (currentStepIndex === -1) return 'pending'
  if (stepIndex === currentStepIndex) return 'active'
  if (stepIndex < currentStepIndex) return 'completed'
  
  // Особый случай: если мы в транскрибации или далее, но оценивается шаг download,
  // помечаем его как завершенный, даже если видео еще загружается
  if (
    stepId === 'download' && 
    ['transcription', 'translation', 'tts', 'merge'].includes(currentActiveStep)
  ) {
    return 'completed'
  }
  
  // Если конкретный прогресс достиг 100%, считаем шаг завершенным
  if (stepId === 'download' && 
      audioProgress.value?.progress === 100 && 
      videoProgress.value?.progress === 100) {
    return 'completed'
  }
  
  if (stepId === 'transcription' && 
      transcriptionProgress.value?.progress === 100) {
    return 'completed'
  }
  
  if (stepId === 'translation' && 
      translationProgress.value?.progress === 100) {
    return 'completed'
  }
  
  if (stepId === 'tts' && 
      ttsProgress.value?.progress === 100) {
    return 'completed'
  }
  
  return 'pending'
}

// Add watcher for currentStep
watch(currentStep, (newVal, oldVal) => {
  console.log('currentStep changed from:', oldVal, 'to:', newVal);
  logComponentState('currentStep changed');
  
  // Особая обработка для случая, когда начался процесс транскрибации,
  // но прогресс загрузки все еще отображается
  if (newVal === 'transcription' && audioProgress.value && videoProgress.value) {
    // Если оба прогресса загрузки показывают 100%, скрываем их
    if (audioProgress.value.progress === 100 && videoProgress.value.progress === 100) {
      console.log('Auto-hiding download progress when transcription started');
      audioProgress.value = null;
      videoProgress.value = null;
    } else {
      // Если процесс скачивания еще не завершен, но транскрибация уже началась,
      // отмечаем компонент скачивания как "завершенный" визуально
      console.log('Download still in progress, but transcription started');
    }
  }
});

// Добавляем реакцию на старт транскрибации
watch(isTranscribing, (newVal) => {
  console.log('isTranscribing changed to:', newVal);
  if (newVal === true) {
    // Если транскрибация началась, проверяем прогресс скачивания
    if (audioProgress.value?.progress === 100 && videoProgress.value?.progress === 100) {
      // Если скачивание завершено, убираем индикаторы
      console.log('Transcription started and download is complete, hiding download progress');
      setTimeout(() => {
        audioProgress.value = null;
        videoProgress.value = null;
      }, 500);
    }
  }
});

// Обновляем остальные вотчеры аналогично
watch(isTranslating, (newVal) => {
  console.log('isTranslating changed to:', newVal);
  if (newVal === true && audioProgress.value && videoProgress.value) {
    // Когда начинается перевод, скрываем индикаторы загрузки, если они все еще видны
    setTimeout(() => {
      audioProgress.value = null;
      videoProgress.value = null;
    }, 500);
  }
});

watch(isTTSGenerating, (newVal) => {
  console.log('isTTSGenerating changed to:', newVal);
  if (newVal === true && audioProgress.value && videoProgress.value) {
    setTimeout(() => {
      audioProgress.value = null;
      videoProgress.value = null;
    }, 500);
  }
});

watch(isMerging, (newVal) => {
  console.log('isMerging changed to:', newVal);
  if (newVal === true && audioProgress.value && videoProgress.value) {
    setTimeout(() => {
      audioProgress.value = null;
      videoProgress.value = null;
    }, 500);
  }
});

// Add a method to reset the component state
function resetState() {
  console.log('Resetting component state');
  internalIsLoading.value = false;
  shouldHideVideoInfo.value = false;
  previousUrl.value = null;
  // Reset initial load flag
  initialLoadDone.value = false;
  // Video is not ready after reset
  emit('video-info-ready-state-change', false);
}

// Add a method to explicitly clear video info
function clearVideoInfo() {
  console.log('Explicitly clearing video info');
  internalIsLoading.value = false;
  previousUrl.value = null;
  shouldHideVideoInfo.value = true;
  // Reset initial load flag
  initialLoadDone.value = false;
  emit('clear-video-info');
  // Video is not ready after clearing
  emit('video-info-ready-state-change', false);
}

// Add a method to force hide video info
function forceHideVideoInfo() {
  console.log('Force hiding video info');
  shouldHideVideoInfo.value = true;
  internalIsLoading.value = false;
  previousUrl.value = null;
  // Reset initial load flag
  initialLoadDone.value = false;
  // Video is not ready after hiding
  emit('video-info-ready-state-change', false);
}

// Expose the methods to the parent component
defineExpose({
  clearVideoInfo,
  resetState,
  forceHideVideoInfo,
  isVideoInfoReady
});
</script>

<template>
  <div class="video-preview">
    <!-- Video info preview -->
    <div v-if="videoInfo && !shouldHideVideoInfo" class="video-info">
      <img :src="videoInfo.thumbnail" :alt="videoInfo.title" class="video-thumbnail" />
      <div class="video-details">
        <h3>{{ videoInfo.title }}</h3>
        <p class="duration">Duration: {{ Math.round(videoInfo.duration / 60) }} minutes</p>
      </div>
    </div>

    <!-- Loading state for video info -->
    <div v-else-if="internalIsLoading" class="video-info loading-state">
      <div class="loading-placeholder thumbnail-placeholder"></div>
      <div class="loading-details">
        <div class="loading-placeholder title-placeholder"></div>
        <div class="loading-placeholder duration-placeholder"></div>
        <div class="loading-text">Loading video info...</div>
      </div>
    </div>

    <!-- Progress Stepper - показываем всегда, когда есть currentStep или завершенный процесс -->
    <ProgressStepper 
      v-if="currentStep || translationComplete"
      :steps="steps"
    />

    <!-- Active Progress Component - показываем всегда, когда есть currentStep или завершенный процесс -->
    <div 
      v-if="currentStep || translationComplete" 
      class="active-progress" 
      :data-step="currentStep"
      :class="{ 'process-complete': translationComplete }"
    >
      <!-- Вместо условия currentStep === 'download' проверяем наличие прогресса загрузки -->
      <div v-if="audioProgress || videoProgress" class="progress-container download-progress" 
           :class="{ 'secondary-progress': currentStep !== 'download', 'complete-progress': translationComplete }">
        <h3 class="progress-title">Download Progress</h3>
        <DownloadProgress v-if="audioProgress" v-bind="audioProgress" />
        <DownloadProgress v-if="videoProgress" v-bind="videoProgress" />
      </div>

      <!-- Transcription progress - показываем, если есть данные, а не по currentStep -->
      <div v-if="transcriptionProgress" 
           class="progress-container" 
           :class="{ 
             'primary-progress': currentStep === 'transcription', 
             'secondary-progress': currentStep !== 'transcription',
             'complete-progress': translationComplete 
           }">
        <TranscriptionProgress 
          :status="transcriptionProgress.status || ''"
          :progress="transcriptionProgress.progress || 0"
        />
      </div>
      
      <!-- Translation progress - показываем, если есть данные -->
      <div v-if="translationProgress" 
           class="progress-container"
           :class="{ 
             'primary-progress': currentStep === 'translation', 
             'secondary-progress': currentStep !== 'translation',
             'complete-progress': translationComplete
           }">
        <TranslationProgress 
          :status="translationProgress.status || ''"
          :progress="translationProgress.progress || 0"
        />
      </div>
      
      <!-- TTS progress - показываем, если есть данные -->
      <div v-if="ttsProgress" 
           class="progress-container"
           :class="{ 
             'primary-progress': currentStep === 'tts', 
             'secondary-progress': currentStep !== 'tts',
             'complete-progress': translationComplete
           }">
        <TTSProgress 
          :status="ttsProgress.status || ''"
          :progress="ttsProgress.progress || 0"
          :current_segment="ttsProgress.current_segment"
          :total_segments="ttsProgress.total_segments"
        />
      </div>
      
      <!-- Merge progress - показываем, если есть данные -->
      <div v-if="mergeProgress" 
           class="progress-container"
           :class="{ 
             'primary-progress': currentStep === 'merge' && !translationComplete, 
             'secondary-progress': currentStep !== 'merge',
             'complete-progress': translationComplete
           }">
        <MergeProgress 
          :status="mergeProgress.status || ''"
          :progress="mergeProgress.progress || 0"
        />
      </div>
    </div>
    
    <!-- Translation Complete - показываем при успешном завершении -->
    <div v-if="translationComplete && outputDirectory" class="translation-complete-container">
      <TranslationComplete :output-dir="outputDirectory" />
    </div>

    <!-- Display for merge errors -->
    <div v-if="mergeError" class="error-message">
      <div class="error-title">Error during final processing:</div>
      <div class="error-details">{{ mergeError }}</div>
      <button @click="mergeError = null" class="error-dismiss">Dismiss</button>
    </div>

    <!-- Empty state -->
    <div v-if="(!videoInfo || shouldHideVideoInfo) && !internalIsLoading && !translationComplete" class="empty-state">
      <p class="description">
        Translate your favorite YouTube videos into any language with AI-powered
        voice translation
      </p>
    </div>
  </div>
</template>

<style scoped>
.description {
  font-size: 0.9rem;
  color: var(--text-secondary);
  max-width: 600px;
  margin: 0 auto;
  font-weight: 400;
}

.video-preview {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
}

.video-info {
  margin-top: 0;
  margin-bottom: 0.25rem;
  display: flex;
  gap: 0.5rem;
  text-align: left;
  background: #f5f5f5;
  padding: 0.5rem;
  border-radius: 8px;
}

.video-thumbnail {
  width: 120px;
  height: 68px;
  object-fit: cover;
  border-radius: 4px;
}

.video-details {
  flex: 1;
  min-width: 0;
}

.video-details h3 {
  margin: 0 0 0.25rem;
  font-size: 0.85rem;
  line-height: 1.3;
}

.duration {
  margin: 0;
  font-size: 0.75rem;
  color: var(--text-secondary);
}

.progress-container {
  background-color: var(--background-secondary, #f5f5f5);
  border-radius: 12px;
  padding: 1rem;
  margin-bottom: 0.5rem;
  transition: all 0.3s ease;
  border: 1px solid rgba(0,0,0,0.05);
}

.primary-progress {
  /* Стиль для активного шага */
  border-color: var(--primary-color, #0077ff);
  border-width: 2px;
  background-color: rgba(0, 119, 255, 0.05);
  transform: scale(1.01);
  z-index: 10;
}

.secondary-progress {
  /* Стиль для неактивного шага */
  opacity: 0.85;
  background-color: var(--background-tertiary, #eaeaea);
  padding: 0.75rem;
  transform: scale(0.98);
}

/* Стиль для завершенных прогрессов */
.complete-progress {
  opacity: 0.95;
  background-color: rgba(76, 217, 100, 0.05);
  border-color: var(--success-color, #4cd964);
  padding: 0.5rem;
  transition: all 0.5s ease;
}

.process-complete .progress-container {
  margin-bottom: 0.25rem;
}

.download-progress.secondary-progress {
  /* Если загрузка не является текущим активным шагом */
  max-height: 80px;
  overflow: hidden;
  transition: all 0.3s ease;
  padding: 0.5rem;
}

.active-progress {
  min-height: 80px;
  transition: all 0.3s ease;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  position: relative;
}

.progress-title {
  font-size: 0.85rem;
  font-weight: 600;
  color: var(--text-primary);
  margin: 0 0 0.5rem;
  letter-spacing: -0.01em;
}

/* Если активный шаг - не download, но download-progress отображается */
.active-progress:not([data-step="download"]) .download-progress {
  border-radius: 8px;
  margin-bottom: 0.75rem;
  opacity: 0.8;
}

.transcription-info {
  margin-bottom: 0.5rem;
  color: var(--text-secondary);
  font-size: 0.75rem;
}

.empty-state {
  text-align: center;
  color: var(--text-secondary);
  padding: 1rem;
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
}

.translation-complete-container {
  margin-top: 0.5rem;
  width: 100%;
}

@media (max-width: 640px) {
  .video-info {
    flex-direction: column;
  }

  .video-thumbnail {
    width: 100%;
    height: auto;
    aspect-ratio: 16/9;
  }
}

/* Add margin-bottom to ProgressStepper when it's shown */
:deep(.stepper) {
  margin-bottom: 0.25rem;
}

.error-message {
  margin-top: 1rem;
  padding: 0.75rem;
  background-color: rgba(255, 59, 48, 0.1);
  border-left: 3px solid var(--error-color, #ff3b30);
  border-radius: 6px;
}

.error-title {
  font-weight: 600;
  color: var(--error-color, #ff3b30);
  margin-bottom: 0.5rem;
}

.error-details {
  font-size: 0.85rem;
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 120px;
  overflow-y: auto;
}

.error-dismiss {
  margin-top: 0.5rem;
  background-color: var(--error-color, #ff3b30);
  color: white;
  font-size: 0.8rem;
  padding: 0.25rem 0.5rem;
}

/* Loading state styles */
.loading-state {
  background: #f5f5f5;
  min-height: 68px;
  animation: pulse 1.5s infinite ease-in-out;
}

.loading-placeholder {
  background: rgba(0, 0, 0, 0.1);
  border-radius: 4px;
}

.thumbnail-placeholder {
  width: 120px;
  height: 68px;
}

.loading-details {
  display: flex;
  flex-direction: column;
  flex: 1;
  justify-content: center;
  gap: 8px;
}

.title-placeholder {
  height: 16px;
  width: 80%;
}

.duration-placeholder {
  height: 12px;
  width: 40%;
}

.loading-text {
  font-size: 0.85rem;
  color: var(--text-secondary);
}

@keyframes pulse {
  0% {
    opacity: 0.7;
  }
  50% {
    opacity: 0.9;
  }
  100% {
    opacity: 0.7;
  }
}
</style> 