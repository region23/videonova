<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { listen } from '@tauri-apps/api/event'
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

const handleDownloadStart = () => {
  if (!selectedLanguages.value) {
    error.value = 'Please select source and target languages first'
    return
  }
  isProcessing.value = true
  error.value = ''
}

const handleDownloadComplete = (result: DownloadResult) => {
  console.log('Download completed:', result)
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
</script>

<template>
  <div class="main-layout">
    <div v-if="!showApiKeyUpdate">
      <main>
        <div class="content-wrapper">
          <div class="content-card main-content">
            <LanguageSelector @languages-selected="handleLanguagesSelected" />
            <div class="divider"></div>
            <YouTubeInput 
              :disabled="isProcessing"
              @video-info="handleVideoInfo"
              @download-start="handleDownloadStart"
              @download-complete="handleDownloadComplete"
              @download-error="handleDownloadError"
            />
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
}

.info-content {
  display: flex;
  flex-direction: column;
}

.divider {
  height: 1px;
  background-color: var(--border-color);
  margin: 2rem 0;
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