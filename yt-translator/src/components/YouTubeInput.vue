<script setup lang="ts">
import { ref } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'

const props = defineProps<{
  disabled?: boolean
}>()

const youtubeUrl = ref('')
const selectedPath = ref('')
const emit = defineEmits(['urlSubmit'])

const validateAndSubmit = () => {
  if (!selectedPath.value) {
    alert('Please select output folder first')
    return
  }

  // Basic YouTube URL validation
  const youtubeRegex = /^(https?:\/\/)?(www\.)?(youtube\.com|youtu\.be)\/.+$/
  if (!youtubeRegex.test(youtubeUrl.value)) {
    alert('Please enter a valid YouTube URL')
    return
  }
  emit('urlSubmit', { url: youtubeUrl.value, outputPath: selectedPath.value })
}

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
</script>

<template>
  <div class="youtube-input">
    <h2>Enter Video URL</h2>
    <form @submit.prevent="validateAndSubmit" class="input-form">
      <div class="input-wrapper">
        <div class="url-input-group">
          <input
            v-model="youtubeUrl"
            type="url"
            placeholder="https://www.youtube.com/watch?v=..."
            required
            :disabled="disabled"
          />
          <button 
            type="button" 
            class="folder-button" 
            @click="selectFolder"
            :disabled="disabled"
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
        <button type="submit" :disabled="disabled || !selectedPath">
          <span class="button-content">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" class="icon">
              <path d="M5 12h14m-4-4l4 4-4 4" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
            Process Video
          </span>
        </button>
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