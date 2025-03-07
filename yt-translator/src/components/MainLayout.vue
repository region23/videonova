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

// Listen for the show-settings event
let unlisten: (() => void) | undefined

onMounted(async () => {
  unlisten = await listen('show-settings', () => {
    showApiKeyUpdate.value = true
  })
})

onUnmounted(() => {
  unlisten?.()
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
</script>

<template>
  <div class="main-layout">
    <div v-if="!showApiKeyUpdate">
      <main>
        <div class="content-wrapper">
          <div class="content-card main-content">
            <YouTubeInput 
              :disabled="isProcessing"
              @video-info="handleVideoInfo"
              @language-detected="handleLanguageDetected"
              @download-start="handleDownloadStart"
              @download-complete="handleDownloadComplete"
              @download-error="handleDownloadError"
              @start-download="handleStartDownload"
            />

            <div class="divider"></div>

            <LanguageSelector
              :initial-source-language="sourceLanguage"
              v-model:source-language="sourceLanguage"
              @languages-selected="handleLanguagesSelected"
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
  padding: 0 2rem;
  display: flex;
  flex-direction: column;
}

main {
  padding: 2rem 0;
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: calc(100vh - 100px);
}

.content-wrapper {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 2rem;
  align-items: stretch;
  height: 100%;
}

.content-card {
  background-color: white;
  border-radius: 12px;
  padding: 2rem;
  box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.1), 0 2px 4px -1px rgba(0, 0, 0, 0.06);
  height: 100%;
  display: flex;
  flex-direction: column;
}

.main-content {
  display: flex;
  flex-direction: column;
  gap: 2rem;
}

.info-content {
  display: flex;
  flex-direction: column;
}

.divider {
  height: 1px;
  background-color: var(--border-color);

}

.error-message {
  color: var(--error-color);
  font-size: 0.9rem;
  margin-top: 0.5rem;
}

.process-button {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 40px;
  padding: 0 16px;
  font-weight: 500;
  transition: all 0.2s ease;
  border-radius: 8px;
  background-color: var(--accent-primary);
  color: white;
  border: none;
  cursor: pointer;
  width: 100%;
}

.process-button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.button-content {
  display: flex;
  align-items: center;
  gap: 8px;
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