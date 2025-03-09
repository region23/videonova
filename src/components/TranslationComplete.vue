<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'

const props = defineProps<{
  outputDir: string
}>()

const openFolder = async () => {
  try {
    await invoke('plugin:opener:open', { path: props.outputDir })
  } catch (e) {
    console.error('Failed to open folder:', e)
  }
}
</script>

<template>
  <div class="translation-complete">
    <div class="icon-container">
      <svg class="check-icon" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"></path>
        <polyline points="22 4 12 14.01 9 11.01"></polyline>
      </svg>
    </div>
    <h3 class="title">Translation Complete!</h3>
    <p class="description">
      Your video has been successfully translated with both audio and subtitles.
    </p>
    <button class="open-folder-btn" @click="openFolder">
      <svg class="folder-icon" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path>
      </svg>
      Open Folder
    </button>
  </div>
</template>

<style scoped>
.translation-complete {
  padding: 2rem;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
  background-color: var(--background-secondary, #f5f5f5);
  border-radius: 12px;
  box-shadow: 0 4px 6px rgba(0, 0, 0, 0.05);
}

.icon-container {
  margin-bottom: 1.5rem;
  width: 64px;
  height: 64px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  background-color: var(--success-light, rgba(16, 185, 129, 0.1));
}

.check-icon {
  width: 36px;
  height: 36px;
  color: var(--success, #10b981);
}

.title {
  font-size: 1.5rem;
  font-weight: 700;
  color: var(--text-primary);
  margin: 0 0 0.75rem;
}

.description {
  font-size: 1rem;
  color: var(--text-secondary);
  margin: 0 0 2rem;
  max-width: 500px;
}

.open-folder-btn {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.75rem 1.5rem;
  background-color: var(--primary, #3b82f6);
  color: white;
  border: none;
  border-radius: 8px;
  font-size: 1rem;
  font-weight: 600;
  cursor: pointer;
  transition: background-color 0.2s;
}

.open-folder-btn:hover {
  background-color: var(--primary-dark, #2563eb);
}

.folder-icon {
  width: 20px;
  height: 20px;
}
</style> 