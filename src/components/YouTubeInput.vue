<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
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

interface MergeResult {
  output_path: string
  output_dir: string
}

const props = defineProps<{
  disabled?: boolean
  folderSelectDisabled?: boolean
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
  'merge-complete': [result: MergeResult]
  'merge-error': [error: string]
  'transcription-progress': [progress: any]
  'translation-progress': [progress: any]
  'tts-progress': [progress: any]
  'merge-progress': [progress: any]
  'tts-complete': [result: TTSResult]
}>()

const youtubeUrl = ref('')
const selectedPath = ref('')
const isLoading = ref(false)
const isTranscribing = ref(false)
const isMerging = ref(false)
const showTranscription = ref(false)
const downloadResult = ref<DownloadResult | null>(null)
const audioPath = ref<string | null>(null)
const vttPath = ref<string | null>(null)
const translatedVttPath = ref<string | null>(null)
const ttsAudioPath = ref<string | null>(null)
const videoPath = ref<string | null>(null)

// Listen for clear-video-info event
listen('clear-video-info', () => {
  youtubeUrl.value = '';
});

// Listen for audio-ready event and automatically start transcription
listen('audio-ready', async (event) => {
  const path = event.payload as string
  audioPath.value = path
  
  // Вытаскиваем путь к видеофайлу из пути к аудиофайлу, если он отсутствует
  if (!videoPath.value) {
    const audioPathStr = path as string
    if (audioPathStr.endsWith('_audio.m4a')) {
      const basePathWithoutSuffix = audioPathStr.slice(0, -10) // убираем '_audio.m4a'
      videoPath.value = `${basePathWithoutSuffix}.mp4` // предполагаем, что видео в формате mp4
      
      // Set downloadResult if it's not already set
      if (!downloadResult.value) {
        downloadResult.value = {
          video_path: videoPath.value,
          audio_path: audioPath.value
        }
        console.log('Setting downloadResult from audio-ready:', downloadResult.value)
      }
    }
  }
  
  // Показываем раздел транскрибации
  isTranscribing.value = true
  showTranscription.value = true
  
  // Автоматически запускаем транскрибацию
  startTranscriptionWithPath(path)
})

// Move download-complete listener outside onMounted
listen<DownloadResult>('download-complete', (event) => {
  console.log('Download complete event received:', event.payload)
  handleDownloadComplete(event.payload)
})

const selectFolder = async () => {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
    })
    if (selected) {
      selectedPath.value = selected as string
      // Only emit start-download if we have a valid URL and video info is loaded
      if (youtubeUrl.value && !isLoading.value) {
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
    console.log('Starting transcription with path:', path)
    // Get OpenAI API key from store
    const store = await TauriStore.load('.settings.dat')
    const apiKey = await store.get('openai-api-key') as string
    
    if (!apiKey) {
      throw new Error('OpenAI API key not found. Please add it in the settings.')
    }
    
    // Используем setTimeout для обеспечения неблокирующего вызова
    setTimeout(async () => {
      try {
        // Listen for transcription progress
        const unlistenTranscriptionProgress = await listen('transcription-progress', (event) => {
          emit('transcription-progress', event.payload)
        })

        const result = await invoke<TranscriptionResult>('transcribe_audio', {
          audioPath: path,
          outputPath: selectedPath.value,
          apiKey: apiKey,
          language: props.sourceLanguage || ''
        })
        
        console.log('Transcription complete, setting vttPath:', result.vtt_path)
        unlistenTranscriptionProgress()
        emit('transcription-complete', result)
        vttPath.value = result.vtt_path
        console.log('Updated state after transcription:', {
          vttPath: vttPath.value,
          downloadResult: downloadResult.value
        })
        
        // Auto-start translation if target language is set
        if (props.targetLanguage && props.targetLanguageCode) {
          startTranslation(result.vtt_path)
        }
      } catch (e) {
        console.error('Failed to transcribe:', e)
        emit('transcription-error', e instanceof Error ? e.message : 'Failed to transcribe. Please try again.')
      }
    }, 100)
  } catch (e) {
    console.error('Failed to initialize transcription:', e)
    emit('transcription-error', e instanceof Error ? e.message : 'Failed to initialize transcription. Please try again.')
  }
}

// Add translation progress listener
const startTranslation = async (vttPath: string) => {
  try {
    const store = await TauriStore.load('.settings.dat')
    const apiKey = await store.get('openai-api-key') as string

    if (!apiKey) {
      throw new Error('OpenAI API key not found')
    }

    // Listen for translation progress
    const unlistenTranslationProgress = await listen('translation-progress', (event) => {
      emit('translation-progress', event.payload)
    })

    const result = await invoke<TranslationResult>('translate_vtt', {
      vttPath,
      outputPath: selectedPath.value,
      sourceLanguage: props.sourceLanguage || '',
      targetLanguage: props.targetLanguage || '',
      targetLanguageCode: props.targetLanguageCode || '',
      apiKey
    })
    
    unlistenTranslationProgress()
    emit('translation-complete', result)
    translatedVttPath.value = result.translated_vtt_path

    // Auto-start TTS
    startTTS(result.translated_vtt_path)
  } catch (e) {
    console.error('Failed to translate:', e)
    emit('translation-error', e instanceof Error ? e.message : 'Failed to translate. Please try again.')
  }
}

// Add TTS progress listener
const startTTS = async (translatedVttPath: string) => {
  console.log('Starting TTS generation for path:', translatedVttPath)
  try {
    const store = await TauriStore.load('.settings.dat')
    const apiKey = await store.get('openai-api-key') as string

    if (!apiKey) {
      throw new Error('OpenAI API key not found')
    }

    // Debug logging
    console.log('Current state before TTS:', {
      downloadResult: downloadResult.value,
      vttPath: vttPath.value,
      videoPath: videoPath.value,
      audioPath: audioPath.value
    })

    if (!downloadResult.value || !vttPath.value) {
      console.error('Missing required files:', {
        hasDownloadResult: !!downloadResult.value,
        hasVttPath: !!vttPath.value,
        downloadResult: downloadResult.value,
        vttPath: vttPath.value
      })
      throw new Error('Missing required video or transcription files')
    }

    // Validate file paths
    const videoExists = await invoke('check_file_exists_command', { path: downloadResult.value.video_path })
    const audioExists = await invoke('check_file_exists_command', { path: downloadResult.value.audio_path })
    const vttExists = await invoke('check_file_exists_command', { path: vttPath.value })

    console.log('File existence check:', {
      videoExists,
      audioExists,
      vttExists,
      videoPath: downloadResult.value.video_path,
      audioPath: downloadResult.value.audio_path,
      vttPath: vttPath.value
    })

    if (!videoExists || !audioExists || !vttExists) {
      throw new Error('One or more required files are missing on disk')
    }

    // Listen for TTS progress
    const unlistenTTSProgress = await listen('tts-progress', (event) => {
      console.log('TTS progress received:', event.payload)
      emit('tts-progress', event.payload)
    })

    console.log('Invoking generate_speech with parameters:', {
      videoPath: downloadResult.value.video_path,
      audioPath: downloadResult.value.audio_path,
      originalVttPath: vttPath.value,
      translatedVttPath: translatedVttPath,
      outputPath: selectedPath.value,
      apiKey,
      voice: 'ash',
      model: 'tts-1',
      wordsPerSecond: 3.0
    })

    const result = await invoke<TTSResult>('generate_speech', {
      videoPath: downloadResult.value.video_path,
      audioPath: downloadResult.value.audio_path,
      originalVttPath: vttPath.value,
      translatedVttPath: translatedVttPath,
      outputPath: selectedPath.value,
      apiKey,
      voice: 'ash',
      model: 'tts-1',
      wordsPerSecond: 3.0
    })
    
    console.log('TTS generation complete, result:', result)
    console.log('TTS file path:', result.audio_path)
    console.log('Checking if TTS file exists at path:', result.audio_path)
    
    unlistenTTSProgress()
    ttsAudioPath.value = result.audio_path

    // Генерируем событие о завершении генерации TTS
    emit('tts-complete', result)

    // Removing automatic merge trigger, letting MainLayout handle it
  } catch (e) {
    console.error('Failed to generate TTS:', e)
    emit('translation-error', e instanceof Error ? e.message : 'Failed to generate TTS. Please try again.')
  }
}

const handleDownloadComplete = (result: DownloadResult) => {
  console.log('Download complete, setting downloadResult:', result)
  downloadResult.value = result
  
  // Сохраняем путь к видеофайлу
  videoPath.value = result.video_path
  console.log('Updated state after download:', {
    downloadResult: downloadResult.value,
    videoPath: videoPath.value
  })
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
      <form @submit.prevent="startDownload" class="input-form">
        <div class="url-input-group">
          <input
            v-model="youtubeUrl"
            type="url"
            placeholder="Paste YouTube link here..."
            required
            :disabled="disabled || isLoading"
            @input="getVideoInfo"
          />
          <button 
            type="button" 
            class="folder-button" 
            @click="selectFolder"
            :disabled="disabled"
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
                Where to save
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
  pointer-events: none;
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