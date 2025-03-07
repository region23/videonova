<script setup lang="ts">
import { ref } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { Store as TauriStore } from '@tauri-apps/plugin-store'
import { findLanguageByCode } from '../utils/languages'

interface VideoInfo {
  title: string
  duration: number
  url: string
  thumbnail: string
  description: string
  language?: string
  original_language?: string
}

interface DownloadResult {
  video_path: string
  audio_path: string
}

interface TranscriptionResult {
  vtt_path: string
}

const props = defineProps<{
  disabled?: boolean
  sourceLanguage?: string
}>()

const emit = defineEmits<{
  'video-info': [info: VideoInfo]
  'download-start': []
  'download-complete': [result: DownloadResult]
  'download-error': [error: string]
  'transcription-complete': [result: TranscriptionResult]
  'transcription-error': [error: string]
  'language-detected': [code: string]
  'start-download': [url: string, path: string]
}>()

const youtubeUrl = ref('')
const selectedPath = ref('')
const isLoading = ref(false)
const isTranscribing = ref(false)
const showTranscription = ref(false)
const downloadResult = ref<DownloadResult | null>(null)
const audioPath = ref<string | null>(null)

// Listen for audio-ready event and automatically start transcription
listen('audio-ready', (event) => {
  const path = event.payload as string
  audioPath.value = path
  
  // Показываем раздел транскрибации
  isTranscribing.value = true
  showTranscription.value = true
  
  // Автоматически запускаем транскрибацию
  startTranscriptionWithPath(path)
})

const selectFolder = async () => {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
    })
    if (selected) {
      selectedPath.value = selected as string
      // Отправляем информацию о пути и URL сразу после выбора папки
      if (youtubeUrl.value) {
        emit('start-download', youtubeUrl.value, selectedPath.value)
      }
    }
  } catch (e) {
    console.error('Failed to select folder:', e)
  }
}

const getVideoInfo = async () => {
  if (!youtubeUrl.value) return

  try {
    isLoading.value = true
    const info = await invoke<VideoInfo>('get_video_info', {
      url: youtubeUrl.value
    })
    
    // Если у видео определен язык, отправляем его в родительский компонент
    if (info.language || info.original_language) {
      const detectedLanguageCode = normalizeLanguageCode(info.language || info.original_language || '')
      const detectedLanguage = findLanguageByCode(detectedLanguageCode)
      
      if (detectedLanguage) {
        emit('language-detected', detectedLanguage.code)
      }
    }
    
    emit('video-info', info)
    
    // Если папка уже выбрана, отправляем информацию о пути и URL
    if (selectedPath.value) {
      emit('start-download', youtubeUrl.value, selectedPath.value)
    }
  } catch (e) {
    console.error('Failed to get video info:', e)
    alert('Failed to get video information. Please check the URL and try again.')
  } finally {
    isLoading.value = false
  }
}

const startDownload = async () => {
  if (!selectedPath.value || !youtubeUrl.value) return

  try {
    isLoading.value = true
    emit('download-start')
    emit('start-download', youtubeUrl.value, selectedPath.value)
  } catch (e) {
    console.error('Failed to download:', e)
    emit('download-error', e instanceof Error ? e.message : 'Failed to download. Please try again.')
  } finally {
    isLoading.value = false
  }
}

// Обновляем функцию запуска транскрибации
const startTranscriptionWithPath = async (path: string) => {
  if (!path) return

  try {
    // Get OpenAI API key from store
    const store = await TauriStore.load('.settings.dat')
    const apiKey = await store.get('openai-api-key') as string
    
    if (!apiKey) {
      throw new Error('OpenAI API key not found. Please add it in the settings.')
    }
    
    // Используем setTimeout для обеспечения неблокирующего вызова
    setTimeout(async () => {
      try {
        const result = await invoke<TranscriptionResult>('transcribe_audio', {
          audioPath: path,
          outputPath: selectedPath.value,
          apiKey: apiKey,
          language: props.sourceLanguage || ''
        })
        
        emit('transcription-complete', result)
        
        // Скрываем индикатор через 3 секунды
        setTimeout(() => {
          isTranscribing.value = false
        }, 3000)
      } catch (e) {
        console.error('Failed to transcribe:', e)
        emit('transcription-error', e instanceof Error ? e.message : 'Failed to transcribe. Please try again.')
        
        // Скрываем индикатор через 5 секунд в случае ошибки
        setTimeout(() => {
          isTranscribing.value = false
        }, 5000)
      }
    }, 100) // Небольшая задержка для завершения обработки аудио
  } catch (e) {
    console.error('Failed to initialize transcription:', e)
    emit('transcription-error', e instanceof Error ? e.message : 'Failed to initialize transcription. Please try again.')
    
    // Скрываем индикатор через 5 секунд в случае ошибки
    setTimeout(() => {
      isTranscribing.value = false
    }, 5000)
  }
}

// Удаляем LANGUAGE_CODES и оставляем только необходимый код
const normalizeLanguageCode = (code: string): string => {
  // Убираем региональный код (например, 'en-US' -> 'en')
  return code.split('-')[0].toLowerCase()
}
</script>

<template>
  <div class="youtube-input">
    <div class="main-column">
      <h2>Enter Video URL</h2>
      
      <form @submit.prevent="startDownload" class="input-form">
        <div class="url-input-group">
          <input
            v-model="youtubeUrl"
            type="url"
            placeholder="https://www.youtube.com/watch?v=..."
            required
            :disabled="disabled || isLoading"
            @input="getVideoInfo"
          />
          <button 
            type="button" 
            class="folder-button" 
            @click="selectFolder"
            :disabled="disabled || isLoading"
            :title="selectedPath || 'Select folder where the video will be downloaded'"
          >
            <span class="button-content">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" class="icon">
                <path d="M3 7v10c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V9c0-1.1-.9-2-2-2h-6l-2-2H5c-1.1 0-2 .9-2 2z" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
              <span class="folder-path" v-if="selectedPath">
                ...{{ selectedPath.split('/').slice(-1)[0] }}
              </span>
              <span class="folder-path" v-else>
                Select Folder
              </span>
            </span>
          </button>
        </div>
      </form>
    </div>
  </div>
</template>

<style scoped>
.youtube-input {
  width: 100%;
  max-width: 600px;
  text-align: center;
}

.main-column {
  width: 100%;
}

h2 {
  font-size: 1.25rem;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 1.5rem;
  letter-spacing: -0.01em;
}

h3 {
  font-size: 1.1rem;
  font-weight: 600;
  color: var(--text-primary);
  margin: 0 0 1rem;
  letter-spacing: -0.01em;
}

.input-form {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  width: 100%;
}


.url-input-group {
  display: flex;
  gap: 0.75rem;
  width: 100%;
}

input[type="url"] {
  flex: 1;
  padding: 10px 16px;
  border-radius: 8px;
  font-size: 0.9rem;
}

.folder-button,
.process-button,
.transcribe-button {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 40px;
  padding: 0 16px;
  font-weight: 500;
  transition: all 0.2s ease;
  border-radius: 8px;
}

.folder-button {
  background-color: var(--background-secondary);
  color: var(--text-primary);
  border: 1px solid var(--border-color);
  min-width: 150px;
}

.process-button,
.transcribe-button {
  background-color: var(--accent-primary);
  color: white;
}

.button-content {
  display: flex;
  align-items: center;
  gap: 8px;
}

.folder-path {
  max-width: 120px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

/* Медиа-запрос для адаптации под мобильные устройства */
@media (max-width: 900px) {
  .youtube-input {
    width: 100%;
  }
}
</style> 