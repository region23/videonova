<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  status: string
  progress: number
}>()

const progressPercentage = computed(() => {
  return `${Math.round(props.progress)}%`
})

// Dynamically calculate the color based on progress
const progressColor = computed(() => {
  // Start with blue, transition to green at completion
  if (props.progress >= 100) {
    return 'var(--success, #10b981)'
  }
  return 'var(--primary, #3b82f6)'
})
</script>

<template>
  <div class="merge-progress">
    <div class="progress-header">
      <h3 class="progress-title">Final Processing</h3>
      <span class="progress-percentage">{{ progressPercentage }}</span>
    </div>
    
    <div class="progress-status">
      {{ status }}
    </div>
    
    <div class="progress-bar-container">
      <div 
        class="progress-bar" 
        :style="{
          width: progressPercentage,
          backgroundColor: progressColor
        }"
      ></div>
    </div>
  </div>
</template>

<style scoped>
.merge-progress {
  padding: 1rem;
}

.progress-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 0.5rem;
}

.progress-title {
  font-size: 1.1rem;
  font-weight: 600;
  color: var(--text-primary);
  margin: 0;
  letter-spacing: -0.01em;
}

.progress-percentage {
  font-size: 0.9rem;
  font-weight: 600;
  color: var(--text-secondary);
}

.progress-status {
  font-size: 0.9rem;
  color: var(--text-secondary);
  margin-bottom: 0.75rem;
}

.progress-bar-container {
  width: 100%;
  height: 8px;
  background-color: var(--background-tertiary, #e5e7eb);
  border-radius: 4px;
  overflow: hidden;
}

.progress-bar {
  height: 100%;
  transition: width 0.3s ease, background-color 0.3s ease;
}
</style> 