<script setup lang="ts">
interface Props {
  currentStep: string
  progress: number
  error?: string
}

defineProps<Props>()

const steps = [
  'Downloading video',
  'Extracting audio',
  'Transcribing audio',
  'Translating text',
  'Generating speech',
  'Creating final video'
]
</script>

<template>
  <div class="processing-progress">
    <div v-if="error" class="error-message">
      <svg class="error-icon" viewBox="0 0 24 24" width="24" height="24">
        <path d="M12 22C6.477 22 2 17.523 2 12S6.477 2 12 2s10 4.477 10 10-4.477 10-10 10zm-1-7v2h2v-2h-2zm0-8v6h2V7h-2z" fill="currentColor"/>
      </svg>
      {{ error }}
    </div>
    <div v-else class="progress-container">
      <div class="steps">
        <div
          v-for="(step, index) in steps"
          :key="index"
          class="step"
          :class="{
            'current': step === currentStep,
            'completed': steps.indexOf(currentStep) > index
          }"
        >
          <div class="step-indicator">
            <div class="step-number">{{ index + 1 }}</div>
            <svg v-if="steps.indexOf(currentStep) > index" class="check-icon" viewBox="0 0 24 24" width="16" height="16">
              <path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41L9 16.17z" fill="currentColor"/>
            </svg>
          </div>
          <div class="step-content">
            <div class="step-text">{{ step }}</div>
            <div v-if="step === currentStep" class="step-status">Processing...</div>
          </div>
        </div>
      </div>
      <div class="progress-track">
        <div class="progress-bar">
          <div class="progress-fill" :style="{ width: `${progress}%` }"></div>
        </div>
        <div class="progress-text">{{ progress }}%</div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.processing-progress {
  text-align: left;
}

.progress-container {
  display: flex;
  flex-direction: column;
  gap: 2rem;
}

.steps {
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
}

.step {
  display: flex;
  gap: 1rem;
  align-items: flex-start;
}

.step-indicator {
  position: relative;
  width: 24px;
  height: 24px;
  flex-shrink: 0;
}

.step-number {
  width: 24px;
  height: 24px;
  border-radius: 12px;
  background-color: var(--background-secondary);
  border: 1px solid var(--border-color);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--text-secondary);
}

.step.completed .step-number {
  display: none;
}

.check-icon {
  position: absolute;
  top: 4px;
  left: 4px;
  color: var(--accent-primary);
}

.step-content {
  flex: 1;
  min-width: 0;
}

.step-text {
  font-weight: 500;
  color: var(--text-primary);
  margin-bottom: 0.25rem;
}

.step.completed .step-text {
  color: var(--text-secondary);
}

.step-status {
  font-size: 0.875rem;
  color: var(--text-secondary);
}

.progress-track {
  display: flex;
  align-items: center;
  gap: 1rem;
}

.progress-bar {
  flex: 1;
  height: 6px;
  background-color: var(--border-color);
  border-radius: 3px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background-color: var(--accent-primary);
  border-radius: 3px;
  transition: width 0.3s ease;
}

.progress-text {
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--text-secondary);
  min-width: 3rem;
}

.error-message {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 1rem;
  border-radius: 12px;
  background-color: var(--error-color);
  color: white;
}

.error-icon {
  flex-shrink: 0;
}

@media (max-width: 640px) {
  .steps {
    gap: 1rem;
  }

  .step {
    gap: 0.75rem;
  }

  .step-indicator {
    width: 20px;
    height: 20px;
  }

  .step-number {
    width: 20px;
    height: 20px;
    font-size: 0.75rem;
  }

  .check-icon {
    width: 14px;
    height: 14px;
    top: 3px;
    left: 3px;
  }

  .step-text {
    font-size: 0.875rem;
  }

  .step-status {
    font-size: 0.75rem;
  }
}
</style> 