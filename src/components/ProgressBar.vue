<template>
  <div 
    class="progress-container" 
    :class="{ 
      'primary-progress': isActive, 
      'secondary-progress': !isActive,
      'complete-progress': isComplete 
    }"
  >
    <h3 class="progress-title">{{ title }}</h3>
    
    <!-- Subtask display if present -->
    <div v-if="subtask" class="subtask-info">
      {{ subtask }}
    </div>

    <!-- Progress bar -->
    <div class="progress-bar-wrapper">
      <div 
        class="progress-bar" 
        :style="{ width: `${progress}%` }"
        :class="{ 'complete': progress >= 100 }"
      ></div>
      <span class="progress-text">{{ progress }}%</span>
    </div>

    <!-- Additional info like segments if provided -->
    <div v-if="showSegments" class="segments-info">
      Segment {{ currentSegment }} of {{ totalSegments }}
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  title: string
  progress: number
  isActive: boolean
  isComplete?: boolean
  subtask?: string
  currentSegment?: number
  totalSegments?: number
}>()

const showSegments = computed(() => 
  typeof props.currentSegment === 'number' && 
  typeof props.totalSegments === 'number'
)
</script>

<style scoped>
.progress-container {
  background-color: var(--background-secondary, #f5f5f5);
  border-radius: 12px;
  padding: 1rem;
  margin-bottom: 0.5rem;
  transition: all 0.3s ease;
  border: 1px solid rgba(0,0,0,0.05);
}

.primary-progress {
  border-color: var(--primary-color, #0077ff);
  border-width: 2px;
  background-color: rgba(0, 119, 255, 0.05);
  transform: scale(1.01);
  z-index: 10;
}

.secondary-progress {
  opacity: 0.85;
  background-color: var(--background-tertiary, #eaeaea);
  padding: 0.75rem;
  transform: scale(0.98);
  max-height: 80px;
  overflow: hidden;
}

.complete-progress {
  opacity: 0.95;
  background-color: rgba(76, 217, 100, 0.05);
  border-color: var(--success-color, #4cd964);
  padding: 0.5rem;
}

.progress-title {
  font-size: 0.85rem;
  font-weight: 600;
  color: var(--text-primary);
  margin: 0 0 0.5rem;
  letter-spacing: -0.01em;
}

.subtask-info {
  font-size: 0.8rem;
  color: var(--text-secondary);
  margin-bottom: 0.5rem;
}

.progress-bar-wrapper {
  position: relative;
  height: 6px;
  background-color: rgba(0, 0, 0, 0.1);
  border-radius: 3px;
  overflow: hidden;
}

.progress-bar {
  position: absolute;
  left: 0;
  top: 0;
  height: 100%;
  background-color: var(--primary-color, #0077ff);
  border-radius: 3px;
  transition: width 0.3s ease;
}

.progress-bar.complete {
  background-color: var(--success-color, #4cd964);
}

.progress-text {
  position: absolute;
  right: -25px;
  top: -2px;
  font-size: 0.75rem;
  color: var(--text-secondary);
}

.segments-info {
  font-size: 0.75rem;
  color: var(--text-secondary);
  margin-top: 0.5rem;
  text-align: right;
}
</style> 