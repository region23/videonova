<script setup lang="ts">
import { ref, watch } from 'vue'
import { languages, findLanguageByCode } from '../utils/languages'

const props = defineProps<{
  initialSourceLanguage?: string
  disabled?: boolean
  sourceLanguageDetected?: boolean
}>()

const sourceLanguage = ref(props.initialSourceLanguage ? findLanguageByCode(props.initialSourceLanguage) : languages[0])
const targetLanguage = ref(languages[1])

const emit = defineEmits(['languagesSelected', 'update:sourceLanguage'])

watch([sourceLanguage, targetLanguage], ([source, target]) => {
  emit('languagesSelected', { source, target })
  emit('update:sourceLanguage', source.code)
}, { immediate: true })

// Следим за изменением пропса
watch(() => props.initialSourceLanguage, (newCode) => {
  if (newCode) {
    sourceLanguage.value = findLanguageByCode(newCode)
  }
})
</script>

<template>
  <div class="language-selector">
    <div class="language-pair">
      <span class="translate-label">Translate from</span>
      
      <div class="language-select">
        <div class="label-container" v-if="props.sourceLanguageDetected">
          <span class="auto-detected-badge">Auto-detected</span>
        </div>
        <div class="select-wrapper">
          <select
            id="source-language"
            v-model="sourceLanguage"
            :disabled="targetLanguage.code === sourceLanguage.code || props.disabled || props.sourceLanguageDetected"
          >
            <option
              v-for="lang in languages"
              :key="lang.code"
              :value="lang"
              :disabled="targetLanguage.code === lang.code"
            >
              {{ lang.name }}
            </option>
          </select>
        </div>
      </div>

      <div class="language-divider">
        <span class="divider-text">to</span>
      </div>

      <div class="language-select">
        <div class="select-wrapper">
          <select
            id="target-language"
            v-model="targetLanguage"
            :disabled="sourceLanguage.code === targetLanguage.code || props.disabled"
          >
            <option
              v-for="lang in languages"
              :key="lang.code"
              :value="lang"
              :disabled="sourceLanguage.code === lang.code"
            >
              {{ lang.name }}
            </option>
          </select>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.language-selector {
  width: 100%;
  padding-top: 16px;
}

.language-pair {
  display: flex;
  gap: 1rem;
  align-items: center;
  width: 100%;
}

.translate-label {
  font-size: 0.875rem;
  color: var(--text-secondary);
  font-weight: 500;
  white-space: nowrap;
}

.language-select {
  position: relative;
  flex: 1;
  min-width: 200px;
}

.language-divider {
  color: var(--text-secondary);
  font-size: 0.875rem;
  white-space: nowrap;
  padding: 0 0.5rem;
}

.label-container {
  position: absolute;
  top: -30px;
  left: 9px;
}

.select-wrapper {
  position: relative;
  width: 100%;
}

select {
  appearance: none;
  background-image: url("data:image/svg+xml;charset=UTF-8,%3csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3e%3cpolyline points='6 9 12 15 18 9'%3e%3c/polyline%3e%3c/svg%3e");
  background-repeat: no-repeat;
  background-position: right 0.7rem center;
  background-size: 1em;
  padding: 0.5rem 2.5rem 0.5rem 0.75rem;
  width: 100%;
  cursor: pointer;
  border: 1px solid var(--border-color, #e2e8f0);
  border-radius: 6px;
  font-size: 0.875rem;
}

select:disabled {
  opacity: 0.5;
  cursor: not-allowed;
  pointer-events: none;
}

.auto-detected-badge {
  font-size: 0.7rem;
  background-color: var(--accent-secondary, #4cd964);
  color: white;
  padding: 2px 6px;
  border-radius: 4px;
  font-weight: 500;
  display: inline-block;
}

@media (max-width: 640px) {
  .language-pair {
    flex-direction: column;
    gap: 0.5rem;
  }

  .language-select {
    width: 100%;
  }
  
  .label-container {
    position: static;
    transform: none;
    margin-bottom: 4px;
  }
}
</style> 