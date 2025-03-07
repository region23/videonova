<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { listen } from '@tauri-apps/api/event'
import DownloadProgress from './DownloadProgress.vue'
import TranscriptionProgress from './TranscriptionProgress.vue'

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

const props = defineProps<{
  videoInfo?: VideoInfo | null
}>()

const audioProgress = ref<DownloadProgress | null>(null)
const videoProgress = ref<DownloadProgress | null>(null)
// Добавляем переменную для прогресса транскрибации
const transcriptionProgress = ref<TranscriptionProgress | null>(null)
const isTranscribing = ref(false)

// Create a cleanup function for the event listener
let unlisten: (() => void) | null = null;
let unlistenTranscription: (() => void) | null = null;

// Setup progress listener
onMounted(async () => {
  unlisten = await listen<DownloadProgress>('download-progress', (event) => {
    const progress = event.payload;
    if (progress.component === 'audio') {
      audioProgress.value = progress;
    } else if (progress.component === 'video') {
      videoProgress.value = progress;
    }
  });

  // Слушаем событие прогресса транскрибации
  unlistenTranscription = await listen<TranscriptionProgress>('transcription-progress', (event) => {
    transcriptionProgress.value = event.payload;
    isTranscribing.value = true;
  });
});

// Cleanup listener on unmount
onUnmounted(() => {
  if (unlisten) {
    unlisten();
  }
  if (unlistenTranscription) {
    unlistenTranscription();
  }
});
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

    <!-- Download progress -->
    <div v-if="audioProgress || videoProgress" class="download-progress-container">
      <h3 class="progress-title">Download Progress</h3>
      <DownloadProgress v-if="audioProgress" v-bind="audioProgress" />
      <DownloadProgress v-if="videoProgress" v-bind="videoProgress" />
    </div>

    <!-- Transcription progress -->
    <div v-if="isTranscribing && transcriptionProgress" class="transcription-progress-container">
      <!-- <h3 class="progress-title">Transcription Progress</h3>
      <p class="transcription-info">
        Transcribing audio to generate VTT subtitles using OpenAI's Whisper API...
      </p> -->
      <TranscriptionProgress 
        :status="transcriptionProgress.status"
        :progress="transcriptionProgress.progress"
      />
    </div>

    <!-- Empty state -->
    <div v-if="!videoInfo && !audioProgress && !videoProgress && !isTranscribing" class="empty-state">
        <p class="description">
          Translate your favorite YouTube videos into any language with AI-powered
          voice translation
        </p>
    </div>
  </div>
</template>

<style scoped>

.description {
  font-size: 1.1rem;
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
  display: flex;
  gap: 1rem;
  text-align: left;
  background: #f5f5f5;
  padding: 1rem;
  border-radius: 8px;
}

.video-thumbnail {
  width: 160px;
  height: 90px;
  object-fit: cover;
  border-radius: 4px;
}

.video-details {
  flex: 1;
  min-width: 0;
}

.video-details h3 {
  margin: 0 0 0.5rem;
  font-size: 1rem;
  line-height: 1.4;
}

.duration {
  margin: 0;
  font-size: 0.9em;
  color: var(--text-secondary);
}

.download-progress-container,
.transcription-progress-container {
  margin-top: 0.5rem;
  background-color: var(--background-secondary, #f5f5f5);
  border-radius: 12px;
  padding: 0.5rem;
  box-shadow: 0 4px 6px rgba(0, 0, 0, 0.05);
}

.progress-title {
  font-size: 1.1rem;
  font-weight: 600;
  color: var(--text-primary);
  margin: 0 0 1rem;
  letter-spacing: -0.01em;
}

.transcription-info {
  margin-bottom: 1rem;
  color: var(--text-secondary);
  font-size: 0.9rem;
}

.empty-state {
  text-align: center;
  color: var(--text-secondary);
  padding: 2rem;
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
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
</style> 