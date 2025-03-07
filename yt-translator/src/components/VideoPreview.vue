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
}>()

// Define props
const props = defineProps<{
  videoInfo?: VideoInfo | null
  transcriptionProgress?: any
  translationProgress?: any
  ttsProgress?: any
  mergeProgress?: any
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

// Create a cleanup function for the event listener
let unlisten: (() => void) | null = null;

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
        audioProgress.value = null;
        videoProgress.value = null;
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

  unlisten = await listen<DownloadProgress>('download-progress', (event) => {
    const progress = event.payload;
    if (progress.component === 'audio') {
      audioProgress.value = progress;
    } else if (progress.component === 'video') {
      videoProgress.value = progress;
    }
  });

  // Добавляем слушатель для события transcription-progress
  const unlistenTranscriptionProgress = await listen<TranscriptionProgress>('transcription-progress', (event) => {
    console.log('Transcription progress received directly in VideoPreview:', event.payload);
    transcriptionProgress.value = event.payload;
    isTranscribing.value = true;
    
    // Если прогресс достиг 100%, отмечаем транскрипцию как завершённую после небольшой задержки
    if (event.payload.progress >= 100) {
      setTimeout(() => {
        isTranscribing.value = false;
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
      setTimeout(() => {
        isTranslating.value = false;
      }, 1000);
    }
  });

  // Добавляем слушатель для события tts-progress
  const unlistenTTSProgress = await listen<TTSProgress>('tts-progress', (event) => {
    console.log('TTS progress received directly in VideoPreview:', event.payload);
    ttsProgress.value = event.payload;
    isTTSGenerating.value = true;
    
    // Если прогресс достиг 100%, отмечаем TTS как завершённый после небольшой задержки
    if (event.payload.progress >= 100) {
      setTimeout(() => {
        isTTSGenerating.value = false;
      }, 1000);
    }
  });

  // Добавляем слушатель для события merge-progress
  const unlistenMergeProgress = await listen<MergeProgress>('merge-progress', (event) => {
    console.log('Merge progress received directly in VideoPreview:', event.payload);
    mergeProgress.value = event.payload;
    isMerging.value = true;
    
    // Если прогресс достиг 100%, отмечаем слияние как завершённое после небольшой задержки
    if (event.payload.progress >= 100) {
      setTimeout(() => {
        isMerging.value = false;
        translationComplete.value = true;
      }, 1000);
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
    
    // Отмечаем процесс слияния как завершенный
    setTimeout(() => {
      isMerging.value = false;
      translationComplete.value = true;
    }, 1000);
    
    // Устанавливаем путь к выходной директории
    outputDirectory.value = event.payload.output_dir;
    
    // Отправляем событие выше
    emit('merge-complete', event.payload.output_dir);
  });

  onUnmounted(() => {
    unlisten?.();
    unlistenTranscriptionProgress?.();
    unlistenTranslationProgress?.();
    unlistenTTSProgress?.();
    unlistenMergeProgress?.();
    unlistenMergeComplete?.();
  });
});

// Add new computed property for steps status
const currentStep = computed(() => {
  // Проверяем, завершена ли загрузка (оба прогресса равны 100%)
  const isDownloadComplete = 
    (!audioProgress.value && !videoProgress.value) || // если прогресс не инициализирован
    (audioProgress.value?.progress === 100 && videoProgress.value?.progress === 100); // или оба прогресса 100%
  
  console.log('isDownloadComplete:', isDownloadComplete, 'audioProgress:', audioProgress.value?.progress, 'videoProgress:', videoProgress.value?.progress);
  
  // Если загрузка не завершена, показываем шаг download
  if (!isDownloadComplete && (audioProgress.value || videoProgress.value)) return 'download'
  
  // Далее проверяем активные статусы по порядку
  if (isTranscribing.value) return 'transcription'
  if (isTranslating.value) return 'translation'
  if (isTTSGenerating.value) return 'tts'
  if (isMerging.value) return 'merge'
  
  // Проверяем, есть ли незавершенная загрузка (случай, когда данные прогресса есть, но < 100%)
  if (audioProgress.value || videoProgress.value) return 'download'
  
  return null
})

// Add watcher for currentStep
watch(currentStep, (newVal) => {
  console.log('currentStep changed to:', newVal);
});

// Add watchers for progress updates
watch(() => props.transcriptionProgress, (newProgress) => {
  if (newProgress) {
    transcriptionProgress.value = newProgress
    isTranscribing.value = true
    if (newProgress.progress >= 100) {
      setTimeout(() => {
        isTranscribing.value = false
      }, 1000)
    }
  }
}, { immediate: true })

watch(() => props.translationProgress, (newProgress) => {
  if (newProgress) {
    translationProgress.value = newProgress
    isTranslating.value = true
    if (newProgress.progress >= 100) {
      setTimeout(() => {
        isTranslating.value = false
      }, 1000)
    }
  }
}, { immediate: true })

watch(() => props.ttsProgress, (newProgress) => {
  if (newProgress) {
    ttsProgress.value = newProgress
    isTTSGenerating.value = true
    if (newProgress.progress >= 100) {
      setTimeout(() => {
        isTTSGenerating.value = false
      }, 1000)
    }
  }
}, { immediate: true })

watch(() => props.mergeProgress, (newProgress) => {
  if (newProgress) {
    mergeProgress.value = newProgress
    isMerging.value = true
    if (newProgress.progress >= 100) {
      setTimeout(() => {
        isMerging.value = false
        translationComplete.value = true
      }, 1000)
    }
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
  // Если обработка полностью завершена, все шаги помечаем как завершенные
  if (translationComplete.value) return 'completed'
  
  // Если процесс слияния достиг 100%, считаем все предыдущие шаги завершенными
  if (mergeProgress.value?.progress === 100) {
    const stepOrder = ['download', 'transcription', 'translation', 'tts', 'merge']
    const stepIndex = stepOrder.indexOf(stepId)
    if (stepId === 'merge') return 'active'
    return stepIndex < stepOrder.indexOf('merge') ? 'completed' : 'pending'
  }
  
  // Стандартная логика определения статуса шага
  const stepOrder = ['download', 'transcription', 'translation', 'tts', 'merge']
  const currentStepIndex = stepOrder.indexOf(currentStep.value || '')
  const stepIndex = stepOrder.indexOf(stepId)
  
  if (currentStepIndex === -1) return 'pending'
  if (stepIndex === currentStepIndex) return 'active'
  if (stepIndex < currentStepIndex) return 'completed'
  return 'pending'
}
</script>

<template>
  <div class="video-preview">
    <!-- Video info preview -->
    <div v-if="videoInfo" class="video-info">
      <img :src="videoInfo.thumbnail" :alt="videoInfo.title" class="video-thumbnail" />
      <div class="video-details">
        <h3>{{ videoInfo.title }}</h3>
        <p class="duration">Duration: {{ Math.round(videoInfo.duration / 60) }} minutes</p>
      </div>
    </div>

    <!-- Progress Stepper -->
    <ProgressStepper 
      v-if="currentStep"
      :steps="steps"
    />

    <!-- Active Progress Component -->
    <div v-if="currentStep" class="active-progress">
      <!-- Download progress -->
      <div v-if="currentStep === 'download'" class="progress-container">
        <h3 class="progress-title">Download Progress</h3>
        <DownloadProgress v-if="audioProgress" v-bind="audioProgress" />
        <DownloadProgress v-if="videoProgress" v-bind="videoProgress" />
      </div>

      <!-- Transcription progress -->
      <div v-if="currentStep === 'transcription'" class="progress-container">
        <TranscriptionProgress 
          :status="transcriptionProgress?.status || ''"
          :progress="transcriptionProgress?.progress || 0"
        />
      </div>
      
      <!-- Translation progress -->
      <div v-if="currentStep === 'translation'" class="progress-container">
        <TranslationProgress 
          :status="translationProgress?.status || ''"
          :progress="translationProgress?.progress || 0"
        />
      </div>
      
      <!-- TTS progress -->
      <div v-if="currentStep === 'tts'" class="progress-container">
        <TTSProgress 
          :status="ttsProgress?.status || ''"
          :progress="ttsProgress?.progress || 0"
          :current_segment="ttsProgress?.current_segment"
          :total_segments="ttsProgress?.total_segments"
        />
      </div>
      
      <!-- Merge progress -->
      <div v-if="currentStep === 'merge'" class="progress-container">
        <MergeProgress 
          :status="mergeProgress?.status || ''"
          :progress="mergeProgress?.progress || 0"
        />
      </div>
    </div>
    
    <!-- Translation Complete -->
    <div v-if="translationComplete && outputDirectory" class="translation-complete-container">
      <TranslationComplete :output-dir="outputDirectory" />
    </div>

    <!-- Empty state -->
    <div v-if="!videoInfo && !translationComplete" class="empty-state">
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
  padding: 0.5rem;
  margin-bottom: 0.25rem;
  transition: all 0.3s ease;
}

.active-progress {
  min-height: 80px;
  transition: all 0.3s ease;
}

.progress-title {
  font-size: 0.85rem;
  font-weight: 600;
  color: var(--text-primary);
  margin: 0 0 0.5rem;
  letter-spacing: -0.01em;
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
</style> 