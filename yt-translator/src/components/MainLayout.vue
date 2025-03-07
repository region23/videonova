<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
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

const isProcessing = ref(false)
const error = ref('')
const selectedLanguages = ref<LanguagePair | null>(null)
const videoInfo = ref<VideoInfo | null>(null)
const showApiKeyUpdate = ref(false)
const sourceLanguage = ref('')
const currentUrl = ref('')
const selectedPath = ref('')
const isMainContentLocked = ref(false)

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
  videoInfo.value = info
}

const handleLanguageDetected = (code: string) => {
  sourceLanguage.value = code
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
  isProcessing.value = false
  
  // Отправляем событие download-complete в YouTubeInput
  console.log("Download completed in MainLayout, emitting result:", result)
}

const handleTranscriptionProgress = (progress: any) => {
  console.log("Transcription progress in MainLayout:", progress)
  transcriptionProgress.value = progress
}

const handleTranscriptionComplete = (result: any) => {
  console.log("Transcription complete in MainLayout:", result)
  // Устанавливаем progress в 100%, чтобы гарантировать правильное состояние
  transcriptionProgress.value = { status: 'Complete', progress: 100 }
}

const handleTranslationProgress = (progress: any) => {
  console.log("Translation progress in MainLayout:", progress)
  translationProgress.value = progress
}

const handleTranslationComplete = (result: any) => {
  console.log("Translation complete in MainLayout:", result)
  // Устанавливаем progress в 100%, чтобы гарантировать правильное состояние
  translationProgress.value = { status: 'Complete', progress: 100 }
}

const handleTTSProgress = (progress: any) => {
  console.log("TTS progress in MainLayout:", progress)
  ttsProgress.value = progress
}

const handleTTSComplete = (result: any) => {
  console.log("TTS complete in MainLayout:", result)
  // Устанавливаем progress в 100%, чтобы гарантировать правильное состояние
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
  if (!videoInfo.value) {
    error.value = 'Please enter a valid YouTube URL first'
    return
  }
  if (!selectedLanguages.value) {
    error.value = 'Please select languages first'
    return
  }
  if (!currentUrl.value || !selectedPath.value) {
    error.value = 'Please select a download folder first'
    return
  }

  try {
    isProcessing.value = true
    isMainContentLocked.value = true
    error.value = ''
    
    const result = await invoke<DownloadResult>('download_video', {
      url: currentUrl.value,
      outputPath: selectedPath.value,
    })
    
    handleDownloadComplete(result)
  } catch (e) {
    console.error('Failed to download:', e)
    handleDownloadError(e instanceof Error ? e.message : 'Failed to download. Please try again.')
  }
}

const handleMergeComplete = () => {
  isMainContentLocked.value = false
  isProcessing.value = false
}
</script>

<template>
  <div class="main-layout">
    <div v-if="!showApiKeyUpdate">
      <main>
        <div class="content-wrapper">
          <div class="content-card main-content" :class="{ 'content-locked': isMainContentLocked }">
            <YouTubeInput 
              :disabled="isProcessing"
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
            />

            <div v-if="error" class="error-message">
              {{ error }}
            </div>

            <button 
              class="process-button"
              :disabled="isProcessing || !videoInfo"
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
  transition: opacity 0.3s ease, filter 0.3s ease;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.content-locked {
  opacity: 0.7;
  pointer-events: none;
  filter: grayscale(0.5);
}

.content-locked::after {
  content: '';
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(255, 255, 255, 0.5);
  border-radius: 12px;
  cursor: not-allowed;
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