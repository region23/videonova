<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { Store as TauriStore } from '@tauri-apps/plugin-store'
import YouTubeInput from './YouTubeInput.vue'
import LanguageSelector from './LanguageSelector.vue'
import VideoPreview from './VideoPreview.vue'
import ApiKeyInput from './ApiKeyInput.vue'

interface Language {
  code: string
  name: string
}

interface LanguagePair {
  source: Language
  target: Language
}

interface VideoInfo {
  title: string
  duration: number
  url: string
  thumbnail: string
  description: string
}

interface DownloadResult {
  video_path: string
  audio_path: string
}

interface ProcessVideoResult {
  video_path: string
  audio_path: string
  transcription_path: string
  translation_path: string
  tts_path: string
  final_path: string
}

const isProcessing = ref(false)
const error = ref('')
const selectedLanguages = ref<LanguagePair | null>(null)
const videoInfo = ref<VideoInfo | null>(null)
const showApiKeyUpdate = ref(false)
const sourceLanguage = ref('')
const currentUrl = ref('')
const selectedPath = ref('')
const isVideoInfoReady = ref(false)
const isSourceLanguageDetected = ref(false)

// Listen for the show-settings event
let unlisten: (() => void) | undefined

// Add new refs for progress states with правильной типизацией
const transcriptionProgress = ref<any>(null)
const translationProgress = ref<any>(null)
const ttsProgress = ref<any>(null)
const mergeProgress = ref<any>(null)

onMounted(async () => {
  unlisten = await listen('show-settings', () => {
    showApiKeyUpdate.value = true
  })

  // Add listener for merge-complete event
  const unlistenMergeComplete = await listen('merge-complete', () => {
    handleMergeComplete()
  })

  onUnmounted(() => {
    unlisten?.()
    unlistenMergeComplete?.()
  })
})

const handleVideoInfo = (info: VideoInfo) => {
  console.log('Video info received:', info ? 'present' : 'null/undefined');
  
  // Если информация о видео успешно загружена, устанавливаем isVideoInfoReady в true
  if (info) {
    console.log('Video info loaded successfully, setting isVideoInfoReady to true');
    videoInfo.value = info;
    isVideoInfoReady.value = true;
  } else {
    console.log('Video info is null/undefined, resetting state');
    videoInfo.value = null;
    isVideoInfoReady.value = false;
  }
}

const handleLanguageDetected = (code: string) => {
  sourceLanguage.value = code
  isSourceLanguageDetected.value = true
}

const handleDownloadStart = () => {
  if (!selectedLanguages.value) {
    error.value = 'Please select source and target languages first'
    return
  }
  isProcessing.value = true
  error.value = ''
}

const handleDownloadComplete = (result: DownloadResult) => {
  console.log("Download completed in MainLayout, emitting result:", result)
}

const handleTranscriptionProgress = (progress: any) => {
  console.log("Transcription progress in MainLayout:", progress)
  transcriptionProgress.value = progress
}

const handleTranscriptionComplete = (result: any) => {
  console.log("Transcription complete in MainLayout:", result)
  transcriptionProgress.value = { status: 'Complete', progress: 100 }
}

const handleTranslationProgress = (progress: any) => {
  console.log("Translation progress in MainLayout:", progress)
  translationProgress.value = progress
}

const handleTranslationComplete = (result: any) => {
  console.log("Translation complete in MainLayout:", result)
  translationProgress.value = { status: 'Complete', progress: 100 }
}

const handleTTSProgress = (progress: any) => {
  console.log("TTS progress in MainLayout:", progress)
  ttsProgress.value = progress
}

const handleTTSComplete = (result: any) => {
  console.log("TTS complete in MainLayout:", result)
  ttsProgress.value = { status: 'Complete', progress: 100 }
}

const handleMergeProgress = (progress: any) => {
  console.log("Merge progress in MainLayout:", progress)
  mergeProgress.value = progress
}

const handleDownloadError = (errorMessage: string) => {
  error.value = errorMessage
  isProcessing.value = false
}

const handleLanguagesSelected = (languages: LanguagePair) => {
  selectedLanguages.value = languages
}

const handleCancelUpdate = () => {
  showApiKeyUpdate.value = false
}

const handleApiKeyUpdated = () => {
  showApiKeyUpdate.value = false
}

const handleStartDownload = async (url: string, path: string) => {
  currentUrl.value = url
  selectedPath.value = path
}

const handleProcessClick = async () => {
  console.log('=== Process Video Started ===')
  console.log('Initial state:', {
    videoInfo: videoInfo.value,
    selectedLanguages: selectedLanguages.value,
    selectedPath: selectedPath.value,
    isVideoInfoReady: isVideoInfoReady.value,
    currentUrl: currentUrl.value,
    isProcessing: isProcessing.value
  })

  if (!videoInfo.value) {
    console.warn('Process aborted: No video info available')
    error.value = 'Please enter a valid YouTube URL first'
    return
  }
  if (!selectedLanguages.value) {
    console.warn('Process aborted: No languages selected')
    error.value = 'Please select languages first'
    return
  }
  if (!selectedPath.value) {
    console.warn('Process aborted: No download folder selected')
    error.value = 'Please select a download folder first'
    return
  }
  if (!isVideoInfoReady.value) {
    console.warn('Process aborted: Video info not ready')
    error.value = 'Please wait for video information to load'
    return
  }

  try {
    // Get API key from store
    const store = await TauriStore.load('.settings.dat')
    const apiKey = await store.get('openai-api-key') as string
    
    if (!apiKey) {
      console.warn('Process aborted: No API key found')
      error.value = 'Please set your OpenAI API key in settings first'
      showApiKeyUpdate.value = true
      return
    }

    console.log('Starting video processing pipeline...')
    console.log('Processing parameters:', {
      url: currentUrl.value,
      outputPath: selectedPath.value,
      targetLanguage: selectedLanguages.value.target.code,
      targetLanguageName: selectedLanguages.value.target.name
    })
    
    isProcessing.value = true
    error.value = ''
    
    // Initialize progress states immediately
    // Сначала показываем только подготовку к загрузке
    transcriptionProgress.value = null
    translationProgress.value = null
    ttsProgress.value = null
    mergeProgress.value = null
    
    const result = await invoke<ProcessVideoResult>('process_video', {
      url: currentUrl.value,
      outputPath: selectedPath.value,
      targetLanguage: selectedLanguages.value.target.code,
      targetLanguageName: selectedLanguages.value.target.name,
      apiKey: apiKey,
      voice: 'alloy',
      model: 'tts-1',
      wordsPerSecond: 3.0
    })
    
    console.log('Processing completed successfully:', result)
    handleDownloadComplete({
      video_path: result.video_path,
      audio_path: result.audio_path
    })
  } catch (e) {
    console.error('Pipeline failed:', e)
    console.error('Error details:', {
      message: e instanceof Error ? e.message : 'Unknown error',
      error: e
    })
    handleDownloadError(e instanceof Error ? e.message : 'Failed to process video. Please try again.')
  }
}

const handleMergeComplete = () => {
  isProcessing.value = false
  transcriptionProgress.value = null
  translationProgress.value = null
  ttsProgress.value = null
  mergeProgress.value = null
}

const handleVideoInfoReadyStateChange = (isReady: boolean) => {
  console.log('Video info ready state changed:', isReady, 'Current videoInfo:', videoInfo.value)
  isVideoInfoReady.value = isReady
  
  // Если видео-информация не готова, сбрасываем флаг определения языка
  if (!isReady) {
    isSourceLanguageDetected.value = false
  }
}
</script>

<template>
  <div class="main-layout">
    <div v-if="!showApiKeyUpdate">
      <main>
        <div class="content-wrapper">
          <div class="content-card main-content">
            <YouTubeInput 
              :disabled="isProcessing"
              :folder-select-disabled="!isVideoInfoReady || isProcessing"
              @video-info="handleVideoInfo"
              @language-detected="handleLanguageDetected"
              @download-start="handleDownloadStart"
              @download-complete="handleDownloadComplete"
              @download-error="handleDownloadError"
              @start-download="handleStartDownload"
              @transcription-progress="handleTranscriptionProgress"
              @transcription-complete="handleTranscriptionComplete"
              @translation-progress="handleTranslationProgress"
              @translation-complete="handleTranslationComplete"
              @tts-progress="handleTTSProgress"
              @tts-complete="handleTTSComplete"
              @merge-progress="handleMergeProgress"
              :source-language="sourceLanguage"
              :target-language="selectedLanguages?.target?.name || ''"
              :target-language-code="selectedLanguages?.target?.code || ''"
              class="youtube-input-section"
            />

            <div class="divider"></div>

            <LanguageSelector
              :initial-source-language="sourceLanguage"
              v-model:source-language="sourceLanguage"
              @languages-selected="handleLanguagesSelected"
              class="language-selector-section"
              :disabled="!isVideoInfoReady || isProcessing"
              :source-language-detected="isSourceLanguageDetected"
            />

            <div v-if="error" class="error-message">
              {{ error }}
            </div>

            <button 
              class="process-button"
              :disabled="isProcessing || !videoInfo || !isVideoInfoReady || !selectedPath"
              @click="handleProcessClick"
            >
              <span class="button-content">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" class="icon">
                  <path d="M5 12h14m-4-4l4 4-4 4" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                </svg>
                {{ isProcessing ? 'Processing...' : 'Process Video' }}
              </span>
            </button>
          </div>
          
          <div class="content-card info-content">
            <VideoPreview 
              :video-info="videoInfo"
              @merge-complete="handleMergeComplete"
              @video-info-ready-state-change="handleVideoInfoReadyStateChange"
              :transcription-progress="transcriptionProgress"
              :translation-progress="translationProgress"
              :tts-progress="ttsProgress"
              :merge-progress="mergeProgress"
            />
          </div>
        </div>
      </main>
    </div>

    <ApiKeyInput
      v-if="showApiKeyUpdate"
      mode="update"
      @apiKeySet="handleApiKeyUpdated"
      @cancel="handleCancelUpdate"
    />
  </div>
</template>

<style scoped>
.main-layout {
  width: 1150px;
  margin: 0 auto;
  padding: 0 1rem;
  display: flex;
  flex-direction: column;
  min-height: 100vh;
}

main {
  padding: 1rem 0;
  flex: 1;
  display: flex;
  flex-direction: column;
}

.content-wrapper {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 1rem;
  margin-top: 0.5rem;
}

.content-card {
  background: white;
  border-radius: 12px;
  padding: 0.75rem;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
}

.main-content {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.info-content {
  display: flex;
  flex-direction: column;
}

.divider {
  height: 1px;
  background-color: var(--border-color);
  margin: 0.5rem 0;
}

.error-message {
  color: var(--error-color);
  font-size: 0.8rem;
  margin-top: 0.125rem;
}

.hint-message {
  color: var(--accent-secondary, #4cd964);
  font-size: 0.8rem;
  margin-top: 0.125rem;
}

.process-button {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 36px;
  padding: 0 12px;
  font-size: 0.9rem;
  font-weight: 500;
  transition: all 0.2s ease;
  border-radius: 6px;
  background-color: var(--accent-primary);
  color: white;
  border: none;
  cursor: pointer;
  width: 100%;
  margin-top: 0.5rem;
}

.process-button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
  pointer-events: none;
}

.button-content {
  display: flex;
  align-items: center;
  gap: 6px;
}

.button-content svg {
  width: 14px;
  height: 14px;
}

.youtube-input-section {
  margin-bottom: 0.25rem;
}

.language-selector-section {
  margin: 0.5rem 0;
}

@media (max-width: 1200px) {
  .main-layout {
    width: 100%;
  }
}

@media (max-width: 768px) {
  .content-wrapper {
    grid-template-columns: 1fr;
  }

  .main-layout {
    padding: 0 1rem;
  }

  .description {
    font-size: 1rem;
  }
}
</style> 