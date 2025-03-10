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
  step_progress?: number  // Add step_progress field from backend
  readonly value?: any    // Add the readonly value field to fix linter error
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

// Добавляем состояние для отслеживания завершения загрузки
const isDownloadComplete = ref(false);

// Добавляем состояния для отслеживания завершения каждого шага
const downloadStepComplete = ref(false)
const transcriptionStepComplete = ref(false)
const translationStepComplete = ref(false)
const ttsStepComplete = ref(false)
const mergeStepComplete = ref(false)

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

// Добавляем watch для проверки завершения загрузки и перехода к следующему шагу
watch([() => audioProgress.value?.progress, () => videoProgress.value?.progress], 
  ([audioProgressValue, videoProgressValue]) => {
    // Используем requestAnimationFrame для обновления UI
    requestAnimationFrame(() => {
      if (audioProgressValue === 100 && videoProgressValue === 100) {
        console.log('Both downloads completed, preparing for next step');
        
        // Если следующий процесс уже начался, просто скрываем индикаторы
        if (isTranscribing.value || isTranslating.value || 
            isTTSGenerating.value || isMerging.value) {
          console.log('Next process already started, hiding download progress');
          audioProgress.value = null;
          videoProgress.value = null;
          return;
        }
        
        // Если следующий процесс не начался, ждем немного и скрываем
        setTimeout(() => {
          if (!isTranscribing.value && !isTranslating.value && 
              !isTTSGenerating.value && !isMerging.value) {
            audioProgress.value = null;
            videoProgress.value = null;
          }
        }, 1000);
      }
    });
  },
  { immediate: true } // Добавляем immediate для немедленной проверки при монтировании
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

function queueTTSUpdate(progress: any) {
  // Create a new object instead of modifying the original
  const processedProgress: TTSProgress = { ...progress };
  
  // Map backend's step_progress to progress if it exists
  if (progress.step_progress !== undefined && (progress.progress === undefined || progress.progress === null)) {
    processedProgress.progress = progress.step_progress;
  }
  
  // Add to queue, replacing any previous item if same segment
  const existingIndex = ttsUpdateQueue.findIndex(p => 
    p.current_segment === processedProgress.current_segment
  );
  
  if (existingIndex >= 0) {
    ttsUpdateQueue[existingIndex] = processedProgress;
  } else {
    ttsUpdateQueue.push(processedProgress);
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
  
  const nextProgress = ttsUpdateQueue.shift();
  if (nextProgress) {
    // Force update on the next animation frame to avoid UI blocking
    requestAnimationFrame(() => {
      console.log('Processing TTS update:', 
        `Progress: ${nextProgress.progress}%, Status: ${nextProgress.status || 'N/A'}`);
        
      // Always update the progress value for TTS to ensure UI reflects current state
      if (ttsProgress.value === null) {
        ttsProgress.value = { ...nextProgress };
      } else {
        ttsProgress.value.progress = nextProgress.progress;
        ttsProgress.value.status = nextProgress.status;
        ttsProgress.value.current_segment = nextProgress.current_segment;
        ttsProgress.value.total_segments = nextProgress.total_segments;
      }
      
      // Check for status changes
      if (nextProgress.progress === 0) {
        console.log('TTS process starting');
        isTTSGenerating.value = true;
        ttsStepComplete.value = false; // Сбрасываем флаг завершения при старте
      } else if (nextProgress.progress === 100) {
        console.log('TTS process complete');
        // Проверяем успешность генерации
        if (nextProgress.status && nextProgress.status.toLowerCase().includes('error')) {
          console.error('TTS generation failed:', nextProgress.status);
          ttsStepComplete.value = false;
        } else {
          isTTSGenerating.value = false;
          ttsStepComplete.value = true;
          // Если файл уже существовал, отмечаем следующий шаг как завершенный
          if (!isMerging.value && props.mergeProgress?.progress === 100) {
            mergeStepComplete.value = true;
          }
        }
        // Очищаем прогресс с задержкой
        setTimeout(() => {
          ttsProgress.value = null;
        }, 1000);
      }
      
      // Process next item in queue after small delay
      setTimeout(processTTSQueue, 50);
    });
  }
}

// Улучшаем функцию отслеживания блокировки UI
function trackUIBlocking(operation: string) {
  const start = performance.now();
  console.log(`[Performance] Start ${operation}`);
  
  // Проверяем через 16ms (примерно один кадр)
  setTimeout(() => {
    const end = performance.now();
    const elapsed = end - start;
    
    if (elapsed > 16) {
      console.warn(`[Performance] UI blocked during ${operation}. Elapsed: ${elapsed.toFixed(2)}ms`);
    } else {
      console.log(`[Performance] ${operation} completed in ${elapsed.toFixed(2)}ms`);
    }
  }, 16);
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
    console.log(`Download progress received: ${progress.component} - ${progress.progress}%`);
    
    requestAnimationFrame(() => {
      if (progress.component === 'audio') {
        if (progress.progress === 100) {
          // Если аудио загружено полностью, начинаем отсчет для скрытия
          setTimeout(() => {
            audioProgress.value = null;
          }, 1000);
        } else {
          audioProgress.value = progress;
        }
      } else if (progress.component === 'video') {
        if (progress.progress === 100) {
          // Если видео загружено полностью, начинаем отсчет для скрытия
          setTimeout(() => {
            videoProgress.value = null;
          }, 1000);
        } else {
          videoProgress.value = progress;
        }
      }

      // Проверяем завершение обоих загрузок для обновления статуса шага
      if (progress.progress === 100) {
        if ((progress.component === 'audio' && videoProgress.value?.progress === 100) ||
            (progress.component === 'video' && audioProgress.value?.progress === 100)) {
          console.log('Both downloads completed');
          isDownloadComplete.value = true;
          downloadStepComplete.value = true;
          // Очищаем оба прогресса после небольшой задержки
          setTimeout(() => {
            audioProgress.value = null;
            videoProgress.value = null;
          }, 1000);
        }
      }
    });
  });

  // Обновляем слушатель download-complete
  const unlistenDownloadComplete = await listen('download-complete', () => {
    console.log('Download complete event received');
    // Устанавливаем флаги завершения загрузки
    isDownloadComplete.value = true;
    downloadStepComplete.value = true;

    // Проверяем наличие файлов для следующих шагов
    // и отмечаем их как завершенные если файлы существуют
    if (props.transcriptionProgress?.progress === 100) {
      transcriptionStepComplete.value = true;
    }
    if (props.translationProgress?.progress === 100) {
      translationStepComplete.value = true;
    }
    if (props.ttsProgress?.progress === 100) {
      ttsStepComplete.value = true;
    }
    if (props.mergeProgress?.progress === 100) {
      mergeStepComplete.value = true;
    }

    // Очищаем прогресс-бары с небольшой задержкой
    setTimeout(() => {
      audioProgress.value = null;
      videoProgress.value = null;
    }, 1000);
  });

  // Добавляем слушатель для события download-error
  const unlistenDownloadError = await listen<string>('download-error', (event) => {
    console.error('Download error received:', event.payload);
    // Сбрасываем прогресс в случае ошибки
    audioProgress.value = null;
    videoProgress.value = null;
  });

  // Обновляем слушатель transcription-progress
  const unlistenTranscriptionProgress = await listen<TranscriptionProgress>('transcription-progress', (event) => {
    console.log('Transcription progress received directly in VideoPreview:', event.payload);
    transcriptionProgress.value = event.payload;
    isTranscribing.value = true;
    
    if (event.payload.progress >= 100) {
      logComponentState('transcription complete');
      isTranscribing.value = false;
      transcriptionStepComplete.value = true;
      // Если файл уже существовал, отмечаем следующие шаги как завершенные
      if (!isTranslating.value && props.translationProgress?.progress === 100) {
        translationStepComplete.value = true;
      }
      if (!isTTSGenerating.value && props.ttsProgress?.progress === 100) {
        ttsStepComplete.value = true;
      }
      if (!isMerging.value && props.mergeProgress?.progress === 100) {
        mergeStepComplete.value = true;
      }
      // Очищаем прогресс с задержкой
      setTimeout(() => {
        transcriptionProgress.value = null;
      }, 1000);
    }
  });

  // Обновляем слушатель translation-progress
  const unlistenTranslationProgress = await listen<TranslationProgress>('translation-progress', (event) => {
    console.log('Translation progress received directly in VideoPreview:', event.payload);
    translationProgress.value = event.payload;
    isTranslating.value = true;
    
    if (event.payload.progress >= 100) {
      logComponentState('translation complete');
      isTranslating.value = false;
      translationStepComplete.value = true;
      // Если файл уже существовал, отмечаем следующие шаги как завершенные
      if (!isTTSGenerating.value && props.ttsProgress?.progress === 100) {
        ttsStepComplete.value = true;
      }
      if (!isMerging.value && props.mergeProgress?.progress === 100) {
        mergeStepComplete.value = true;
      }
      // Очищаем прогресс с задержкой
      setTimeout(() => {
        translationProgress.value = null;
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
    
    // Set final progress status
    mergeProgress.value = { 
      status: 'Processing complete',
      progress: 100.0
    };
    
    // Important: Mark merge as complete and update step status
    isMerging.value = false;
    mergeStepComplete.value = true;
    translationComplete.value = true;
    
    // Set output directory
    outputDirectory.value = event.payload.output_dir;
    
    // Only emit this event once
    emit('merge-complete', event.payload.output_dir);
  });

  // Обновляем обработчик ошибки merge
  const unlistenMergeError = await listen<string>('merge-error', (event) => {
    console.error('Merge error received in VideoPreview:', event.payload);
    const errorMessage = event.payload;
    
    // Формируем понятное пользователю сообщение об ошибке
    let userFriendlyError = 'Failed to create final video: ';
    
    if (errorMessage.includes('No audio stream found')) {
      userFriendlyError += 'Audio file is missing or corrupted. This might happen if voice generation was not completed properly. Please try processing the video again.';
      // Сбрасываем флаг завершения TTS, чтобы пользователь мог повторить попытку
      ttsStepComplete.value = false;
      isTTSGenerating.value = false;
    } else {
      userFriendlyError += errorMessage;
    }
    
    mergeError.value = userFriendlyError;
    mergeProgress.value = {
      status: 'Failed',
      progress: 0
    };
    isMerging.value = false;
  });

  // Add this listener for merge start event if not already present
  const unlistenMergeStart = await listen('merge-start', () => {
    console.log('Merge start event received');
    isMerging.value = true;
    currentStep.value = 'merge'; // Explicitly set the current step
    
    // Show the merge UI
    ttsStepComplete.value = true;
    trackUIBlocking('Merge start received');
  });

  onUnmounted(() => {
    unlisten?.();
    unlistenDownloadComplete?.();
    unlistenDownloadError?.();
    unlistenTranscriptionProgress?.();
    unlistenTranslationProgress?.();
    unlistenTTSProgress?.();
    unlistenMergeProgress?.();
    unlistenMergeComplete?.();
    unlistenMergeError?.();
    unlistenMergeStart?.();
    
    // Clean up the URL input listener
    if (urlInputListener) {
      window.removeEventListener('input', urlInputListener);
    }
  });
});

// Add new computed property for steps status
const currentStep = computed(() => {
  // Если процесс только начался и есть информация о видео, показываем download
  if (internalIsLoading.value && props.videoInfo && !isTranscribing.value && !isTranslating.value && 
      !isTTSGenerating.value && !isMerging.value) {
    return 'download'
  }

  // Если загрузка завершена и начался следующий процесс, переходим к нему
  if (isDownloadComplete.value) {
    if (isTranscribing.value) return 'transcription'
    if (isTranslating.value) return 'translation'
    if (isTTSGenerating.value) return 'tts'
    if (isMerging.value) return 'merge'
  }

  // Показываем download ТОЛЬКО если есть активные индикаторы загрузки
  if (audioProgress.value || videoProgress.value) {
    return 'download'
  }
  
  // Если нет активных процессов, но есть какой-то прогресс,
  // показываем соответствующий шаг
  if (transcriptionProgress.value) return 'transcription'
  if (translationProgress.value) return 'translation'
  if (ttsProgress.value) return 'tts'
  if (mergeProgress.value) return 'merge'
  
  return null
})

// Добавляем отслеживание в watch
watch(() => props.transcriptionProgress, (newProgress) => {
  if (newProgress) {
    trackUIBlocking('Transcription progress update');
    requestAnimationFrame(() => {
      transcriptionProgress.value = newProgress;
      isTranscribing.value = newProgress.progress < 100;
      
      if (newProgress.progress >= 100) {
        logComponentState('transcription complete');
        isTranscribing.value = false;
        transcriptionStepComplete.value = true;
        // Если файл уже существовал, отмечаем следующие шаги как завершенные
        if (!isTranslating.value && props.translationProgress?.progress === 100) {
          translationStepComplete.value = true;
        }
        if (!isTTSGenerating.value && props.ttsProgress?.progress === 100) {
          ttsStepComplete.value = true;
        }
        if (!isMerging.value && props.mergeProgress?.progress === 100) {
          mergeStepComplete.value = true;
        }
        // Очищаем прогресс с задержкой
        setTimeout(() => {
          transcriptionProgress.value = null;
        }, 1000);
      }
    });
  }
}, { immediate: true })

// Оптимизация обработки пропсов Translation
watch(() => props.translationProgress, (newProgress) => {
  if (newProgress) {
    trackUIBlocking('Translation progress update');
    
    setTimeout(() => {
      translationProgress.value = newProgress;
      isTranslating.value = newProgress.progress < 100;
      
      if (newProgress.progress >= 100) {
        logComponentState('translation complete');
        isTranslating.value = false;
        translationStepComplete.value = true;
        // Если файл уже существовал, отмечаем следующие шаги как завершенные
        if (!isTTSGenerating.value && props.ttsProgress?.progress === 100) {
          ttsStepComplete.value = true;
        }
        if (!isMerging.value && props.mergeProgress?.progress === 100) {
          mergeStepComplete.value = true;
        }
        // Очищаем прогресс с задержкой
        setTimeout(() => {
          translationProgress.value = null;
        }, 1000);
      }
    }, 0);
  }
}, { immediate: true })

// Оптимизированный обработчик для Merge
watch(() => props.mergeProgress, (newProgress) => {
  if (newProgress && !translationComplete.value) {
    trackUIBlocking('Merge progress update');
    mergeProgress.value = newProgress;
    isMerging.value = newProgress.progress < 100;
    
    // Don't set translationComplete here - wait for the merge-complete event
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

  // Определяем статус каждого шага независимо
  switch (stepId) {
    case 'download':
      // Шаг Download завершен если:
      // 1. Установлен флаг downloadStepComplete
      // 2. ИЛИ оба прогресса достигли 100%
      if (downloadStepComplete.value) return 'completed'
      if (audioProgress.value?.progress === 100 && videoProgress.value?.progress === 100) return 'completed'
      if (audioProgress.value || videoProgress.value) return 'active'
      break

    case 'transcription':
      if (transcriptionStepComplete.value) return 'completed'
      if (isTranscribing.value || (transcriptionProgress.value && transcriptionProgress.value.progress > 0)) return 'active'
      if (!downloadStepComplete.value) return 'pending'
      break

    case 'translation':
      if (translationStepComplete.value) return 'completed'
      if (isTranslating.value || (translationProgress.value && translationProgress.value.progress > 0)) return 'active'
      if (!transcriptionStepComplete.value) return 'pending'
      break

    case 'tts':
      if (ttsStepComplete.value) return 'completed'
      if (isTTSGenerating.value || (ttsProgress.value && ttsProgress.value.progress > 0)) return 'active'
      if (!translationStepComplete.value) return 'pending'
      break

    case 'merge':
      if (mergeStepComplete.value) return 'completed'
      if (isMerging.value || (mergeProgress.value && mergeProgress.value.progress > 0)) return 'active'
      if (!ttsStepComplete.value) return 'pending'
      break
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
  isDownloadComplete.value = false;
  initialLoadDone.value = false;
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

    <!-- Progress Stepper - показываем только когда есть активный шаг или процесс завершен -->
    <ProgressStepper 
      v-if="currentStep || translationComplete"
      :steps="steps"
    />

    <!-- Active Progress Component - показываем всегда, когда есть currentStep, завершенный процесс или идет обработка -->
    <div 
      v-if="currentStep || translationComplete || isLoading" 
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
      <div class="quick-start-guide">
        <h3>Quick Start Guide</h3>
        <div class="steps">
          <div class="step">
            <div class="step-number">1</div>
            <p>Paste a YouTube video URL in the input field above</p>
          </div>
          <div class="step">
            <div class="step-number">2</div>
            <p>Select folder for output video</p>
          </div>
          <div class="step">
            <div class="step-number">3</div>
            <p>Select your desired target language for translation</p>
          </div>
          <div class="step">
            <div class="step-number">4</div>
            <p>Click "Translate" and wait while we process your video</p>
          </div>
          <div class="step">
            <div class="step-number">5</div>
            <p>Enjoy translated video saved locally!</p>
          </div>
        </div>
      </div>
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

.quick-start-guide {
  margin-top: 0.5rem;
  padding: 1.25rem;
  background: rgba(255, 255, 255, 0.02);
  border-radius: 8px;
  text-align: center;
  max-width: 500px;
  margin-left: auto;
  margin-right: auto;
  border: 1px solid rgba(255, 255, 255, 0.1);
}

.quick-start-guide h3 {
  font-size: 1rem;
  margin-bottom: 1.25rem;
  color: #545353;
  text-align: center;
  font-weight: 500;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.steps {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  align-items: center;
  max-width: 400px;
  margin: 0 auto;
}

.step {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  width: 100%;
}

.step-number {
  width: 22px;
  height: 22px;
  background: rgba(59, 130, 246, 0.5);
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 500;
  color: rgba(255, 255, 255, 0.9);
  flex-shrink: 0;
  font-size: 0.85rem;
}

.step p {
  margin: 0;
  color: #545353;
  font-size: 0.9rem;
  line-height: 1.4;
  text-align: left;
  font-weight: 400;
}
</style>
