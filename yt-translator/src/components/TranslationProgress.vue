<script setup lang="ts">
defineProps<{
  status: string
  progress: number
}>()
</script>

<template>
  <div class="translation-progress">
    <h3 class="progress-title">Translation Progress</h3>
    <p class="translation-info">
      Translating subtitles using OpenAI's GPT4o-mini model...
    </p>
    <div class="progress-container">
      <div class="progress-bar">
        <div class="progress-fill" :style="{ width: `${progress}%` }"></div>
      </div>
      <div class="progress-info">
        <span class="status">{{ status }}</span>
        <span class="percent">{{ Math.round(progress) }}%</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.translation-progress {
  padding: 1rem;
  border-radius: 8px;
}

.progress-title {
  font-size: 1.1rem;
  font-weight: 600;
  color: var(--text-primary);
  margin: 0 0 0.5rem;
  letter-spacing: -0.01em;
}

.translation-info {
  margin-bottom: 1rem;
  color: var(--text-secondary);
  font-size: 0.9rem;
}

.progress-container {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.progress-bar {
  height: 8px;
  background-color: rgba(0, 0, 0, 0.1);
  border-radius: 4px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background-color: var(--accent-primary);
  border-radius: 4px;
  transition: width 0.3s ease;
}

.progress-info {
  display: flex;
  justify-content: space-between;
  font-size: 0.9rem;
}

.status {
  color: var(--text-secondary);
}

.percent {
  font-weight: 500;
  color: var(--text-primary);
}
</style> 