<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import DownloadProgress from './DownloadProgress.vue'

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

interface DownloadResult {
  video_path: string
  audio_path: string
}

const props = defineProps<{
  disabled?: boolean
}>()

const youtubeUrl = ref('')
const selectedPath = ref('')
const videoInfo = ref<VideoInfo | null>(null)
const audioProgress = ref<DownloadProgress | null>(null)
const videoProgress = ref<DownloadProgress | null>(null)
const isLoading = ref(false)

// Create a cleanup function for the event listener
let unlisten: (() => void) | null = null;

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
});

// Cleanup listener on unmount
onUnmounted(() => {
  if (unlisten) {
    unlisten();
  }
});

const selectFolder = async () => {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
    })
    if (selected) {
      selectedPath.value = selected as string
    }
  } catch (e) {
    console.error('Failed to select folder:', e)
  }
}

const getVideoInfo = async () => {
  if (!youtubeUrl.value) return

  try {
    isLoading.value = true
    videoInfo.value = await invoke('get_video_info', {
      url: youtubeUrl.value
    })
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
    
    // Reset progress
    audioProgress.value = {
      status: 'Initializing audio download...',
      progress: 0,
      component: 'audio'
    }
    videoProgress.value = {
      status: 'Initializing video download...',
      progress: 0,
      component: 'video'
    }

    const result = await invoke<DownloadResult>('download_video', {
      url: youtubeUrl.value,
      outputPath: selectedPath.value,
    })

    console.log('Download completed:', result)
  } catch (e) {
    console.error('Failed to download:', e)
    alert('Failed to download. Please try again.')
    
    // Update error state
    audioProgress.value = {
      status: 'Download failed',
      progress: 0,
      component: 'audio'
    }
    videoProgress.value = {
      status: 'Download failed',
      progress: 0,
      component: 'video'
    }
  } finally {
    isLoading.value = false
  }
}
</script>

<template>
  <div class="youtube-input">
    <h2>Enter Video URL</h2>
    <form @submit.prevent="startDownload" class="input-form">
      <div class="input-wrapper">
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
            :title="selectedPath || 'Select output folder'"
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
        <button 
          type="submit" 
          :disabled="disabled || isLoading || !selectedPath || !youtubeUrl"
        >
          <span class="button-content">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" class="icon">
              <path d="M5 12h14m-4-4l4 4-4 4" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
            {{ isLoading ? 'Processing...' : 'Process Video' }}
          </span>
        </button>
      </div>

      <!-- Video info preview -->
      <div v-if="videoInfo" class="video-info">
        <img 
          :src="videoInfo.thumbnail" 
          :alt="videoInfo.title"
          class="video-thumbnail"
        />
        <div class="video-details">
          <h3>{{ videoInfo.title }}</h3>
          <p class="duration">Duration: {{ Math.round(videoInfo.duration / 60) }} minutes</p>
        </div>
      </div>

      <!-- Download progress -->
      <div v-if="audioProgress || videoProgress" class="download-progress-container">
        <DownloadProgress
          v-if="audioProgress"
          v-bind="audioProgress"
        />
        <DownloadProgress
          v-if="videoProgress"
          v-bind="videoProgress"
        />
      </div>
    </form>
  </div>
</template>

<style scoped>
.youtube-input {
  text-align: center;
}

h2 {
  font-size: 1.25rem;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 1.5rem;
  letter-spacing: -0.01em;
}

.input-form {
  max-width: 800px;
  margin: 0 auto;
}

.input-wrapper {
  display: flex;
  gap: 1rem;
  align-items: center;
}

.url-input-group {
  display: flex;
  gap: 0.5rem;
  flex: 1;
  min-width: 0;
}

input {
  flex: 1;
  min-width: 0;
}

input::placeholder {
  color: var(--text-secondary);
  opacity: 0.7;
}

button {
  padding: 10px 20px;
  min-width: 140px;
}

.folder-button {
  background-color: var(--background-secondary);
  color: var(--text-primary);
  border: 1px solid var(--border-color);
  min-width: auto;
  padding: 8px 16px;
}

.folder-button:hover {
  background-color: var(--background-secondary);
  border-color: var(--accent-primary);
  transform: none;
}

.button-content {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  white-space: nowrap;
}

.folder-path {
  max-width: 150px;
  overflow: hidden;
  text-overflow: ellipsis;
}

.icon {
  stroke: currentColor;
  flex-shrink: 0;
}

button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
  transform: none;
}

.video-info {
  margin-top: 2rem;
  display: flex;
  gap: 1rem;
  text-align: left;
  background: var(--background-secondary);
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

.download-progress-container {
  margin-top: 2rem;
}

@media (max-width: 640px) {
  .input-wrapper {
    flex-direction: column;
  }

  .url-input-group {
    width: 100%;
    flex-direction: column;
  }

  input, .folder-button {
    width: 100%;
  }

  button {
    width: 100%;
  }

  .folder-path {
    max-width: none;
  }

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