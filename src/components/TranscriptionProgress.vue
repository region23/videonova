<template>
  <div class="transcription-progress">
    <div class="progress-info">
      <div class="status">
        <span class="component">Transcription:</span> {{ status }}
      </div>
      <div class="percentage">{{ Math.round(progress) }}%</div>
    </div>
    <div class="progress-bar">
      <div 
        class="progress-fill"
        :style="{ 
          width: `${progress}%`,
          backgroundColor: progressColor
        }"
      ></div>
    </div>
    <div class="progress-animation" v-if="progress > 0 && progress < 100">
      <div class="processing-indicator"></div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

interface Props {
  status: string
  progress: number
}

const props = defineProps<Props>()

// Цвет прогресса зависит от его значения
const progressColor = computed(() => {
  return props.progress === 100 ? '#4CAF50' : 'var(--accent-secondary, #5d87ff)'
})
</script>

<style scoped>
.transcription-progress {
  width: 100%;
  margin-bottom: 1rem;
}

.progress-info {
  display: flex;
  justify-content: space-between;
  margin-bottom: 0.75rem;
  color: var(--text-primary);
}

.status {
  font-weight: 500;
}

.percentage {
  font-weight: 600;
  color: var(--accent-primary);
}

.component {
  color: var(--accent-primary);
  margin-right: 0.5rem;
}

.progress-bar {
  width: 100%;
  height: 8px;
  background-color: var(--background-secondary);
  border-radius: 4px;
  overflow: hidden;
  box-shadow: inset 0 1px 3px rgba(0, 0, 0, 0.1);
}

.progress-fill {
  height: 100%;
  transition: width 0.3s ease, background-color 0.5s ease;
}

/* Анимация индикатора обработки */
.progress-animation {
  margin-top: 10px;
  height: 2px;
  width: 100%;
  overflow: hidden;
  position: relative;
}

.processing-indicator {
  position: absolute;
  height: 2px;
  width: 50%;
  background-color: var(--accent-primary);
  animation: processing 1.5s infinite ease-in-out;
}

@keyframes processing {
  0% {
    left: -50%;
  }
  100% {
    left: 100%;
  }
}
</style> 