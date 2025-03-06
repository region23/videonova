<script setup lang="ts">
import { ref } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { invoke } from '@tauri-apps/api/core'

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

const props = defineProps<{
  disabled?: boolean
}>()

const emit = defineEmits<{
  'video-info': [info: VideoInfo]
  'download-start': []
  'download-complete': [result: DownloadResult]
  'download-error': [error: string]
}>()

const youtubeUrl = ref('')
const selectedPath = ref('')
const isLoading = ref(false)

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
    const info = await invoke<VideoInfo>('get_video_info', {
      url: youtubeUrl.value
    })
    emit('video-info', info)
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
    
    const result = await invoke<DownloadResult>('download_video', {
      url: youtubeUrl.value,
      outputPath: selectedPath.value,
    })

    console.log('Download completed:', result)
    emit('download-complete', result)
  } catch (e) {
    console.error('Failed to download:', e)
    emit('download-error', e instanceof Error ? e.message : 'Failed to download. Please try again.')
  } finally {
    isLoading.value = false
  }
}
</script>

<template>
  <div class="youtube-input">
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

      <button 
        type="submit" 
        class="process-button"
        :disabled="disabled || isLoading || !selectedPath || !youtubeUrl"
      >
        <span class="button-content">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" class="icon">
            <path d="M5 12h14m-4-4l4 4-4 4" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
          </svg>
          {{ isLoading ? 'Processing...' : 'Process Video' }}
        </span>
      </button>
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
  width: 100%;
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

.process-button {
  width: 100%;
  margin-top: 1rem;
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
}
</style> 