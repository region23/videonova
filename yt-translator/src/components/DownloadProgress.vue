<template>
  <div class="download-progress">
    <div class="progress-info">
      <div class="status">
        <span class="component">{{ componentLabel }}:</span> {{ status }}
      </div>
      <div class="details" v-if="showDetails">
        <span class="speed" v-if="speed">{{ speed }}</span>
        <span class="eta" v-if="eta">ETA: {{ eta }}</span>
      </div>
    </div>
    <div class="progress-bar">
      <div 
        class="progress-fill"
        :style="{ 
          width: `${progress}%`,
          backgroundColor: componentColor
        }"
      ></div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

interface Props {
  status: string
  progress: number
  speed?: string
  eta?: string
  component: string
}

const props = defineProps<Props>()

const componentLabel = computed(() => 
  props.component.charAt(0).toUpperCase() + props.component.slice(1)
)

const componentColor = computed(() => {
  if (props.progress === 100) return '#4CAF50'
  return props.component === 'audio' ? 'var(--accent-secondary, #00b4d8)' : 'var(--accent-primary, #0077ff)'
})

const showDetails = computed(() => props.speed || props.eta)
</script>

<style scoped>
.download-progress {
  width: 100%;
  margin-bottom: 1rem;
}

.download-progress:last-child {
  margin-bottom: 0;
}

.progress-info {
  display: flex;
  justify-content: space-between;
  margin-bottom: 0.5rem;
  color: var(--text-primary);
}

.status {
  font-weight: 500;
}

.component {
  color: var(--accent-primary);
  margin-right: 0.5rem;
}

.details {
  display: flex;
  gap: 1rem;
  font-size: 0.9em;
  color: var(--text-secondary);
}

.progress-bar {
  width: 100%;
  height: 6px;
  background-color: var(--background-secondary);
  border-radius: 3px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  transition: width 0.3s ease;
}
</style> 