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

interface TranslationResult {
  translated_vtt_path: string
  base_filename: string
}

interface TTSResult {
  audio_path: string
}

const props = defineProps<{
  disabled?: boolean
  sourceLanguage?: string
  targetLanguage?: string
  targetLanguageCode?: string
}>()

const emit = defineEmits<{
  'video-info': [info: VideoInfo]
  'download-start': []
  'download-complete': [result: DownloadResult]
  'download-error': [error: string]
  'transcription-complete': [result: TranscriptionResult]
  'transcription-error': [error: string]
  'translation-complete': [result: TranslationResult]
  'translation-error': [error: string]
  'language-detected': [code: string]
  'start-download': [url: string, path: string]
}>()

const youtubeUrl = ref('')
const selectedPath = ref('')
const isLoading = ref(false)
const isTranscribing = ref(false)
const isTranslating = ref(false)
const isGeneratingTTS = ref(false)
const showTranscription = ref(false)
const downloadResult = ref<DownloadResult | null>(null)
const audioPath = ref<string | null>(null)
const vttPath = ref<string | null>(null)
const translatedVttPath = ref<string | null>(null)
const ttsAudioPath = ref<string | null>(null)

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
        vttPath.value = result.vtt_path
        
        // Auto-start translation if target language is set
        if (props.targetLanguage && props.targetLanguageCode) {
          startTranslation(result.vtt_path);
        } else {
          // Скрываем индикатор через 3 секунды
          setTimeout(() => {
            isTranscribing.value = false
          }, 3000)
        }
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

// Add a new function to handle translation
const startTranslation = async (vttPath: string) => {
  if (!vttPath || !props.targetLanguage || !props.targetLanguageCode) return
  
  try {
    isTranslating.value = true
    
    // Get OpenAI API key from store
    const store = await TauriStore.load('.settings.dat')
    const apiKey = await store.get('openai-api-key') as string
    
    if (!apiKey) {
      throw new Error('OpenAI API key not found. Please add it in the settings.')
    }
    
    // Use setTimeout for non-blocking execution
    setTimeout(async () => {
      try {
        const result = await invoke<TranslationResult>('translate_vtt', {
          vttPath: vttPath,
          outputPath: selectedPath.value,
          sourceLanguage: props.sourceLanguage || '',
          targetLanguage: props.targetLanguage,
          targetLanguageCode: props.targetLanguageCode,
          apiKey: apiKey
        })
        
        emit('translation-complete', result)
        translatedVttPath.value = result.translated_vtt_path
        
        // Automatically start TTS generation after translation completes
        // Use the base_filename from the translation result
        generateTTS(result.translated_vtt_path, result.base_filename);
        
        // Hide indicator after 3 seconds
        setTimeout(() => {
          isTranslating.value = false
          isTranscribing.value = false
        }, 3000)
      } catch (e) {
        console.error('Failed to translate:', e)
        emit('translation-error', e instanceof Error ? e.message : 'Failed to translate. Please try again.')
        
        // Hide indicator after 5 seconds in case of error
        setTimeout(() => {
          isTranslating.value = false
          isTranscribing.value = false
        }, 5000)
      }
    }, 100) // Small delay
  } catch (e) {
    console.error('Failed to initialize translation:', e)
    emit('translation-error', e instanceof Error ? e.message : 'Failed to initialize translation. Please try again.')
    
    // Hide indicator after 5 seconds in case of error
    setTimeout(() => {
      isTranslating.value = false
      isTranscribing.value = false
    }, 5000)
  }
}

// Update function to generate TTS from subtitle file with base filename
const generateTTS = async (subtitlePath: string | null, baseFilename: string) => {
  if (!subtitlePath) return

  try {
    isGeneratingTTS.value = true
    
    // Get OpenAI API key from store
    const store = await TauriStore.load('.settings.dat')
    const apiKey = await store.get('openai-api-key') as string
    
    if (!apiKey) {
      throw new Error('OpenAI API key not found. Please add it in the settings.')
    }
    
    // Use setTimeout for non-blocking execution
    setTimeout(async () => {
      try {
        const result = await invoke<TTSResult>('generate_speech', {
          vttPath: subtitlePath,
          outputPath: selectedPath.value,
          apiKey: apiKey,
          voice: 'nova', // Default voice, could be made configurable
          model: 'tts-1', // Default model, could be made configurable
          wordsPerSecond: 2.5, // Default words per second, could be made configurable
          baseFilename: baseFilename
        })
        
        ttsAudioPath.value = result.audio_path
        
        // Hide indicator after 3 seconds
        setTimeout(() => {
          isGeneratingTTS.value = false
        }, 3000)
      } catch (e) {
        console.error('Failed to generate TTS:', e)
        
        // Hide indicator after 5 seconds in case of error
        setTimeout(() => {
          isGeneratingTTS.value = false
        }, 5000)
      }
    }, 100) // Small delay
  } catch (e) {
    console.error('Failed to initialize TTS generation:', e)
    
    // Hide indicator after 5 seconds in case of error
    setTimeout(() => {
      isGeneratingTTS.value = false
    }, 5000)
  }
}

// Function to open a file
const openFile = async (path: string | null) => {
  if (path) {
    try {
      // Use the invoke pattern instead of direct plugin import
      await invoke('plugin:opener:open', { path })
    } catch (e) {
      console.error('Failed to open file:', e)
    }
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

.tts-controls {
  margin-top: 1rem;
  padding: 1rem;
  background-color: var(--background-secondary, #f5f5f5);
  border-radius: 8px;
}

.tts-button {
  display: flex;
  align-items: center;
  justify-content: center;
  background-color: var(--primary, #3b82f6);
  color: white;
  border: none;
  border-radius: 4px;
  padding: 0.5rem 1rem;
  font-size: 0.9rem;
  font-weight: 500;
  cursor: pointer;
  transition: background-color 0.2s ease;
}

.tts-button:hover {
  background-color: var(--primary-dark, #2563eb);
}

.tts-button:disabled {
  background-color: var(--disabled, #9ca3af);
  cursor: not-allowed;
}

.tts-result {
  margin-top: 1rem;
  font-size: 0.9rem;
  color: var(--text-secondary);
}

.play-button {
  display: flex;
  align-items: center;
  justify-content: center;
  background-color: var(--success, #10b981);
  color: white;
  border: none;
  border-radius: 4px;
  padding: 0.5rem 1rem;
  font-size: 0.9rem;
  font-weight: 500;
  cursor: pointer;
  margin-top: 0.5rem;
  transition: background-color 0.2s ease;
}

.play-button:hover {
  background-color: var(--success-dark, #059669);
}

.button-content {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.icon {
  flex-shrink: 0;
}
</style> 