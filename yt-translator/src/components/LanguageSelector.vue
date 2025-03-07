<script setup lang="ts">
import { ref, watch } from 'vue'
import { languages, findLanguageByCode } from '../utils/languages'

const props = defineProps<{
  initialSourceLanguage?: string
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
    <h2>Select Languages</h2>
    <div class="language-pair">
      <div class="language-select">
        <label for="source-language">From</label>
        <select
          id="source-language"
          v-model="sourceLanguage"
          :disabled="targetLanguage.code === sourceLanguage.code"
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

      <div class="language-divider">
        <svg width="24" height="24" viewBox="0 0 24 24" fill="none">
          <path d="M5 12h14m-4-4l4 4-4 4" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      </div>

      <div class="language-select">
        <label for="target-language">To</label>
        <select
          id="target-language"
          v-model="targetLanguage"
          :disabled="sourceLanguage.code === targetLanguage.code"
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
</template>

<style scoped>
.language-selector {
  text-align: center;
}

h2 {
  font-size: 1.25rem;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 1rem;
  letter-spacing: -0.01em;
}

.language-pair {
  display: flex;
  gap: 1rem;
  justify-content: center;
  align-items: flex-end;
  flex-wrap: wrap;
}

.language-select {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  min-width: 200px;
}

.language-divider {
  color: var(--text-secondary);
  display: flex;
  align-items: center;
  margin-bottom: 4px;
}

label {
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--text-secondary);
  text-align: left;
  padding-left: 0.80rem;
}

select {
  appearance: none;
  background-image: url("data:image/svg+xml;charset=UTF-8,%3csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3e%3cpolyline points='6 9 12 15 18 9'%3e%3c/polyline%3e%3c/svg%3e");
  background-repeat: no-repeat;
  background-position: right 0.7rem center;
  background-size: 1em;
  padding: 0.5rem 2.5rem 0.5rem 0.75rem;
  cursor: pointer;
}

select:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

@media (max-width: 640px) {
  .language-pair {
    flex-direction: column;
    gap: 0.5rem;
  }

  .language-divider {
    transform: rotate(90deg);
    padding: 0.5rem;
  }

  .language-select {
    width: 100%;
  }
}
</style> 