<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { Store as TauriStore } from '@tauri-apps/plugin-store'
import YouTubeInput from './YouTubeInput.vue'
import LanguageSelector from './LanguageSelector.vue'
import VideoPreview from './VideoPreview.vue'
import ApiKeyInput from './ApiKeyInput.vue'
import appLogo from '../assets/app_icon_2.png'

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

interface TTSResult {
  audio_path: string
}

interface MergeResult {
  output_path: string
}

interface TranscriptionResult {
  vtt_path: string
}

interface TranslationResult {
  translated_vtt_path: string
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

// Add state for tracking process results
const downloadResult = ref<DownloadResult | null>(null)
const transcriptionResult = ref<TranscriptionResult | null>(null)
const translationResult = ref<TranslationResult | null>(null)

// Listen for the show-settings event
let unlisten: (() => void) | undefined

// Add new refs for progress states
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
  if (info) {
    videoInfo.value = info;
    isVideoInfoReady.value = true;
  } else {
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
  downloadResult.value = result
}

const handleTranscriptionProgress = (progress: any) => {
  transcriptionProgress.value = progress
}

const handleTranscriptionComplete = (result: TranscriptionResult) => {
  transcriptionResult.value = result
  transcriptionProgress.value = { status: 'Complete', progress: 100 }
}

const handleTranslationProgress = (progress: any) => {
  translationProgress.value = progress
}

const handleTranslationComplete = (result: TranslationResult) => {
  translationResult.value = result
  translationProgress.value = { status: 'Complete', progress: 100 }
}

const handleTTSProgress = (progress: any) => {
  ttsProgress.value = progress
}

const handleTTSComplete = (ttsResult: TTSResult) => {
  console.log('TTS complete, starting merge process', ttsResult)
  startMergeProcess(ttsResult)
}

const startMergeProcess = async (ttsResult: TTSResult) => {
  // Emit event to notify that merge is starting
  window.emit('merge-start', {});
  
  if (!downloadResult.value) {
    console.error('Download result is missing, cannot start merge')
    return
  }

  if (!transcriptionResult.value || !translationResult.value) {
    console.error('Transcription or translation results are missing, cannot start merge')
    return
  }

  try {
    // Set up a single listener for progress updates
    const unlistenMergeProgress = await listen('merge-progress', (event) => {
      handleMergeProgress(event.payload)
    })
    
    // Don't set up another merge-complete listener here
    // Let VideoPreview handle the complete event
    
    await invoke<MergeResult>('merge_media', {
      videoPath: downloadResult.value.video_path,
      translatedAudioPath: ttsResult.audio_path,
      originalAudioPath: downloadResult.value.audio_path,
      originalVttPath: transcriptionResult.value?.vtt_path,
      translatedVttPath: translationResult.value?.translated_vtt_path,
      outputPath: selectedPath.value
    })
    
    // Clean up the progress listener when done
    setTimeout(() => unlistenMergeProgress(), 2000);
  } catch (e) {
    console.error('Failed to merge media:', e)
    handleDownloadError(e instanceof Error ? e.message : 'Failed to merge media')
  }
}

const handleMergeProgress = (progress: any) => {
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
  if (!videoInfo.value || !selectedLanguages.value || !selectedPath.value || !isVideoInfoReady.value) {
    console.warn('Process aborted: missing required data')
    return
  }

  try {
    const store = await TauriStore.load('.settings.dat')
    const apiKey = await store.get('openai-api-key') as string
    
    if (!apiKey) {
      console.warn('Process aborted: No API key found')
      error.value = 'Please set your OpenAI API key in settings first'
      showApiKeyUpdate.value = true
      return
    }

    isProcessing.value = true
    error.value = ''
    
    // Reset progress states
    transcriptionProgress.value = null
    translationProgress.value = null
    ttsProgress.value = null
    mergeProgress.value = null

    // Pre-set download result paths
    const videoFileName = currentUrl.value.split('v=')[1].replace(/[^a-zA-Z0-9]/g, '_')
    downloadResult.value = {
      video_path: `${selectedPath.value}/${videoFileName}_video.mp4`,
      audio_path: `${selectedPath.value}/${videoFileName}_audio.m4a`
    }
    
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
    
    // Set all results after process_video completes
    downloadResult.value = {
      video_path: result.video_path,
      audio_path: result.audio_path
    }
    
    transcriptionResult.value = {
      vtt_path: result.transcription_path
    }
    
    translationResult.value = {
      translated_vtt_path: result.translation_path
    }

    // Start merge process with TTS result
    if (result.tts_path) {
      console.log('Starting merge process after video processing')
      await startMergeProcess({ audio_path: result.tts_path })
    } else {
      console.error('TTS path is missing from process_video result')
      error.value = 'Failed to get TTS audio path'
      isProcessing.value = false
    }

  } catch (e) {
    console.error('Pipeline failed:', e instanceof Error ? e.message : e)
    handleDownloadError(e instanceof Error ? e.message : 'Failed to process video')
    // Reset all results if process failed
    downloadResult.value = null
    transcriptionResult.value = null
    translationResult.value = null
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
            <header class="app-header">
              <div class="app-branding">
                <img :src="appLogo" alt="Videonova Logo" class="app-logo" />
                <div class="app-info">
                  <h1 class="app-name">Videonova</h1>
                  <p class="app-description">Translate your&nbsp;favorite YouTube&nbsp;videos into&nbsp;any language with&nbsp;AI&#8209;powered voice&nbsp;translation</p>
                </div>
              </div>
            </header>

            <div class="divider"></div>

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
                {{ isProcessing ? 'Translating...' : 'Translate Video' }}
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

.app-header {
  margin-bottom: 0.5rem;
}

.app-branding {
  display: flex;
  align-items: center;
  gap: 1.25rem;
}

.app-logo {
  width: 88px;
  height: 88px;
  object-fit: contain;
}

.app-info {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.app-name {
  font-size: 1.5rem;
  font-weight: 700;
  color: var(--text-primary);
  letter-spacing: -0.02em;
  margin: 0;
  background: linear-gradient(135deg, #4F46E5, #E11D48, #F97316);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  background-clip: text;
}

.app-description {
  font-size: 0.875rem;
  color: var(--text-secondary);
  margin: 0;
  line-height: 1.4;
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

  .app-branding {
    flex-direction: column;
    text-align: center;
    gap: 1rem;
  }

  .app-info {
    align-items: center;
  }

  .app-description {
    font-size: 0.9rem;
  }

  .description {
    font-size: 1rem;
  }
}
</style>
