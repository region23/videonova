<script setup lang="ts">
interface Step {
  id: string
  label: string
  status: 'pending' | 'active' | 'completed'
}

defineProps<{
  steps: Step[]
}>()
</script>

<template>
  <div class="stepper">
    <div class="steps">
      <div
        v-for="(step, index) in steps"
        :key="step.id"
        class="step"
        :class="[step.status]"
      >
        <!-- Step circle with icon -->
        <div class="step-circle">
          <span v-if="step.status === 'pending'" class="step-number">{{ index + 1 }}</span>
          <span v-else-if="step.status === 'active'" class="step-icon">⋯</span>
          <span v-else class="step-icon">✓</span>
        </div>
        
        <!-- Step label -->
        <span class="step-label">{{ step.label }}</span>
        
        <!-- Connector line (except for last step) -->
        <div v-if="index < steps.length - 1" class="step-connector"></div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.stepper {
  padding: 0.5rem;
  background: var(--background-secondary, #f5f5f5);
  border-radius: 12px;
  margin-bottom: 1rem;
}

.steps {
  display: flex;
  justify-content: space-between;
  align-items: center;
  position: relative;
}

.step {
  display: flex;
  align-items: center;
  flex: 1;
  position: relative;
}

.step-circle {
  width: 20px;
  height: 20px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 600;
  font-size: 0.7rem;
  flex-shrink: 0;
  transition: all 0.3s ease;
}

.step-label {
  font-size: 0.7rem;
  margin-left: 0.25rem;
  white-space: nowrap;
  transition: all 0.3s ease;
}

.step-connector {
  flex: 1;
  height: 1px;
  margin: 0 0.25rem;
  transition: all 0.3s ease;
}

/* Pending state */
.pending .step-circle {
  background: #e5e7eb;
  color: #6b7280;
}

.pending .step-label {
  color: #6b7280;
}

.pending .step-connector {
  background: #e5e7eb;
}

/* Active state */
.active .step-circle {
  background: #3b82f6;
  color: white;
}

.active .step-label {
  color: #3b82f6;
  font-weight: 600;
}

.active .step-connector {
  background: #e5e7eb;
}

/* Completed state */
.completed .step-circle {
  background: #10b981;
  color: white;
}

.completed .step-label {
  color: #10b981;
}

.completed .step-connector {
  background: #10b981;
}

/* Responsive adjustments */
@media (max-width: 640px) {
  .step-label {
    font-size: 0.65rem;
  }
  
  .step-circle {
    width: 18px;
    height: 18px;
  }
}
</style> 